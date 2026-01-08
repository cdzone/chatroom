//! 聊天室服务端
//!
//! 基于 Tokio 的异步 TCP 服务器

mod server;

use anyhow::Result;
use server::ChatServer;
use tracing::info;
use tracing_subscriber::EnvFilter;

const DEFAULT_ADDR: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("chat_server=debug".parse()?)
                .add_directive("protocol=debug".parse()?),
        )
        .init();

    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ADDR.to_string());

    info!("Chat Server starting on {}", addr);

    let server = ChatServer::new();
    server.run(&addr).await?;

    Ok(())
}
