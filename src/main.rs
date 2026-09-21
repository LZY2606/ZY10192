use clap::Parser;
use concordia_jiaotai::{db::AppState, server, FIXTURE_JSON};
use std::{net::SocketAddr, sync::Arc};

#[derive(Parser, Debug)]
#[command(
    name = "concordia-jiaotai",
    version,
    about = "协和线交台：本地 U-Pb 协和线交点分析服务"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:5532")]
    listen: SocketAddr,
    #[arg(long, default_value = "data/concordia.sqlite")]
    database: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if let Some(parent) = std::path::Path::new(&args.database).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let state = Arc::new(AppState::open(&args.database)?);
    state.seed_if_empty(FIXTURE_JSON)?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    println!("协和线交台监听 http://{}", args.listen);
    axum::serve(listener, server::router(state)).await?;
    Ok(())
}
