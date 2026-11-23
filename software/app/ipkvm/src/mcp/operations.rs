use super::*;
use crate::{console, hardware::hid};
use base64::engine::general_purpose::STANDARD;
use std::sync::atomic::Ordering;
use tokio::time::Instant as Deadline;

fn ms(value: &Value, key: &str, fallback: u64) -> Duration {
    Duration::from_millis(value[key].as_u64().unwrap_or(fallback))
}

pub(super) fn valid(name: &str, args: &Value) -> bool {
    match name {
        "type_text" => args["text"].as_str().is_some_and(|s| s.len() <= 16 * 1024),
        "press_keys" => {
            let codes: Option<Vec<_>> = args["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|key| hid::keycode(key.as_str()?))
                .collect();
            codes.is_some_and(|codes| codes.iter().filter(|code| **code < 224).count() <= 6)
        }
        "screenshot" | "screen_wait" => args.get("region").is_none_or(|region| {
            region["x"].as_f64().unwrap() + region["w"].as_f64().unwrap() <= 1.0 + 1e-12
                && region["y"].as_f64().unwrap() + region["h"].as_f64().unwrap() <= 1.0 + 1e-12
        }),
        "host_drop_file" => {
            let name = args["name"].as_str().unwrap();
            let content = args["content"].as_str().unwrap();
            let components_valid = name.split('/').all(|part| {
                part != "."
                    && part != ".."
                    && part.len() <= 255
                    && !part.chars().any(char::is_control)
            });
            components_valid
                && if args["encoding"] == "base64" {
                    STANDARD
                        .decode(content)
                        .is_ok_and(|bytes| bytes.len() <= 512 * 1024)
                } else {
                    content.len() <= 512 * 1024
                }
        }
        _ => true,
    }
}

pub(super) async fn call(
    app: &App,
    key: &Key,
    name: &str,
    args: &Value,
) -> Result<Output, ApiError> {
    match name {
        "type_text" | "press_keys" | "move_mouse" | "click_mouse" | "drag_mouse"
        | "scroll_mouse" | "power" | "boot_watch" | "enter_firmware" | "wake_on_lan"
        | "virtual_media" | "host_drop_file" | "share" => {
            let app = app.clone();
            let key = key.clone();
            let name = name.to_owned();
            let args = args.clone();
            // A disconnected HTTP peer must not abandon cleanup of a held key or mounted LUN.
            tokio::spawn(async move { control(&app, &key, &name, &args).await })
                .await
                .map_err(io::Error::other)?
        }
        _ => {
            let permit = app.mcp.permit(app, false)?;
            observe::call(app, name, args, &permit).await
        }
    }
}

async fn control(app: &App, key: &Key, name: &str, args: &Value) -> Result<Output, ApiError> {
    let input_tool = matches!(
        name,
        "type_text"
            | "press_keys"
            | "move_mouse"
            | "click_mouse"
            | "drag_mouse"
            | "scroll_mouse"
            | "enter_firmware"
    );
    let permit = app.mcp.permit(app, input_tool)?;
    match name {
        "type_text" | "press_keys" | "move_mouse" | "click_mouse" | "drag_mouse"
        | "scroll_mouse" => {
            let _input = gate(&app.mcp.input, &permit).await?;
            release_raw(app).await;
            begin(app).await?;
            let result = input(app, name, args, &permit).await;
            let cleanup = finish(app).await;
            permit.check().and(result).and(cleanup)?;
            Ok(Output::new(json!({"ok":true})))
        }
        "power" => {
            let action = args["action"].as_str().unwrap();
            if let Some(confirm) = confirm_power(app, key, name, args, action)? {
                return Ok(Output::new(confirm));
            }
            let _power = gate(&app.mcp.power, &permit).await?;
            do_power(app, action, &permit, None).await?;
            let reader = app.clone();
            let state = tokio::task::spawn_blocking(move || observe::power_state(&reader))
                .await
                .map_err(io::Error::other)?;
            Ok(Output::new(json!({"power":state})))
        }
        "boot_watch" => {
            let start = args["start"].as_str().unwrap_or("none");
            if args["retries"].as_u64().unwrap_or(0) > 0 {
                if let Some(confirm) = app.mcp.confirm(
                    key,
                    name,
                    args,
                    Value::Null,
                    "Boot retries can force the host off and turn it back on.",
                )? {
                    return Ok(Output::new(confirm));
                }
            } else if let Some(confirm) = confirm_power(app, key, name, args, start)? {
                return Ok(Output::new(confirm));
            }
            let total = Duration::from_secs(args["timeout_s"].as_u64().unwrap_or(120));
            let deadline = Deadline::now() + total;
            let _power = tokio::select! {
                guard = gate(&app.mcp.power,&permit) => guard?,
                _ = tokio::time::sleep_until(deadline) => return Err(ApiError::new(409,"timeout")),
            };
            boot(app, args, &permit, deadline).await
        }
        "enter_firmware" => {
            let start = args["start"].as_str().unwrap();
            let config = app.config();
            let cfg = &config["host"];
            let target = if args["target"] == "setup" {
                "setup_key"
            } else {
                "boot_menu_key"
            };
            let code = cfg[target]
                .as_str()
                .filter(|code| hid::keycode(code).is_some())
                .ok_or_else(|| ApiError::new(409, "not_configured"))?;
            if let Some(confirm) = confirm_power(app, key, name, args, start)? {
                return Ok(Output::new(confirm));
            }
            let _power = gate(&app.mcp.power, &permit).await?;
            let _input = gate(&app.mcp.input, &permit).await?;
            release_raw(app).await;
            if app.applied_usb()["keyboard"] != true {
                return Err(ApiError::new(409, "keyboard_disabled"));
            }
            do_power(app, start, &permit, None).await?;
            begin(app).await?;
            let result = firmware(app, cfg, code, &permit).await;
            let cleanup = finish(app).await;
            permit.check().and(cleanup)?;
            result
        }
        "wake_on_lan" => {
            let mac = if let Some(name) = args["name"].as_str() {
                console::saved_mac(app, name).await?
            } else {
                console::mac(args["mac"].as_str().unwrap())
                    .ok_or_else(|| ApiError::new(400, "bad_mac"))?
            };
            permit.check()?;
            console::wake_mac(app, mac).await?;
            Ok(Output::new(json!({"ok":true})))
        }
        "virtual_media" | "host_drop_file" | "share" => {
            let _media = gate(&app.mcp.media, &permit).await?;
            let result = media(app, key, name, args, &permit).await;
            // The transition runs to completion or rolls back even when stopped.
            permit.check()?;
            result.map(Output::new)
        }
        _ => Err(ApiError::new(404, "not_found")),
    }
}

async fn gate<'a>(
    lock: &'a AsyncMutex<()>,
    permit: &Permit,
) -> Result<tokio::sync::MutexGuard<'a, ()>, ApiError> {
    let guard = tokio::select! {
        biased;
        error = permit.interrupted() => return Err(error),
        guard = lock.lock() => guard,
    };
    permit.check()?;
    Ok(guard)
}

