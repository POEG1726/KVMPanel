//! Local board telemetry remains available without a PC telemetry session.

use crate::{console, system, App};
use serde_json::{json, Value};
use std::{fs, time::Instant};

#[derive(Default)]
pub(super) struct BoardSampler {
    cpu: Option<(u64, u64)>,
    eth: Option<(Instant, u64, u64)>,
}

impl BoardSampler {
    fn cpu_percent(&mut self) -> f64 {
        let current = fs::read_to_string("/proc/stat").ok().and_then(|text| {
            let line = text.lines().next()?.strip_prefix("cpu ")?;
            // Do not count guest time twice. It is already included in user/nice.
            let mut fields = line.split_whitespace().take(8).map(str::parse::<u64>);
            let mut total = 0u64;
            let mut idle = 0u64;
            for i in 0..8 {
                let value = fields.next()?.ok()?;
                total = total.checked_add(value)?;
                if i == 3 || i == 4 {
                    idle += value;
                }
            }
            Some((total, idle))
        });
        let previous = std::mem::replace(&mut self.cpu, current);
        match (previous, current) {
            (Some((old_total, old_idle)), Some((total, idle)))
                if total > old_total && idle >= old_idle =>
            {
                let delta = total - old_total;
                100. * delta.saturating_sub(idle - old_idle) as f64 / delta as f64
            }
            _ => 0.,
        }
    }

    fn ethernet_rate(&mut self, now: Instant, connected: bool) -> (f64, f64) {
        let bytes = |way| {
            fs::read_to_string(format!("/sys/class/net/eth0/statistics/{way}_bytes"))
                .ok()?
                .trim()
                .parse::<u64>()
                .ok()
        };
        let current = connected
            .then(|| Some((now, bytes("rx")?, bytes("tx")?)))
            .flatten();
        let previous = std::mem::replace(&mut self.eth, current);
        match (previous, current) {
            (Some((last, old_rx, old_tx)), Some((now, rx, tx)))
                if now > last && rx >= old_rx && tx >= old_tx =>
            {
                let seconds = (now - last).as_secs_f64();
                (
                    8. * (rx - old_rx) as f64 / seconds,
                    8. * (tx - old_tx) as f64 / seconds,
                )
            }
            _ => (0., 0.),
        }
    }

    pub(super) fn sample(&mut self, app: &App, info: &Value, now: Instant, ts: u64) -> Value {
        let pc_power = match console::panel_pc_power(app)
            .ok()
            .and_then(|v| v["on"].as_bool())
        {
            Some(true) => "on",
            Some(false) => "off",
            None => "unknown",
        };
        let link = info["link_mbps"].as_u64().unwrap_or(0);
        let (cpu, rate, duplex, sd, tail) = if app.sim {
            (
                12.,
                (130_000., 42_000.),
                Some("full".to_owned()),
                Some(info["data"].clone()),
                None,
            )
        } else {
            let duplex = fs::read_to_string("/sys/class/net/eth0/duplex")
                .ok()
                .map(|mode| mode.trim().to_ascii_lowercase())
                .filter(|mode| matches!(mode.as_str(), "full" | "half") && link != 0);
            (
                self.cpu_percent(),
                self.ethernet_rate(now, link != 0),
                duplex,
                app.storage.sd_usage(),
                system::tailscale_ip(),
            )
        };
        let network = app.system.network();
        let (scheme, key, default) = if network["https"]["enabled"] == true {
            ("https", &network["https"]["port"], 443)
        } else {
            ("http", &network["http_port"], 80)
        };
        let hostname = info["hostname"].as_str().unwrap_or("kvmpanel");
        let port = key.as_u64().unwrap_or(default);
        let suffix = if port == default {
            String::new()
        } else {
            format!(":{port}")
        };
        let applied = app.applied_usb();
        let bound = app.hw.usb_bound().unwrap_or(false);
        let functions: Vec<_> = [
            "keyboard",
            "mouse_rel",
            "mouse_abs",
            "storage",
            "net",
            "acm",
            "telemetry",
            "microphone",
        ]
        .into_iter()
        .filter(|name| bound && applied[*name] == true)
        .collect();
        let mut kvm = console::panel_video(app);
        kvm["usb_functions"] = json!(functions);
        kvm["microphone_status"] = json!(app.microphone_status(bound, &applied));
        json!({"t":"board","ts":ts,"pc_power":pc_power,"hostname":hostname,
            "version":info["version"],"uptime_s":info["uptime_s"],"cpu_pct":cpu,
            "soc_temp_c":info["soc_temp_c"],"mem":info["mem"],"sd":sd,
            "eth":{"iface":"eth0","link_mbps":link,"duplex":duplex,"mac":info["mac"],
                   "rx_bps":rate.0,"tx_bps":rate.1},
            "access":{"url":format!("{scheme}://{hostname}.local{suffix}"),"tailscale_ip":tail},"kvm":kvm})
    }
}
