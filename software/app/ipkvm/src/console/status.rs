//! One hardware status sampler shared by all console clients.
use super::*;
use std::{sync::Arc, time::Instant};

const STATUS_PERIOD: Duration = Duration::from_millis(100);
const HEARTBEAT: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct PowerCache {
    sample: Mutex<Option<(Instant, io::Result<Value>)>>,
}
impl PowerCache {
    pub fn read(&self, read: impl FnOnce() -> io::Result<Value>) -> io::Result<Value> {
        // Called on the blocking pool. Serialize concurrent clients' two gpioget reads.
        let mut cache = self.sample.lock().unwrap();
        if !cache
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < HEARTBEAT)
        {
            let power = read();
            *cache = Some((Instant::now(), power));
        }
        cache
            .as_ref()
            .unwrap()
            .1
            .as_ref()
            .cloned()
            .map_err(|e| io::Error::new(e.kind(), e.to_string()))
    }
    pub fn invalidate(&self) {
        *self.sample.lock().unwrap() = None;
    }
}

type Snapshot = Option<Arc<Value>>;
#[derive(Default)]
pub(super) struct StatusFeed {
    sender: Mutex<Option<watch::Sender<Snapshot>>>,
}

impl StatusFeed {
    pub fn subscribe(&self, app: &App) -> watch::Receiver<Snapshot> {
        let mut sender = self.sender.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            return tx.subscribe();
        }
        let (tx, rx) = watch::channel(None);
        *sender = Some(tx.clone());
        tokio::spawn(publish(app.clone(), tx, app.events.subscribe()));
        rx
    }
}

fn sample(app: App) -> Value {
    let hdmi = video::hdmi(&app);
    let power = app
        .console
        .power_cache
        .read(|| app.hw.power())
        .unwrap_or_else(|_| json!({"on":false}));
    let locks = app
        .hw
        .locks()
        .unwrap_or_else(|_| json!({"num":false,"caps":false,"scroll":false}));
    let (fps, kbps) = if hdmi["signal"] != true {
        (0, 0)
    } else if app.sim {
        (
            28.min(app.config()["video"]["fps_max"].as_u64().unwrap_or(30)),
            24800,
        )
    } else {
        *app.console.stats.lock().unwrap()
    };
    let sessions: Vec<_> = app
        .console
        .sessions
        .lock()
        .unwrap()
        .entries
        .values()
        .cloned()
        .collect();
    json!({"t":"status","hdmi":hdmi,"stream":{"fps":fps,"kbps":kbps},"power":power,"locks":locks,"sessions":sessions,"media":app.media_status()})
}

async fn publish(app: App, tx: watch::Sender<Snapshot>, mut events: broadcast::Receiver<String>) {
    let mut dirty = true;
    let mut refresh = tokio::time::Instant::now();
    let mut tick = tokio::time::interval(STATUS_PERIOD);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _=tx.closed()=>{
                // subscribe() can race the last client's disconnect.
                let mut sender=app.console.status_feed.sender.lock().unwrap();
                if tx.receiver_count()==0 {*sender=None;return;}
            }
            _=tick.tick()=>{
                if !dirty && tokio::time::Instant::now()<refresh {continue;}
                let sampler=app.clone();
                let Ok(status)=tokio::task::spawn_blocking(move||sample(sampler)).await else{return;};
                tx.send_replace(Some(Arc::new(status)));
                dirty=false;
                refresh=tokio::time::Instant::now()+HEARTBEAT;
            }
            event=events.recv()=>match event {
                Ok(event)=>dirty |= matches!(event.as_str(),"status_changed"|"console_sessions"|"config_changed"|"sessions_changed"),
                Err(broadcast::error::RecvError::Lagged(_))=>dirty=true,
                Err(broadcast::error::RecvError::Closed)=>return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn leds_read_once_until_invalidated() {
        let cache = PowerCache::default();
        assert_eq!(
            cache.read(|| Ok(json!({"on":true}))).unwrap()["on"],
            true
        );
        assert_eq!(
            cache.read(|| panic!("cached LED sample was lost")).unwrap()["on"],
            true
        );
        cache.invalidate();
        assert!(cache.read(|| Err(io::ErrorKind::NotFound.into())).is_err());
        assert!(cache
            .read(|| panic!("missing GPIO was polled twice"))
            .is_err());
    }
}