async fn report(app: &App, permit: &Permit, event: Value) -> Result<(), ApiError> {
    permit.check()?;
    let result = console::input(app, &event, || permit.running()).await;
    permit.check().and(result)
}
pub(super) async fn begin(app: &App) -> Result<(), ApiError> {
    let hw = app.hw.clone();
    let shared = !app.system.snapshot().mcp_input_policy.human_input_priority;
    tokio::task::spawn_blocking(move || hw.begin_mcp_input(shared))
        .await
        .map_err(io::Error::other)?
        .map_err(ApiError::device)
}

pub(super) async fn finish(app: &App) -> Result<(), ApiError> {
    let hw = app.hw.clone();
    tokio::task::spawn_blocking(move || hw.finish_mcp_input())
        .await
        .map_err(io::Error::other)?
        .map_err(ApiError::device)
}

pub(super) async fn release_raw(app: &App) {
    let active = std::mem::take(&mut *app.mcp.raw_input.lock().unwrap());
    if active.is_some() {
        let _ = finish(app).await;
    }
}

fn absolute_point(value: &Value, buttons: u8) -> Value {
    let scale = |field: &str| (65535.0 * value[field].as_f64().unwrap()).round() as u16;
    json!({"t":"mouse_abs","x":scale("x"),"y":scale("y"),"buttons":buttons})
}

