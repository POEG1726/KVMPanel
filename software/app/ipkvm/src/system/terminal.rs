use super::{bad_body, missing};
use crate::{ApiError, App};
use axum::{
    body::Bytes,
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, Write},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::fs::{FileTypeExt, OpenOptionsExt},
    },
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{unix::AsyncFd, AsyncReadExt},
    process::{Child, Command},
};

pub(super) fn routes() -> Router<App> {
    Router::new()
        .route("/api/terminal", get(upgrade))
        .route("/api/scripts", get(list))
        .route("/api/scripts/{name}", put(store).delete(remove))
        .route("/api/scripts/{name}/run", post(run))
}

#[derive(Deserialize)]
struct TerminalQuery {
    kind: Option<String>,
}

async fn upgrade(
    State(app): State<App>,
    headers: HeaderMap,
    Query(q): Query<TerminalQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    // Subscribe before checking the cookie so changes racing the handshake are observed.
    let events = app.events.subscribe();
    let closes = app.socket_closes.subscribe();
    ws.on_upgrade(move |socket| terminal(socket, app, headers, events, closes, q.kind))
}

fn ioctl_winsize(fd: RawFd, cols: u16, rows: u16) -> io::Result<()> {
    let size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &size) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// When the last websocket is closed, promptly release its child and any processes
// still using the controlling PTY. kill_on_drop additionally reaps the shell.
struct ProcessGroup(i32);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}
struct Terminal {
    file: AsyncFd<File>,
    _process: Option<(ProcessGroup, Child)>,
}

impl Terminal {
    fn local(app: &App) -> io::Result<Self> {
        // posix_openpt sets CLOEXEC at creation, even with other shells spawning concurrently.
        let pt = unsafe {
            libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC | libc::O_NONBLOCK)
        };
        if pt < 0 {
            return Err(io::Error::last_os_error());
        }
        let master = unsafe { File::from_raw_fd(pt) };
        if unsafe { libc::grantpt(pt) } < 0 || unsafe { libc::unlockpt(pt) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut number = 0u32;
        if unsafe { libc::ioctl(pt, libc::TIOCGPTN, &mut number) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let slave = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(format!("/dev/pts/{number}"))?;
        ioctl_winsize(pt, 80, 24)?;
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-i")
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        if app.sim {
            let dir = app.data_dir.join("shell");
            std::fs::create_dir_all(dir.join("images"))?;
            cmd.current_dir(dir)
                .env("PS1", "root@kvmpanel:~# ")
                .env_remove("ENV");
        }
        cmd.env("TERM", "xterm-256color").kill_on_drop(true);
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn()?;
        Ok(Self {
            // The File owns this descriptor until AsyncFd drops it.
            file: unsafe { AsyncFd::register(master) }?,
            _process: Some((ProcessGroup(child.id().unwrap() as i32), child)),
        })
    }

    fn serial(path: &str, baud: u64) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
            .open(path)?;
        if !file.metadata()?.file_type().is_char_device()
            || unsafe { libc::isatty(file.as_raw_fd()) } != 1
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "serial device must be a tty",
            ));
        }
        let speed = match baud {
            50 => libc::B50,
            75 => libc::B75,
            110 => libc::B110,
            300 => libc::B300,
            600 => libc::B600,
            1200 => libc::B1200,
            2400 => libc::B2400,
            4800 => libc::B4800,
            9600 => libc::B9600,
            19200 => libc::B19200,
            38400 => libc::B38400,
            57600 => libc::B57600,
            115200 => libc::B115200,
            230400 => libc::B230400,
            460800 => libc::B460800,
            921600 => libc::B921600,
            1500000 => libc::B1500000,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unsupported serial speed",
                ))
            }
        };
        let fd = file.as_raw_fd();
        unsafe {
            let mut tty = std::mem::MaybeUninit::uninit();
            if libc::tcgetattr(fd, tty.as_mut_ptr()) < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut tty = tty.assume_init();
            libc::cfmakeraw(&mut tty);
            tty.c_cflag |= libc::CLOCAL | libc::CREAD;
            if libc::cfsetispeed(&mut tty, speed) < 0
                || libc::cfsetospeed(&mut tty, speed) < 0
                || libc::tcsetattr(fd, libc::TCSANOW, &tty) < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self {
            // The File owns this descriptor until AsyncFd drops it.
            file: unsafe { AsyncFd::register(file) }?,
            _process: None,
        })
    }
    async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.file
            .async_io(tokio::io::Interest::READABLE, |f| (&*f).read(buf))
            .await
    }
    async fn write(&self, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            let n = self
                .file
                .async_io(tokio::io::Interest::WRITABLE, |f| (&*f).write(buf))
                .await?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::WriteZero));
            }
            buf = &buf[n..];
        }
        Ok(())
    }
}

async fn close(ws: &mut WebSocket, code: u16) {
    let _ = tokio::time::timeout(
        Duration::from_secs(1),
        ws.send(Message::Close(Some(CloseFrame {
            code,
            reason: "".into(),
        }))),
    )
    .await;
}

