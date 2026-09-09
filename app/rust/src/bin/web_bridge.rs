#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bind =
        std::env::var("MICROSLOP_WEB_BRIDGE_BIND").unwrap_or_else(|_| "127.0.0.1:18444".to_owned());
    ost_frb::web_bridge::run_server(&bind).await
}
