//! 聊天室客户端
//!
//! 基于 egui 的图形化客户端

use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("chat_client=debug".parse()?))
        .init();

    // TODO: 实现客户端 GUI

    println!("Chat Client - TODO: implement egui GUI");

    Ok(())
}