async fn terminal(
    mut ws: WebSocket,
    app: App,
    headers: HeaderMap,
    mut events: tokio::sync::broadcast::Receiver<String>,
    mut closes: tokio::sync::watch::Receiver<u16>,
    kind: Option<String>,
) {
    if let Some(code) = app.socket_code(&headers, "terminal", "") {
        close(&mut ws, code).await;
        return;
    }
    let cfg = app.config();
    let port = match kind.as_deref().unwrap_or("local") {
        "local" => Terminal::local(&app),
        "serial" => match cfg["serial"]["device"].as_str() {
            None => {
                close(&mut ws, 4404).await;
                return;
            }
            Some(_) if app.sim => Terminal::local(&app),
            Some(path) => Terminal::serial(path, cfg["serial"]["baud"].as_u64().unwrap_or(115200)),
        },
        _ => {
            close(&mut ws, 4400).await;
            return;
        }
    };
    let Ok(port) = port else {
        close(&mut ws, 1011).await;
        return;
    };
    let mut buf = [0u8; 8192];
    // The I/O future stays cancelable even when the peer stops reading or the PTY
    // buffers fill. Revocation must terminate an idle or backpressured shell.
    let io = async {
        loop {
            tokio::select! {
                msg=ws.recv()=>match msg {
                    Some(Ok(Message::Binary(b)))=>port.write(&b).await?,
                    Some(Ok(Message::Text(s)))=>{
                        if let Ok(size)=serde_json::from_str::<Resize>(&s) {
                            if size.t=="resize" && size.cols>0 && size.rows>0 {ioctl_winsize(port.file.get_ref().as_raw_fd(),size.cols,size.rows)?;}
                        }
                    }
                    Some(Ok(Message::Close(_)))|Some(Err(_))|None=>break,
                    _=>{}
                },
                result=port.read(&mut buf)=> {
                    let n=result?;
                    if n==0 {break;}
                    if ws.send(Message::Binary(Bytes::copy_from_slice(&buf[..n]))).await.is_err(){break;}
                },
            }
        }
        Ok::<_, io::Error>(())
    };
    let code = tokio::select! {
        biased;
        _=closes.changed()=>*closes.borrow_and_update(),
        code=async{
            loop{
                match events.recv().await{
                    Ok(event)=>if let Some(c)=app.socket_code(&headers,"terminal",&event){break c;},
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>if let Some(c)=app.socket_code(&headers,"terminal",""){break c;},
                    Err(_)=>break 1012,
                }
            }
        }=>code,
        _=io=>1000,
    };
    close(&mut ws, code).await;
}
#[derive(Deserialize)]
struct Resize {
    t: String,
    cols: u16,
    rows: u16,
}

fn check_name(name: &str) -> Result<(), ApiError> {
    if (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        Ok(())
    } else {
        Err(ApiError::new(400, "bad_name"))
    }
}
async fn list(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "terminal")?;
    let s = app.system.snapshot();
    Ok(Json(json!(s
        .scripts
        .iter()
        .map(|(name, body)| json!({"name":name,"size":body.len()}))
        .collect::<Vec<_>>())))
}
async fn store(
    State(app): State<App>,
    h: HeaderMap,
    Path(name): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    app.require(&h, "terminal")?;
    check_name(&name)?;
    if super::mime(&h) != "text/plain" {
        return Err(bad_body(""));
    }
    let script = String::from_utf8(body.to_vec()).map_err(|_| bad_body(""))?;
    let info = json!({"name":name,"size":body.len()});
    app.system
        .change(move |s| {
            s.scripts.insert(name, script);
            Ok(())
        })
        .await?;
    Ok((StatusCode::CREATED, Json(info)))
}
async fn remove(
    State(app): State<App>,
    h: HeaderMap,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.require(&h, "terminal")?;
    check_name(&name)?;
    app.system
        .change(move |s| s.scripts.remove(&name).ok_or_else(missing))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn run(
    State(app): State<App>,
    h: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "terminal")?;
    check_name(&name)?;
    let text = app
        .system
        .snapshot()
        .scripts
        .get(&name)
        .cloned()
        .ok_or_else(missing)?;
    // Read source from a file descriptor, not an interpolated command line.
    let dir = app.data_dir.clone();
    let source = tokio::task::spawn_blocking(move || -> io::Result<_> {
        let mut source = tempfile::tempfile_in(dir)?;
        writeln!(source, "exec 2>&1")?;
        source.write_all(text.as_bytes())?;
        source.rewind()?;
        Ok(source)
    })
    .await
    .map_err(io::Error::other)??;
    let mut cmd = Command::new("/bin/sh");
    cmd.current_dir(&app.data_dir)
        .stdin(source)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .process_group(0);
    let mut child = cmd.spawn()?;
    let group = ProcessGroup(child.id().unwrap() as i32);
    let mut stdout = child.stdout.take().unwrap();
    let mut output = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(60), async {
        let mut chunk = [0u8; 8192];
        loop {
            let n = stdout.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            // Drain until exit, keeping the response below the board memory budget.
            let room = (4 * 1024 * 1024usize).saturating_sub(output.len());
            output.extend_from_slice(&chunk[..n.min(room)]);
        }
        child.wait().await
    })
    .await;
    drop(group);
    let exit = match result {
        Ok(status) => {
            use std::os::unix::process::ExitStatusExt;
            let code = status?;
            code.code()
                .unwrap_or_else(|| 128 + code.signal().unwrap_or(0))
        }
        Err(_) => {
            let _ = child.wait().await;
            124
        }
    };
    Ok(Json(
        json!({"exit_code":exit,"output":String::from_utf8_lossy(&output)}),
    ))
}
