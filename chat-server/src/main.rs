//! 聊天室服务端
//!
//! 基于 Tokio 的异步 TCP 服务器

use anyhow::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("chat_server=debug".parse()?))
        .init();

    info!("Chat Server starting...");

    // TODO: 实现服务端逻辑

    Ok(())
}
