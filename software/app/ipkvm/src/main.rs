use clap::Parser;
use kvmd::{router, App};
use std::{io, path::PathBuf};
#[derive(Parser)]
struct Args {
    #[arg(long)]
    sim: bool,
    #[arg(long, hide = true)]
    verify_update: bool,
    #[arg(long, hide = true, conflicts_with = "verify_update")]
    prepare_update: bool,
    #[arg(long)]
    listen: Option<std::net::SocketAddr>,
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long)]
    web_dir: Option<PathBuf>,
    #[arg(long, env = "KVMPANEL_PANEL_SOCK")]
    panel: Option<String>,
}
#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();
    let dir = args.data_dir.unwrap_or_else(|| {
        if args.sim {
            "./kvmd-sim".into()
        } else {
            "/data/kvmd".into()
        }
    });
    if args.prepare_update {
        return kvmd::system::prepare_update(&dir);
    }
    if args.verify_update {
        return kvmd::system::verify_update(
            &dir,
            args.panel.as_deref().unwrap_or("/run/kvmd/panel.sock"),
        )
        .await;
    }
    std::fs::create_dir_all(&dir)?;
    let directory_lock = std::fs::File::open(&dir)?;
    directory_lock
        .try_lock()
        .map_err(|e| io::Error::other(format!("cannot lock {}: {e}", dir.display())))?;
    let app = App::new(&dir, args.sim)?;
    let web = args.web_dir.unwrap_or_else(|| "/usr/share/kvmd/web".into());
    if !app.sim {
        let board = app.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || board.restore_usb())
            .await
            .map_err(io::Error::other)?
        {
            eprintln!("kvmd: cannot restore USB gadget: {e:?}");
        }
    }
    tokio::spawn(kvmd::console::jiggler(app.clone()));
    tokio::spawn(kvmd::mcp_background(app.clone()));
    tokio::spawn(kvmd::keys::run(app.clone()));
    kvmd::system::start_services(app.clone());
    let panel = args.panel.unwrap_or_else(|| {
        if args.sim {
            "127.0.0.1:7701".into()
        } else {
            "/run/kvmd/panel.sock".into()
        }
    });
    // Keep panel and HTTP lifetimes together; report either bind failure to init.
    let routes = router(app.clone(), &web);
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let stop = async {
        tokio::select! {_=term.recv()=>{}, _=tokio::signal::ctrl_c()=>{}}
        kvmd::mcp_flush_audit(&app);
        app.emit("kvm_restarting");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        Ok(())
    };
    tokio::select! {
        result=stop=>result,
        result = kvmd::panel::serve(app.clone(), &panel) => result,
        result = kvmd::system::listeners::serve(app.clone(), routes, args.listen) => result,
    }
}