fn require_abs(app: &App) -> Result<(), ApiError> {
    if app.applied_usb()["mouse_abs"] != true {
        return Err(ApiError::new(409, "abs_mouse_disabled"));
    }
    Ok(())
}

fn button_mask(args: &Value) -> u8 {
    match args["button"].as_str().unwrap() {
        "left" => 1,
        "right" => 2,
        _ => 4,
    }
}

async fn buttons(app: &App, permit: &Permit, mask: u8, absolute: bool) -> Result<(), ApiError> {
    permit.check()?;
    let hw = app.hw.clone();
    let result = tokio::task::spawn_blocking(move || hw.mouse_buttons(mask, absolute))
        .await
        .map_err(io::Error::other)?;
    permit.check()?;
    result.map_err(ApiError::device)
}

async fn input(app: &App, name: &str, args: &Value, permit: &Permit) -> Result<(), ApiError> {
    let xyz = args.get("x").is_some();
    let usb = app.applied_usb();
    match name {
        "type_text" => {
            let input = args["text"].as_str().unwrap();
            let cfg = app.config();
            let layout = cfg["hid"]["keyboard_layout"].as_str().unwrap_or("us");
            if crate::hardware::layout::strokes(layout, input, false).is_err() {
                let chars: BTreeSet<_> = input
                    .chars()
                    .filter(|c| {
                        crate::hardware::layout::strokes(layout, &c.to_string(), false).is_err()
                    })
                    .collect();
                return Err(
                    ApiError::new(422, "unmappable").with_detail(json!({"characters":chars}))
                );
            }
            report(app, permit, json!({"t":"paste","text":input})).await?;
        }
        "press_keys" => {
            let keys = args["keys"].as_array().unwrap();
            for key in keys {
                report(app, permit, json!({"t":"key","code":key,"down":true})).await?;
            }
            permit.sleep(ms(args, "hold_ms", 50)).await?;
            for key in keys.iter().rev() {
                report(app, permit, json!({"t":"key","code":key,"down":false})).await?;
            }
        }
        "move_mouse" => {
            if args["mode"] == "relative" {
                if usb["mouse_rel"] != true {
                    return Err(ApiError::new(409, "rel_mouse_disabled"));
                }
                let (mut dx, mut dy) = (args["dx"].as_i64().unwrap(), args["dy"].as_i64().unwrap());
                while dx != 0 || dy != 0 {
                    let (x, y) = (dx.clamp(-127, 127), dy.clamp(-127, 127));
                    report(
                        app,
                        permit,
                        json!({"t":"mouse_rel","dx":x,"dy":y,"buttons":0}),
                    )
                    .await?;
                    dx -= x;
                    dy -= y;
                }
            } else {
                require_abs(app)?;
                report(app, permit, absolute_point(args, 0)).await?;
            }
        }
        "click_mouse" => {
            if xyz {
                require_abs(app)?;
                report(app, permit, absolute_point(args, 0)).await?;
            }
            let absolute = xyz || usb["mouse_rel"] != true;
            if absolute {
                require_abs(app)?;
            }
            for click in 0..args["clicks"].as_u64().unwrap_or(1) {
                if click > 0 {
                    permit.sleep(Duration::from_millis(30)).await?;
                }
                buttons(app, permit, button_mask(args), absolute).await?;
                permit.sleep(Duration::from_millis(25)).await?;
                buttons(app, permit, 0, absolute).await?;
            }
        }
        "drag_mouse" => {
            require_abs(app)?;
            let from = &args["from"];
            let to = &args["to"];
            let steps = args["steps"].as_u64().unwrap_or(20) as u32;
            let time = ms(args, "duration_ms", 300);
            let mask = button_mask(args);
            report(app, permit, absolute_point(from, 0)).await?;
            report(app, permit, absolute_point(from, mask)).await?;
            for step in 1..=steps {
                permit.sleep(time / steps).await?;
                let position = |component: &str| {
                    let start = from[component].as_f64().unwrap();
                    start + (to[component].as_f64().unwrap() - start) * step as f64 / steps as f64
                };
                report(
                    app,
                    permit,
                    absolute_point(&json!({"x":position("x"),"y":position("y")}), mask),
                )
                .await?;
            }
            report(app, permit, absolute_point(to, 0)).await?;
        }
        "scroll_mouse" => {
            if xyz {
                require_abs(app)?;
                report(app, permit, absolute_point(args, 0)).await?;
            }
            if !xyz && usb["mouse_rel"] != true {
                require_abs(app)?;
            }
            let mut remaining = args["dy"].as_i64().unwrap();
            while remaining != 0 {
                let dy = remaining.clamp(-127, 127);
                report(app, permit, json!({"t":"wheel","dy":dy})).await?;
                remaining -= dy;
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn confirm_power(
    app: &App,
    key: &Key,
    tool: &str,
    args: &Value,
    action: &str,
) -> Result<Option<Value>, ApiError> {
    let summary = match action {
        "force_off" => "Force the host off by holding its power button for five seconds.",
        "reset" => "Reset the host using the physical reset button.",
        "cycle" => "Force the host off, wait five seconds, then turn it on.",
        _ => return Ok(None),
    };
    app.mcp.confirm(key, tool, args, Value::Null, summary)
}

async fn pulse(
    app: &App,
    button: &'static str,
    milliseconds: u64,
    expected_power: Option<bool>,
    permit: &Permit,
    deadline: Option<Deadline>,
) -> Result<(), ApiError> {
    permit.check()?;
    let control = app.clone();
    let signal = Permit {
        stop: permit.stop.clone(),
        human: permit.human.clone(),
    };
    let result = tokio::task::spawn_blocking(move || {
        console::pulse_cancellable(&control, button, milliseconds, expected_power, || {
            !signal.running() || deadline.is_some_and(|at| Deadline::now() >= at)
        })
    })
    .await
    .map_err(io::Error::other)?;
    permit.check()?;
    if deadline.is_some_and(|at| Deadline::now() >= at) {
        return Err(ApiError::new(409, "timeout"));
    }
    result
}

fn sim_boot(app: &App, boot: bool) {
    if !app.sim {
        return;
    }
    app.console.sim_signal.store(false, Ordering::Relaxed);
    app.console.sim_pc_connected.store(false, Ordering::Relaxed);
    let generation = app
        .console
        .sim_session
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    if boot {
        let app = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if app.console.sim_session.load(Ordering::Relaxed) == generation {
                app.console.sim_stable.store(true, Ordering::Relaxed);
                app.console.sim_signal.store(true, Ordering::Relaxed);
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
            if app.console.sim_session.load(Ordering::Relaxed) == generation {
                app.console.sim_pc_connected.store(true, Ordering::Relaxed);
            }
        });
    }
}

async fn do_power(
    app: &App,
    action: &str,
    permit: &Permit,
    deadline: Option<Deadline>,
) -> Result<(), ApiError> {
    match action {
        "on" => {
            pulse(app, "power", 200, Some(false), permit, deadline).await?;
            sim_boot(app, true);
        }
        "shutdown" => {
            // A short press on an off system would start it.
            pulse(app, "power", 200, Some(true), permit, deadline).await?;
            sim_boot(app, false);
        }
        "press" => {
            pulse(app, "power", 200, None, permit, deadline).await?;
            let on = observe::power_state(app) == "on";
            sim_boot(app, on);
        }
        "reset" => {
            pulse(app, "reset", 200, None, permit, deadline).await?;
            sim_boot(app, true);
        }
        "force_off" | "cycle" => {
            pulse(app, "power", 5000, None, permit, deadline).await?;
            sim_boot(app, false);
            if action == "cycle" {
                let delay = Duration::from_secs(5);
                if deadline.is_some_and(|at| Deadline::now() + delay >= at) {
                    return Err(ApiError::new(409, "timeout"));
                }
                permit.sleep(delay).await?;
                pulse(app, "power", 200, Some(false), permit, deadline).await?;
                sim_boot(app, true);
            }
        }
        "none" => {}
        _ => unreachable!(),
    }
    Ok(())
}

async fn firmware(app: &App, cfg: &Value, code: &str, permit: &Permit) -> Result<Output, ApiError> {
    if app.applied_usb()["keyboard"] != true {
        return Err(ApiError::new(409, "keyboard_disabled"));
    }
    let start = Instant::now();
    let duration = Duration::from_secs(cfg["tap_duration_s"].as_u64().unwrap_or(15));
    let interval = ms(cfg, "tap_interval_ms", 200);
    let mut signal_seen = None;
    while start.elapsed() < duration
        && signal_seen.is_none_or(|when: Instant| when.elapsed() < Duration::from_secs(3))
    {
        report(app, permit, json!({"t":"key","code":code,"down":true})).await?;
        permit.sleep(Duration::from_millis(20)).await?;
        report(app, permit, json!({"t":"key","code":code,"down":false})).await?;
        if signal_seen.is_none() && hdmi_signal(app).await? {
            signal_seen = Some(Instant::now());
        }
        permit
            .sleep(interval.saturating_sub(Duration::from_millis(20)))
            .await?;
    }
    with_screenshot(app, json!({}), permit, None).await
}

async fn hdmi_signal(app: &App) -> Result<bool, ApiError> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || console::video::hdmi(&app)["signal"] == true)
        .await
        .map_err(io::Error::other)
        .map_err(Into::into)
}

async fn boot(
    app: &App,
    args: &Value,
    permit: &Permit,
    deadline: Deadline,
) -> Result<Output, ApiError> {
    let mut action = args["start"].as_str().unwrap_or("none");
    let until = args["until"].as_str().unwrap_or("os");
    let retries = args["retries"].as_u64().unwrap_or(0);
    let budget = deadline.saturating_duration_since(Deadline::now());
    let mut rounds = Vec::new();
    let mut result = "no_power";
    let mut last_frame = None;
    for round in 0..=retries {
        let round_start = Instant::now();
        let mut timeline = json!({"power_on_ms":null,"signal_ms":null,"stable_ms":null,"telemetry_ms":null,"host_agent_ms":null});
        let round_limit = (Deadline::now() + budget / (retries + 1) as u32).min(deadline);
        let old_agent = app.mcp.host.session(app);
        let old_telemetry = app.mcp.metrics.session(app);
        let mut signal_went_down = action == "none" || !hdmi_signal(app).await?;
        if round > 0 {
            if let Err(error) = do_power(app, "force_off", permit, Some(round_limit)).await {
                if error.code != "timeout" {
                    return Err(error);
                }
                rounds.push(timeline);
                break;
            }
            signal_went_down |= !hdmi_signal(app).await?;
            action = "on";
        }
        if let Err(error) = do_power(app, action, permit, Some(round_limit)).await {
            if error.code != "timeout" {
                return Err(error);
            }
            rounds.push(timeline);
            break;
        }
        let action_time = Instant::now();
        let mut frames = console::video::subscribe(app, true).await;
        frames.rx.mark_changed();
        let mut reference = None::<Vec<u8>>;
        let mut stable_since = None::<Instant>;
        loop {
            permit.check()?;
            let step = round_start.elapsed().as_millis() as u64;
            let board = app.clone();
            let (power, signal) = tokio::task::spawn_blocking(move || {
                (
                    observe::power_state(&board),
                    console::video::hdmi(&board)["signal"] == true,
                )
            })
            .await
            .map_err(io::Error::other)?;
            if power == "on" && timeline["power_on_ms"].is_null() {
                timeline["power_on_ms"] = step.into();
            }
            if !signal {
                signal_went_down = true;
                reference = None;
                stable_since = None;
            } else if signal_went_down
                && (timeline["signal_ms"].is_null() || timeline["signal_ms"] == "unknown")
            {
                timeline["signal_ms"] = step.into();
            } else if !signal_went_down
                && action_time.elapsed() >= Duration::from_secs(10)
                && timeline["signal_ms"].is_null()
            {
                timeline["signal_ms"] = "unknown".into();
            }
            if signal
                && (signal_went_down || timeline["signal_ms"] == "unknown")
                && frames.rx.has_changed().unwrap_or(false)
            {
                let frame = frames.rx.borrow_and_update().clone();
                if let console::video::Frame::Jpeg(jpeg) = frame {
                    last_frame = Some(jpeg.clone());
                    if timeline["stable_ms"].is_null() {
                        let luma = tokio::task::spawn_blocking(move || {
                            image::difference_frame(&jpeg, None)
                        })
                        .await
                        .map_err(io::Error::other)??;
                        let stable = reference.as_ref().is_some_and(|base| {
                            base.len() == luma.len()
                                && luma
                                    .iter()
                                    .zip(base)
                                    .filter(|(a, b)| a.abs_diff(**b) > 8)
                                    .count() as f64
                                    <= luma.len() as f64 * 0.005
                        });
                        if !stable {
                            stable_since = Some(Instant::now());
                            reference = Some(luma);
                        } else if stable_since
                            .is_some_and(|when| when.elapsed() >= Duration::from_millis(1500))
                        {
                            timeline["stable_ms"] = step.into();
                        }
                    }
                }
            }
            let agent = app.mcp.host.session(app);
            let telemetry = app.mcp.metrics.session(app);
            if timeline["host_agent_ms"].is_null()
                && agent.is_some()
                && (action == "none" || agent != old_agent)
            {
                timeline["host_agent_ms"] = step.into();
            }
            if timeline["telemetry_ms"].is_null()
                && telemetry.is_some()
                && (action == "none" || telemetry != old_telemetry)
            {
                timeline["telemetry_ms"] = step.into();
            }
            let os = !timeline["telemetry_ms"].is_null() || !timeline["host_agent_ms"].is_null();
            let met = match until {
                "signal" => timeline["signal_ms"].is_number(),
                "stable" => !timeline["stable_ms"].is_null(),
                _ => os,
            };
            if met {
                result = "ok";
                break;
            }
            result = if timeline["power_on_ms"].is_null() {
                "no_power"
            } else if timeline["signal_ms"].is_null() || until == "signal" {
                "no_signal"
            } else if until == "stable" {
                "stuck"
            } else {
                "os_timeout"
            };
            if Deadline::now() >= round_limit {
                break;
            }
            permit
                .sleep(
                    Duration::from_millis(100)
                        .min(round_limit.saturating_duration_since(Deadline::now())),
                )
                .await?;
        }
        rounds.push(timeline);
        if result == "ok" || Deadline::now() >= deadline {
            break;
        }
    }
    let mut structured = json!({"result":result,"rounds":rounds});
    if let Some(jpeg) = last_frame {
        permit.check()?;
        let frame = observe::rendered(jpeg, None, u16::MAX.into(), None).await?;
        structured["screenshot"] = frame.structured;
        Ok(Output {
            structured,
            content: frame.content,
        })
    } else {
        with_screenshot(app, structured, permit, Some(deadline)).await
    }
}

async fn with_screenshot(
    app: &App,
    mut structured: Value,
    permit: &Permit,
    deadline: Option<Deadline>,
) -> Result<Output, ApiError> {
    let capture = tokio::select! {
        biased;
        error = permit.interrupted() => return Err(error),
        result = async {
            if let Some(at) = deadline { tokio::time::timeout_at(at,observe::screenshot(app,&json!({}))).await.ok() }
            else { Some(observe::screenshot(app,&json!({})).await) }
        } => result,
    };
    if let Some(Ok(frame)) = capture {
        structured["screenshot"] = frame.structured;
        Ok(Output {
            structured,
            content: frame.content,
        })
    } else {
        structured["screenshot"] = Value::Null;
        Ok(Output::new(structured))
    }
}

async fn media(
    app: &App,
    key: &Key,
    tool: &str,
    args: &Value,
    permit: &Permit,
) -> Result<Value, ApiError> {
    let action = args["action"].as_str().unwrap_or("");
    match (tool, action) {
        ("virtual_media", "list") => crate::storage::list_media(app.clone()).await,
        ("virtual_media", "status") => Ok(app.storage.mcp_media_status()),
        ("virtual_media", "mount") => crate::storage::mount_media(app.clone(), args.clone()).await,
        ("virtual_media", "eject") => {
            let mounted = app.storage.mcp_media_status();
            let confirmed = (mounted["readonly"] == false && observe::usb_connected(app))
                || args.get("confirm_token").is_some();
            if confirmed {
                let summary = "Eject the writable mounted image while USB is attached; unsaved host writes may be lost.";
                if let Some(request) = app.mcp.confirm(key, tool, args, mounted.clone(), summary)? {
                    return Ok(request);
                }
            }
            permit.check()?;
            let app = app.clone();
            tokio::task::spawn_blocking(move || {
                app.storage
                    .mcp_eject(&mounted, confirmed, || observe::usb_connected(&app))?;
                app.emit("status_changed");
                Ok(app.storage.mcp_media_status())
            })
            .await
            .map_err(io::Error::other)?
        }
        ("host_drop_file", _) => {
            let content = args["content"].as_str().unwrap();
            let bytes = if args["encoding"] == "base64" {
                STANDARD.decode(content).unwrap()
            } else {
                content.as_bytes().to_vec()
            };
            crate::storage::drop_file(
                app.clone(),
                args["name"].as_str().unwrap().into(),
                bytes,
                args["export"].as_bool().unwrap_or(true),
            )
            .await
        }
        ("share", "status") => Ok(json!({"exported_to_host":app.storage.share_state().0})),
        ("share", "export") => crate::storage::set_share_state(app.clone(), true, None).await,
        ("share", "reclaim") => {
            let (exported, generation) = app.storage.share_state();
            if !exported {
                return Ok(json!({"exported_to_host":false}));
            }
            let secure = args.get("confirm_token").is_none() && {
                // Once the host may be changing the LUN, complete the reclaim handshake.
                let (_running, stop) = watch::channel(Interrupt::Running);
                let finish_reclaim = Permit { stop, human: None };
                app.mcp
                    .host
                    .call(app, "eject", &finish_reclaim)
                    .await
                    .is_ok()
            };
            if !secure {
                permit.check()?;
                let summary = "Reclaim the shared disk; unsaved host writes may be lost.";
                if let Some(request) =
                    app.mcp
                        .confirm(key, tool, args, json!(generation), summary)?
                {
                    return Ok(request);
                }
            }
            crate::storage::set_share_state(app.clone(), false, Some(generation)).await
        }
        _ => Err(ApiError::new(400, "invalid_body")),
    }
}
