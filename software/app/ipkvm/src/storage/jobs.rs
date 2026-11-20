use super::*;
use reqwest::{Client, Url};
use std::time::Duration;
use tokio::sync::watch;
use uuid::Uuid;

pub(super) struct Job {
    id: String,
    url: String,
    path: String,
    state: watch::Sender<&'static str>,
    stopped: watch::Receiver<bool>,
    received: u64,
    total: u64,
}

impl Job {
    fn json(&self) -> Value {
        json!({"id":self.id,"url":self.url,"path":self.path,"state":*self.state.borrow(),"received":self.received,"total":self.total})
    }
}

pub(super) async fn list(State(app): State<App>, h: HeaderMap) -> Reply {
    app.require(&h, "media")?;
    let state = app.storage.state.lock().unwrap();
    Ok(ok(
        json!({"jobs":state.jobs.iter().map(Job::json).collect::<Vec<_>>()}),
    ))
}

pub(super) async fn control(
    State(app): State<App>,
    h: HeaderMap,
    RoutePath((id, action)): RoutePath<(String, String)>,
) -> Reply {
    app.require(&h, "media")?;
    let (reply, mut stopped) = {
        let state = app.storage.state.lock().unwrap();
        let job = state
            .jobs
            .iter()
            .find(|j| j.id == id)
            .ok_or_else(|| ApiError::new(404, "not_found"))?;
        let next = match (action.as_str(), *job.state.borrow()) {
            ("pause", "running") => "paused",
            ("resume", "paused") => "running",
            ("cancel", "paused" | "running") => "canceled",
            ("pause" | "resume" | "cancel", _) => return Err(ApiError::new(409, "bad_state")),
            _ => return Err(ApiError::new(404, "not_found")),
        };
        job.state.send_replace(next);
        (job.json(), job.stopped.clone())
    };
    if action == "cancel" {
        let _ = stopped.wait_for(|v| *v).await;
    }
    Ok(ok(reply))
}

pub(super) async fn start(State(app): State<App>, h: HeaderMap, JsonBody(b): JsonBody) -> Reply {
    app.require(&h, "media")?;
    validate("startDownload", &b)?;
    let url = Url::parse(vstr(&b, "url"))
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https") && u.has_host())
        .ok_or_else(|| ApiError::new(400, "bad_url"))?;
    let t = Target::new("sd", vstr(&b, "path"))?;
    let display = t.displayed();
    let tx = Transfer::begin(&app, t, 0, true)?;
    let id = Uuid::new_v4().to_string();
    let (state, rx) = watch::channel("running");
    let (done, stopped) = watch::channel(false);
    let job = Job {
        id: id.clone(),
        url: vstr(&b, "url").into(),
        path: display,
        state,
        stopped,
        received: 0,
        total: if app.storage.sim { 64 << 20 } else { 0 },
    };
    let reply = job.json();
    app.storage.state.lock().unwrap().jobs.push(job);
    let checksum = b["sha256"].as_str().map(str::to_owned);
    tokio::spawn(async move {
        let mut worker = Worker {
            storage: app.storage.clone(),
            tx,
            id,
            rx,
        };
        let result = worker.download(url, checksum).await;
        if result.is_err() {
            let mut state = worker.storage.state.lock().unwrap();
            let j = state.jobs.iter_mut().find(|j| j.id == worker.id).unwrap();
            if *j.state.borrow() != "canceled" {
                j.state.send_replace("failed");
            }
        }
        drop(worker);
        done.send_replace(true);
    });
    Ok((StatusCode::ACCEPTED, Json(json!({"job":reply}))).into_response())
}

struct Worker {
    tx: Transfer,
    id: String,
    storage: std::sync::Arc<Storage>,
    rx: watch::Receiver<&'static str>,
}

// Keep the pending network read alive across pause/resume. Cancellation drops
// the HTTP response and staging file even when the remote server has stalled.
async fn active<F: std::future::Future>(
    rx: &mut watch::Receiver<&'static str>,
    f: F,
) -> Result<F::Output, ()> {
    tokio::pin!(f);
    loop {
        let st = *rx.borrow_and_update();
        match st {
            "running" => tokio::select! {biased;
                changed=rx.changed()=>changed.map_err(|_|())?,
                output=&mut f=>return Ok(output),
            },
            "paused" => rx.changed().await.map_err(|_| ())?,
            _ => return Err(()),
        }
    }
}

impl Worker {
    async fn download(&mut self, url: Url, checksum: Option<String>) -> Result<(), ()> {
        if self.storage.sim {
            // Deterministic offline image as in the mock. Work in bounded blocks
            // so simulation also exercises checksums, cancellation and free space.
            let block = vec![0; 256 * 1024];
            for _ in 0..256 {
                active(&mut self.rx, tokio::time::sleep(Duration::from_millis(8))).await?;
                self.append(&block).await?;
            }
        } else {
            let client = Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .build()
                .map_err(|_| ())?;
            let mut res = active(&mut self.rx, client.get(url).send())
                .await?
                .map_err(|_| ())?
                .error_for_status()
                .map_err(|_| ())?;
            let length = res.content_length().unwrap_or(0);
            {
                let mut state = self.storage.state.lock().unwrap();
                state
                    .jobs
                    .iter_mut()
                    .find(|j| j.id == self.id)
                    .unwrap()
                    .total = length;
            }
            if length > self.storage.volumes["sd"].capacity().map_err(|_| ())?.1 {
                return Err(());
            }
            while let Some(bytes) = active(&mut self.rx, res.chunk()).await?.map_err(|_| ())? {
                self.append(&bytes).await?;
            }
        }
        loop {
            active(&mut self.rx, std::future::ready(())).await?;
            let mut state = self.storage.state.lock().unwrap();
            let job = state.jobs.iter_mut().find(|j| j.id == self.id).unwrap();
            if *job.state.borrow() == "running" {
                self.tx.finish(checksum.as_deref(), true).map_err(|_| ())?;
                job.state.send_replace("done");
                return Ok(());
            }
        }
    }
    async fn append(&mut self, data: &[u8]) -> Result<(), ()> {
        loop {
            active(&mut self.rx, std::future::ready(())).await?;
            let mut state = self.storage.state.lock().unwrap();
            let job = state.jobs.iter_mut().find(|j| j.id == self.id).unwrap();
            if *job.state.borrow() == "running" {
                self.tx.write_locked(data).map_err(|_| ())?;
                job.received += data.len() as u64;
                return Ok(());
            }
        }
    }
}
