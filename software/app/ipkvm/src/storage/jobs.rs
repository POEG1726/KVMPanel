use super::*;
use axum::body::Bytes;
use reqwest::{Client, Url};
use std::time::{Duration, Instant};
const FINISHED_LIMIT: usize = 20;
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
    finished: Option<Instant>,
}

impl Job {
    fn json(&self) -> Value {
        json!({"id":self.id,"url":self.url,"path":self.path,"state":*self.state.borrow(),"received":self.received,"total":self.total})
    }
}

pub(super) async fn list(State(app): State<App>, h: HeaderMap) -> Reply {
    app.require(&h, "media")?;
    blocking(move || {
        let state = app.storage.state.lock().unwrap();
        Ok(ok(
            json!({"jobs":state.jobs.iter().map(Job::json).collect::<Vec<_>>()}),
        ))
    })
    .await
}

pub(super) async fn control(
    State(app): State<App>,
    h: HeaderMap,
    RoutePath((id, action)): RoutePath<(String, String)>,
) -> Reply {
    app.require(&h, "media")?;
    let cancel = action == "cancel";
    let (reply, mut stopped) = blocking(move || {
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
        Ok((job.json(), job.stopped.clone()))
    })
    .await?;
    if cancel {
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
    let tx = AsyncTransfer::begin(app.clone(), t, 0, true).await?;
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
        finished: None,
        total: if app.storage.sim { 64 << 20 } else { 0 },
    };
    let reply = job.json();
    let s = app.storage.clone();
    blocking(move || {
        s.state.lock().unwrap().jobs.push(job);
        Ok(())
    })
    .await?;
    let checksum = b["sha256"].as_str().map(str::to_owned);
    tokio::spawn(async move {
        let mut worker = Worker {
            storage: app.storage.clone(),
            tx,
            id,
            rx,
        };
        let result = worker.download(url, checksum).await;
        tokio::task::spawn_blocking(move || {
            // Release the target reservation and remove the staging file before
            // acknowledging cancellation, so a retry can reuse the same path.
            drop(worker.tx.0.take());
            let mut state = worker.storage.state.lock().unwrap();
            let job = state.jobs.iter_mut().find(|j| j.id == worker.id).unwrap();
            if result.is_err() && *job.state.borrow() != "canceled" {
                job.state.send_replace("failed");
            }
            job.finished = Some(Instant::now());
            if state.jobs.iter().filter(|j| j.finished.is_some()).count() > FINISHED_LIMIT {
                let oldest = state
                    .jobs
                    .iter()
                    .enumerate()
                    .filter_map(|(i, j)| j.finished.map(|at| (at, i)))
                    .min()
                    .unwrap()
                    .1;
                state.jobs.remove(oldest);
            }
            done.send_replace(true);
        })
        .await
        .expect("download cleanup");
    });
    Ok((StatusCode::ACCEPTED, Json(json!({"job":reply}))).into_response())
}

struct Worker {
    tx: AsyncTransfer,
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

fn download_client() -> reqwest::Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
}

impl Worker {
    async fn download(&mut self, url: Url, checksum: Option<String>) -> Result<(), ()> {
        if self.storage.sim {
            // Deterministic offline image as in the mock. Work in bounded blocks
            // so simulation also exercises checksums, cancellation and free space.
            let block = Bytes::from(vec![0; 256 * 1024]);
            for _ in 0..256 {
                active(&mut self.rx, tokio::time::sleep(Duration::from_millis(8))).await?;
                self.append(block.clone()).await?;
            }
        } else {
            let client = download_client().map_err(|_| ())?;
            let mut res = active(&mut self.rx, client.get(url).send())
                .await?
                .map_err(|_| ())?
                .error_for_status()
                .map_err(|_| ())?;
            let length = res.content_length().unwrap_or(0);
            let storage = self.storage.clone();
            let id = self.id.clone();
            blocking(move || {
                let free = storage.volumes["sd"].capacity().map_err(error)?.1;
                let mut state = storage.state.lock().unwrap();
                state.jobs.iter_mut().find(|j| j.id == id).unwrap().total = length;
                if length > free {
                    return Err(ApiError::new(507, "no_space"));
                }
                Ok(())
            })
            .await
            .map_err(|_| ())?;
            while let Some(bytes) = active(&mut self.rx, res.chunk()).await?.map_err(|_| ())? {
                self.append(bytes).await?;
            }
        }
        self.perform(move |tx, job| {
            tx.finish(checksum.as_deref(), true)?;
            job.state.send_replace("done");
            Ok(())
        })
        .await
    }

    async fn append(&mut self, data: Bytes) -> Result<(), ()> {
        self.perform(move |tx, job| {
            tx.write_locked(&data)?;
            job.received += data.len() as u64;
            Ok(())
        })
        .await
    }

    async fn perform(
        &mut self,
        f: impl Fn(&mut Transfer, &mut Job) -> Result<(), ApiError> + Send + Sync + 'static,
    ) -> Result<(), ()> {
        let f = Arc::new(f);
        loop {
            active(&mut self.rx, std::future::ready(())).await?;
            let writer = self.tx.writer();
            let storage = self.storage.clone();
            let id = self.id.clone();
            let f = f.clone();
            let ran = blocking(move || {
                let mut state = storage.state.lock().unwrap();
                let job = state.jobs.iter_mut().find(|j| j.id == id).unwrap();
                if *job.state.borrow() != "running" {
                    return Ok(false);
                }
                f(&mut writer.lock().unwrap(), job)?;
                Ok(true)
            })
            .await
            .map_err(|_| ())?;
            if ran {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn idle_response_times_out_after_sixty_seconds() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/image", l.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut sock, _) = l.accept().await.unwrap();
            let mut bytes = [0u8; 1024];
            sock.read(&mut bytes).await.unwrap();
            sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\na")
                .await
                .unwrap();
            std::future::pending::<()>().await;
        });
        let mut res = download_client().unwrap().get(url).send().await.unwrap();
        assert_eq!(res.chunk().await.unwrap().unwrap().as_ref(), b"a");
        tokio::time::pause();
        let mut next = Box::pin(res.chunk());
        // Start the per-read timer, then leave the remote socket stalled.
        assert!(futures_util::poll!(next.as_mut()).is_pending());
        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(futures_util::poll!(next.as_mut()).is_pending());
        tokio::time::advance(Duration::from_secs(2)).await;
        assert!(next.await.unwrap_err().is_timeout());
        task.abort();
    }
}
