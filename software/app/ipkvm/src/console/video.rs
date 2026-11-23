use super::*;
use futures_util::{Sink, SinkExt};

use std::time::Instant;

pub(crate) fn hdmi(app: &App) -> Value {
    let absent = || json!({"signal":false,"width":0,"height":0,"fps":0});
    if app.sim && !app.console.sim_signal.load(Ordering::Relaxed) {
        return absent();
    }
    app.hw.hdmi().unwrap_or_else(|_| absent())
}
#[derive(Clone)]
pub(crate) enum Frame {
    Pending,
    NoSignal,
    DeviceError(std::sync::Arc<io::Error>),
    InternalError,
    Jpeg(Bytes),
}
impl Frame {
    fn captured(result: io::Result<Bytes>) -> Self {
        match result {
            Ok(bytes) => Self::Jpeg(bytes),
            Err(e) if e.kind() == io::ErrorKind::NotConnected => Self::NoSignal,
            Err(e) => Self::DeviceError(e.into()),
        }
    }
    pub(crate) fn snapshot(&self) -> Result<Vec<u8>, ApiError> {
        match self {
            Self::Jpeg(bytes) => Ok(bytes.to_vec()),
            Self::NoSignal => Err(ApiError::new(503, "no_signal")),
            Self::DeviceError(e) => Err(ApiError::device(io::Error::other(e.clone()))),
            Self::InternalError | Self::Pending => Err(ApiError::new(500, "internal")),
        }
    }
}
pub(crate) struct Subscription {
    app: App,
    pub(crate) rx: watch::Receiver<Frame>,
    fast: bool,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if self.fast {
            self.app.console.fast_video.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
pub(crate) async fn subscribe(app: &App, fast: bool) -> Subscription {
    if fast {
        app.console.fast_video.fetch_add(1, Ordering::Relaxed);
    }
    let rx = frame_channel(app).await;
    Subscription {
        app: app.clone(),
        rx,
        fast,
    }
}
async fn frame_channel(app: &App) -> watch::Receiver<Frame> {
    let mut frames = app.console.frames.lock().await;
    if let Some(tx) = frames.as_ref() {
        return tx.subscribe();
    }
    let (tx, rx) = watch::channel(Frame::Pending);
    *frames = Some(tx.clone());
    tokio::spawn(produce(app.clone(), tx));
    rx
}

async fn produce(app: App, tx: watch::Sender<Frame>) {
    let mut start = Instant::now();
    let (mut count, mut bytes) = (0u64, 0u64);
    loop {
        if tx.receiver_count() == 0 {
            let mut frames = app.console.frames.lock().await;
            if tx.receiver_count() == 0 {
                // Keep a new producer from acquiring the camera until the old child has exited.
                let hw = app.hw.clone();
                let _ = tokio::task::spawn_blocking(move || hw.stop_video()).await;
                *frames = None;
                *app.console.stats.lock().unwrap() = (0, 0);
                return;
            }
        }
        let cfg = app.config();
        let fast = app.console.fast_video.load(Ordering::Relaxed) > 0;
        let fps = if fast {
            cfg["video"]["fps_max"].as_u64().unwrap_or(30).clamp(1, 60)
        } else {
            1
        };
        let quality = cfg["video"]["quality"].as_u64().unwrap_or(80) as u8;
        let deadline = tokio::time::Instant::now() + Duration::from_secs_f64(1. / fps as f64);
        let source = app.clone();
        let next = tokio::task::spawn_blocking(move || {
            if source.sim && !source.console.sim_signal.load(Ordering::Relaxed) {
                return Frame::NoSignal;
            }
            if source.sim && source.console.sim_stable.load(Ordering::Relaxed) {
                return Frame::captured(crate::hardware::sim_still(quality));
            }
            Frame::captured(source.hw.frame(quality))
        })
        .await
        .unwrap_or(Frame::InternalError);
        if let Frame::Jpeg(frame) = &next {
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
        if !fast {
            while tokio::time::Instant::now() < deadline
                && tx.receiver_count() > 0
                && app.console.fast_video.load(Ordering::Relaxed) == 0
            {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        } else {
            tokio::time::sleep_until(deadline).await;
        }
    }
}

pub async fn snapshot(app: &App) -> Result<Vec<u8>, ApiError> {
    if app.sim && !app.console.sim_signal.load(Ordering::Relaxed) {
        return Err(ApiError::new(503, "no_signal"));
    }
    let mut frames = subscribe(app, true).await;
    let ready = tokio::time::timeout(
        Duration::from_secs(5),
        frames.rx.wait_for(|frame| !matches!(frame, Frame::Pending)),
    )
    .await;
    match ready {
        Ok(Ok(frame)) => frame.snapshot(),
        Ok(Err(_)) => Err(ApiError::new(500, "internal")),
        Err(_) => Err(ApiError::device(io::ErrorKind::TimedOut.into())),
    }
}

pub(super) async fn stream(
    State(app): State<App>,
    ws: WebSocketUpgrade,
    h: HeaderMap,
) -> impl IntoResponse {
    let closes = app.socket_closes.subscribe();
    let events = app.events.subscribe();
    ws.on_upgrade(move |sock| stream_socket(sock, app, h, events, closes))
}

async fn send_frame<S: Sink<Message> + Unpin>(socket: &mut S, frame: Bytes) -> bool {
    match tokio::time::timeout(Duration::from_secs(3), socket.send(Message::Binary(frame))).await {
        Ok(result) => result.is_ok(),
        Err(_) => {
            let close = Message::Close(Some(CloseFrame {
                code: 1011,
                reason: "video send timed out".into(),
            }));
            // Attempt the close handshake even if the previous frame is still flushing.
            // A peer that never reads must not keep a capture subscription forever.
            let _ = tokio::time::timeout(Duration::from_secs(1), socket.send(close)).await;
            false
        }
    }
}

async fn stream_socket(
    mut socket: WebSocket,
    app: App,
    h: HeaderMap,
    mut events: broadcast::Receiver<String>,
    mut closes: watch::Receiver<u16>,
) {
    if let Some(c) = app.socket_code(&h, "console", "") {
        close(&mut socket, c).await;
        return;
    }
    let mut frames = subscribe(&app, true).await;
    // A new receiver must see the cached frame even before the next capture.
    frames.rx.mark_changed();
    let mut last = None;
    let mut paused = false;
    loop {
        tokio::select! {
            biased;
            changed=closes.changed()=>{
                if changed.is_err(){return;}
                let code=*closes.borrow_and_update();
                close(&mut socket,code).await;
                return;
            }
            event=events.recv()=>{
                match event{
                    Ok(event) if event=="sessions_changed"=>{},
                    Ok(_)=>continue,
                    Err(broadcast::error::RecvError::Lagged(_))=>{events=events.resubscribe();last=None;frames.rx.mark_changed();},
                    Err(broadcast::error::RecvError::Closed)=>return,
                }
                if let Some(code)=app.socket_code(&h,"console",""){close(&mut socket,code).await;return;}
            },
            msg=socket.recv()=>{
                match msg{
                    None|Some(Err(_))|Some(Ok(Message::Close(_)))=>return,
                    Some(Ok(Message::Text(raw)))=>match serde_json::from_str::<Value>(&raw).ok().as_ref().and_then(|v|v["t"].as_str()) {
                        Some("pause")=>paused=true,
                        Some("resume")=>{paused=false;last=None;frames.rx.mark_changed();},
                        _=>{}
                    },
                    _=>{}
                }
            },
            result=frames.rx.changed()=>{
                if result.is_err(){return;}
                let frame=frames.rx.borrow_and_update().clone();
                let Frame::Jpeg(frame)=frame else {last=None;continue};
                if paused || (app.config()["video"]["frame_diff"]==true && last.as_ref()==Some(&frame)){continue;}
                if !send_frame(&mut socket,frame.clone()).await{return;}
                last=Some(frame);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_distinguishes_missing_hardware_from_no_signal() {
        for (error, code) in [
            (io::ErrorKind::NotFound, "device_unavailable"),
            (io::ErrorKind::PermissionDenied, "device_unavailable"),
            (io::ErrorKind::NotConnected, "no_signal"),
        ] {
            let result = Frame::captured(Err(error.into())).snapshot().unwrap_err();
            assert_eq!((result.status, result.code.as_str()), (503, code));
        }
        assert_eq!(Frame::InternalError.snapshot().unwrap_err().status, 500);
    }
    #[tokio::test(start_paused = true)]
    async fn a_slow_video_send_closes_with_1011() {
        let sent = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink = futures_util::sink::unfold(sent.clone(), |log, msg| async move {
            if matches!(msg, Message::Binary(_)) {
                tokio::time::sleep(Duration::from_millis(3100)).await;
            }
            log.lock().unwrap().push(msg);
            Ok::<_, io::Error>(log)
        });
        futures_util::pin_mut!(sink);
        assert!(!send_frame(&mut sink, Bytes::from_static(b"frame")).await);
        let log = sent.lock().unwrap();
        assert!(
            matches!(log.as_slice(),[Message::Binary(_),Message::Close(Some(close))] if close.code==1011)
        );
    }
}
