use super::*;

const MAX_REPLY: usize = 16 * 1024;
const IDLE: Duration = Duration::from_secs(2);
const TOTAL: Duration = Duration::from_secs(10);

#[derive(Default)]
struct AgentState {
    caps: BTreeSet<String>,
    last_seen: Option<Instant>,
    session: Option<u64>,
    next: u16,
    pending: Option<Transfer>,
}

struct Transfer {
    seq: u16,
    part: usize,
    total: Option<usize>,
    bytes: Vec<u8>,
    feed: tokio::sync::mpsc::Sender<Option<Vec<u8>>>,
}

#[derive(Default)]
pub(crate) struct HostAgent {
    state: Mutex<AgentState>,
    gate: AsyncMutex<()>,
}

impl HostAgent {
    fn online(state: &AgentState) -> bool {
        !state.caps.is_empty()
            && state
                .last_seen
                .is_some_and(|at| at.elapsed() < Duration::from_secs(7))
    }
    pub(crate) fn status(&self, app: &App) -> Value {
        if app.sim {
            let online = app
                .console
                .sim_pc_connected
                .load(std::sync::atomic::Ordering::Relaxed);
            return json!({"online":online,"capabilities":if online {vec!["info","clipboard","eject"]}else{vec![]}});
        }
        let state = self.state.lock().unwrap();
        let online = Self::online(&state);
        json!({"online":online,"capabilities":if online {state.caps.clone()}else{BTreeSet::new()}})
    }

    pub(crate) fn session(&self, app: &App) -> Option<u64> {
        if app.sim {
            return app
                .console
                .sim_pc_connected
                .load(std::sync::atomic::Ordering::Relaxed)
                .then(|| {
                    app.console
                        .sim_session
                        .load(std::sync::atomic::Ordering::Relaxed)
                });
        }
        let state = self.state.lock().unwrap();
        Self::online(&state).then_some(state.session).flatten()
    }

    pub(crate) fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        let next = state.next;
        *state = AgentState {
            next,
            ..AgentState::default()
        };
    }

    pub(crate) fn heartbeat(&self, session: u64) {
        let mut state = self.state.lock().unwrap();
        if state.session != Some(session) {
            state.caps.clear();
            state.pending = None;
        }
        state.session = Some(session);
        state.last_seen = Some(Instant::now());
    }

    pub(crate) fn packet(&self, app: &App, raw: &[u8]) {
        if raw.first() != Some(&3) {
            return;
        }
        let body = &raw[1..];
        let body = &body[..body.iter().position(|b| *b == 0).unwrap_or(body.len())];
        if let Some(caps) = body.strip_prefix(b"^caps=") {
            if let Ok(text) = std::str::from_utf8(caps) {
                let mut state = self.state.lock().unwrap();
                state.caps = text
                    .split(',')
                    .filter(|cap| matches!(*cap, "info" | "clipboard" | "eject"))
                    .map(str::to_owned)
                    .collect();
                state.last_seen = Some(Instant::now());
            }
            return;
        }
        let Some((seq, part, total, data)) = parse_part(body) else {
            return;
        };
        let (feed, result) = {
            let mut state = self.state.lock().unwrap();
            let Some(current) = state.pending.as_mut() else {
                return;
            };
            if current.seq != seq || current.total.is_some_and(|previous| total != previous) {
                return;
            }
            if part + 1 == current.part {
                let _ = send(app, &format!("!{seq}:{part}"));
                return;
            }
            if current.part != part || current.bytes.len() + data.len() > MAX_REPLY {
                return;
            }
            if send(app, &format!("!{seq}:{part}")).is_err() {
                return;
            }
            current.total = Some(total);
            current.part += 1;
            current.bytes.extend_from_slice(data);
            let result = (part == total).then(|| std::mem::take(&mut current.bytes));
            let feed = current.feed.clone();
            state.last_seen = Some(Instant::now());
            (feed, result)
        };
        let _ = feed.try_send(result);
    }

    pub(super) async fn call(
        &self,
        app: &App,
        method: &str,
        permit: &Permit,
    ) -> Result<Value, ApiError> {
        let _gate = tokio::select! {
            guard = self.gate.lock() => guard,
            error = permit.interrupted() => return Err(error),
        };
        permit.check()?;
        if app.sim {
            if self.status(app)["online"] != true {
                return Err(ApiError::new(503, "host_agent_offline"));
            }
            return Ok(match method {
                "info" => {
                    json!({"foreground":{"title":"KVMPanel simulated host","process":"sim.exe"},"cursor":{"x":0.5,"y":0.5},"displays":[{"id":"sim","name":"HDMI","x":0,"y":0,"width":1920,"height":1080,"scale":1.0,"primary":true,"captured":true}],"session":"unlocked"})
                }
                "clipboard" => json!({"text":"KVMPanel simulated clipboard","truncated":false}),
                _ => json!({}),
            });
        }
        let (tx, rx) = tokio::sync::mpsc::channel(384);
        let seq = {
            let mut state = self.state.lock().unwrap();
            if !Self::online(&state) {
                return Err(ApiError::new(503, "host_agent_offline"));
            }
            if !state.caps.contains(method) {
                return Err(ApiError::new(403, "host_feature_disabled"));
            }
            let seq = state.next;
            state.next = state.next.wrapping_add(1);
            state.pending = Some(Transfer {
                seq,
                part: 1,
                total: None,
                bytes: Vec::new(),
                feed: tx,
            });
            seq
        };
        // Clean up even if the client drops the HTTP operation.
        struct Pending<'a>(&'a HostAgent, u16);
        impl Drop for Pending<'_> {
            fn drop(&mut self) {
                let mut state = self.0.state.lock().unwrap();
                if state.pending.as_ref().is_some_and(|p| p.seq == self.1) {
                    state.pending = None;
                }
            }
        }
        let _pending = Pending(self, seq);
        send(app, &format!("?{seq}:{method}")).map_err(ApiError::device)?;
        let raw = receive_reply(rx, permit).await?;
        let value: Value =
            serde_json::from_slice(&raw).map_err(|_| ApiError::new(502, "bad_host_reply"))?;
        if value["ok"] == true && value["data"].is_object() {
            return Ok(value["data"].clone());
        }
        let code = value["error"]
            .as_str()
            .filter(|s| {
                !s.is_empty()
                    && s.len() < 64
                    && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            })
            .unwrap_or("bad_host_reply");
        Err(ApiError::new(409, code))
    }
}

