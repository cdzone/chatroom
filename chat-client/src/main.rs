//! 聊天室客户端
//!
//! 基于 egui 的图形化客户端

mod client;
mod ui;

use anyhow::Result;
use tracing_subscriber::EnvFilter;
use ui::ChatApp;

fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("chat_client=debug".parse()?)
                .add_directive("protocol=debug".parse()?),
        )
        .init();

    // 运行 GUI
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "聊天室",
        options,
        Box::new(|cc| Ok(Box::new(ChatApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}
