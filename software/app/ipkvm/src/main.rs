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
    kvmd::system::listeners::serve(app.clone(), router(app, &web), args.listen).await
}