async fn receive_reply(
    mut parts: tokio::sync::mpsc::Receiver<Option<Vec<u8>>>,
    permit: &Permit,
) -> Result<Vec<u8>, ApiError> {
    use tokio::time::Instant;
    let deadline = Instant::now() + TOTAL;
    loop {
        let limit = deadline.min(Instant::now() + IDLE);
        let msg = tokio::select! {
            biased;
            error = permit.interrupted() => return Err(error),
            data = tokio::time::timeout_at(limit, parts.recv()) => data,
        };
        match msg {
            Ok(Some(Some(raw))) => return Ok(raw),
            Ok(Some(None)) => continue,
            Ok(None) => return Err(ApiError::new(503, "host_agent_offline")),
            Err(_) => return Err(ApiError::new(504, "host_agent_timeout")),
        }
    }
}

fn parse_part(body: &[u8]) -> Option<(u16, usize, usize, &[u8])> {
    let content = body.strip_prefix(b"<")?;
    let mut slices = content.splitn(3, |byte| *byte == b':');
    let seq = std::str::from_utf8(slices.next()?)
        .ok()?
        .parse::<u16>()
        .ok()?;
    let (part, total) = std::str::from_utf8(slices.next()?).ok()?.split_once('/')?;
    let (part, total) = (part.parse::<usize>().ok()?, total.parse::<usize>().ok()?);
    let data = slices.next()?;
    (part > 0 && part <= total && total <= 384 && !data.is_empty())
        .then_some((seq, part, total, data))
}

fn send(app: &App, text: &str) -> io::Result<()> {
    let mut packet = [0; 64];
    packet[0] = 4;
    packet[1..1 + text.len()].copy_from_slice(text.as_bytes());
    app.hw.host_write(&packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn idle_and_total_reply_deadlines_are_independent() {
        use tokio::time::Instant;
        let (_running, stop) = watch::channel(Interrupt::Running);
        let permit = Permit { stop, human: None };

        let (_idle_sender, idle_parts) = tokio::sync::mpsc::channel(1);
        let start = Instant::now();
        assert_eq!(
            receive_reply(idle_parts, &permit).await.unwrap_err().code,
            "host_agent_timeout"
        );
        assert_eq!(start.elapsed(), IDLE);

        let (sender, parts) = tokio::sync::mpsc::channel(1);
        let producer = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                if sender.send(None).await.is_err() {
                    break;
                }
            }
        });
        let start = Instant::now();
        assert_eq!(
            receive_reply(parts, &permit).await.unwrap_err().code,
            "host_agent_timeout"
        );
        assert_eq!(start.elapsed(), TOTAL);
        producer.abort();
    }
    #[tokio::test]
    async fn assembles_only_in_order_and_keeps_active_host_online() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(dir.path(), true).unwrap();
        let host = &app.mcp.host;
        let (feed, mut rx) = tokio::sync::mpsc::channel(8);
        host.heartbeat(123);
        host.packet(&app, b"\x03^caps=info,eject");
        {
            let mut state = host.state.lock().unwrap();
            state.last_seen = Some(Instant::now() - Duration::from_secs(6));
            state.pending = Some(Transfer {
                seq: 7,
                part: 1,
                total: None,
                bytes: vec![],
                feed,
            });
        }
        host.packet(&app, b"\x03<7:2/2:lost");
        host.packet(&app, b"\x03<8:1/2:wrong");
        assert!(rx.try_recv().is_err());
        host.packet(&app, b"\x03<7:1/2:{\"text\":\"\xe4");
        assert_eq!(rx.recv().await, Some(None));
        host.packet(&app, b"\x03<7:1/2:{\"text\":\"\xe4");
        assert!(rx.try_recv().is_err());
        host.packet(&app, b"\x03<7:2/2:\xb8\xad\"}");
        let joined = rx.recv().await.unwrap().unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&joined).unwrap()["text"],
            "中"
        );
        assert!(host.state.lock().unwrap().last_seen.unwrap().elapsed() < Duration::from_secs(1));
        host.clear();
        assert_eq!(rx.recv().await, None);
    }

    #[test]
    fn parses_numbered_reports_without_decoding_partial_utf8() {
        assert_eq!(
            parse_part(b"<65535:2/3:\xe4\xb8"),
            Some((65535, 2, 3, b"\xe4\xb8".as_slice()))
        );
        for invalid in [
            b"<1:0/3:x".as_slice(),
            b"<1:2/1:x",
            b"<1:1/0:x",
            b"<65536:1/1:x",
            b"<1:1/1:",
        ] {
            assert!(parse_part(invalid).is_none());
        }
    }
}
