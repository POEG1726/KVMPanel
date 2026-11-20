use clap::Parser;
use kvmd::{router, App};
use std::{io, path::PathBuf};
#[derive(Parser)]
struct Args {
    #[arg(long)]
    sim: bool,
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
    let app = App::new(&dir, args.sim)?;
    let web = args.web_dir.unwrap_or_else(|| "/usr/share/kvmd/web".into());
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
    tokio::select! {
        result = kvmd::panel::serve(app.clone(), &panel) => result,
        result = kvmd::system::listeners::serve(app, routes, args.listen) => result,
    }
}
