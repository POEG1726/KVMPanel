use super::*;

use std::time::Instant;

pub(super) fn hdmi(app: &App) -> Value {
    let absent = || json!({"signal":false,"width":0,"height":0,"fps":0});
    if app.sim && !app.console.sim_signal.load(Ordering::Relaxed) {
        return absent();
    }
    app.hw.hdmi().unwrap_or_else(|_| absent())
}
fn subscribe(app: &App) -> watch::Receiver<Option<Bytes>> {
    let mut frames = app.console.frames.lock().unwrap();
    if let Some(tx) = frames.as_ref() {
        return tx.subscribe();
    }
    let (tx, rx) = watch::channel(None);
    *frames = Some(tx.clone());
    tokio::spawn(produce(app.clone(), tx));
    rx
}

async fn produce(app: App, tx: watch::Sender<Option<Bytes>>) {
    let mut start = Instant::now();
    let (mut count, mut bytes) = (0u64, 0u64);
    loop {
        {
            let mut frames = app.console.frames.lock().unwrap();
            if tx.receiver_count() == 0 {
                app.hw.stop_video();
                *frames = None;
                *app.console.stats.lock().unwrap() = (0, 0);
                return;
            }
        }
        let cfg = app.config();
        let fps = cfg["video"]["fps_max"].as_u64().unwrap_or(30).clamp(1, 60);
        let quality = cfg["video"]["quality"].as_u64().unwrap_or(80) as u8;
        let deadline = tokio::time::Instant::now() + Duration::from_secs_f64(1. / fps as f64);
        let source = app.clone();
        let next = tokio::task::spawn_blocking(move || {
            if source.sim && !source.console.sim_signal.load(Ordering::Relaxed) {
                return None;
            }
            source.hw.frame(quality).ok()
        })
        .await
        .unwrap_or(None);
        if let Some(frame) = next.as_ref() {
            count += 1;
            bytes += frame.len() as u64;
        }
        // Each receiver sees only the newest complete frame, even while its network write is blocked.
        tx.send_replace(next);
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed >= 1. {
            *app.console.stats.lock().unwrap() = (
                (count as f64 / elapsed).round() as u64,
                (bytes as f64 * 8. / 1000. / elapsed).round() as u64,
            );
            count = 0;
            bytes = 0;
            start = Instant::now();
        }
        tokio::time::sleep_until(deadline).await;
    }
}

pub async fn snapshot(app: &App) -> Result<Vec<u8>, ApiError> {
    if app.sim && !app.console.sim_signal.load(Ordering::Relaxed) {
        return Err(ApiError::new(503, "no_signal"));
    }
    let mut frames = subscribe(app);
    if let Some(frame) = frames.borrow_and_update().as_ref() {
        return Ok(frame.to_vec());
    }
    let updated = tokio::time::timeout(Duration::from_secs(5), frames.changed()).await;
    if matches!(updated, Ok(Ok(()))) {
        if let Some(frame) = frames.borrow().as_ref() {
            return Ok(frame.to_vec());
        }
    }
    Err(ApiError::new(503, "no_signal"))
}

pub(super) async fn stream(
    State(app): State<App>,
    ws: WebSocketUpgrade,
    h: HeaderMap,
) -> impl IntoResponse {
    let events = app.events.subscribe();
    ws.on_upgrade(move |sock| stream_socket(sock, app, h, events))
}

async fn stream_socket(
    mut socket: WebSocket,
    app: App,
    h: HeaderMap,
    mut events: broadcast::Receiver<String>,
) {
    if let Some(c) = app.socket_code(&h, "console", "") {
        close(&mut socket, c).await;
        return;
    }
    let mut frames = subscribe(&app);
    // A new receiver must see the cached frame even before the next capture.
    frames.mark_changed();
    let mut last = None;
    let mut paused = false;
    loop {
        tokio::select! {
            biased;
            event=events.recv()=>{
                let event=match event {Ok(s)=>s,Err(broadcast::error::RecvError::Lagged(_))=>{close(&mut socket,4409).await;return;},Err(_)=>return};
                if let Some(code)=app.socket_code(&h,"console",&event){close(&mut socket,code).await;return;}
            },
            msg=socket.recv()=>{
                match msg{
                    None|Some(Err(_))|Some(Ok(Message::Close(_)))=>return,
                    Some(Ok(Message::Text(raw)))=>match serde_json::from_str::<Value>(&raw).ok().as_ref().and_then(|v|v["t"].as_str()) {
                        Some("pause")=>paused=true,
                        Some("resume")=>{paused=false;last=None;frames.mark_changed();},
                        _=>{}
                    },
                    _=>{}
                }
            },
            result=frames.changed()=>{
                if result.is_err(){return;}
                let frame=frames.borrow_and_update().clone();
                let Some(frame)=frame else {last=None;continue};
                if paused || (app.config()["video"]["frame_diff"]==true && last.as_ref()==Some(&frame)){continue;}
                if !matches!(tokio::time::timeout(Duration::from_secs(3),socket.send(Message::Binary(frame.clone()))).await,Ok(Ok(()))) {return;}
                last=Some(frame);
            }
        }
    }
}
