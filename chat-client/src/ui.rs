//! egui 界面实现

use eframe::egui;

use crate::client::{ChatClient, ConnectionState};

/// 聊天室应用
pub struct ChatApp {
    client: ChatClient,
    /// 是否自动滚动到底部
    auto_scroll: bool,
}

impl ChatApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 加载中文字体
        setup_fonts(&cc.egui_ctx);

        Self {
            client: ChatClient::new(),
            auto_scroll: true,
        }
    }
}

/// 配置中文字体
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 加载系统中文字体（macOS）
    // 优先尝试苹方字体，其次是华文黑体
    let font_paths = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
    ];

    let mut font_loaded = false;
    for path in &font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "chinese".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );

            // 将中文字体添加到所有字体族的首位
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "chinese".to_owned());

            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "chinese".to_owned());

            font_loaded = true;
            break;
        }
    }

    if !font_loaded {
        tracing::warn!("Failed to load Chinese font, Chinese characters may not display correctly");
    }

    ctx.set_fonts(fonts);
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 轮询网络事件，只在有新事件时请求重绘
        let has_events = self.client.poll_events();
        if has_events {
            ctx.request_repaint();
        } else {
            // 没有事件时，定时重绘以检查新消息（降低 CPU 占用）
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // 顶部面板：连接状态
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("聊天室");
                ui.separator();

                match &self.client.state {
                    ConnectionState::Disconnected => {
                        ui.label("未连接");
                    }
                    ConnectionState::Connecting => {
                        ui.spinner();
                        ui.label("连接中...");
                    }
                    ConnectionState::Connected { username, .. } => {
                        ui.label(format!("已连接 - {}", username));
                    }
                }
            });
        });

        // 底部面板：输入框
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            if self.client.is_connected() {
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.client.input_text)
                            .hint_text("输入消息...")
                            .desired_width(ui.available_width() - 80.0),
                    );

                    // 按 Enter 发送
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.client.send_message();
                        response.request_focus();
                    }

                    if ui.button("发送").clicked() {
                        self.client.send_message();
                    }
                });
            } else {
                // 登录界面
                ui.horizontal(|ui| {
                    ui.label("服务器:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.client.server_addr)
                            .desired_width(150.0),
                    );

                    ui.label("用户名:");
                    let username_response = ui.add(
                        egui::TextEdit::singleline(&mut self.client.username)
                            .desired_width(100.0),
                    );

                    let can_connect = !self.client.username.is_empty()
                        && !self.client.server_addr.is_empty()
                        && matches!(self.client.state, ConnectionState::Disconnected);

                    // 按 Enter 连接
                    if username_response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && can_connect
                    {
                        self.client.connect();
                    }

                    if ui
                        .add_enabled(can_connect, egui::Button::new("连接"))
                        .clicked()
                    {
                        self.client.connect();
                    }

                    if matches!(self.client.state, ConnectionState::Connecting) {
                        ui.spinner();
                    }
                });

                if let Some(err) = &self.client.error_message {
                    ui.colored_label(egui::Color32::RED, err);
                }
            }
        });

        // 中间区域：消息列表
        egui::CentralPanel::default().show(ctx, |ui| {
            // 断开按钮
            if self.client.is_connected() {
                ui.horizontal(|ui| {
                    if ui.button("断开连接").clicked() {
                        self.client.disconnect();
                    }
                    ui.checkbox(&mut self.auto_scroll, "自动滚动");
                });
                ui.separator();
            }

            // 消息滚动区域
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(self.auto_scroll)
                .show(ui, |ui| {
                    for msg in &self.client.messages {
                        if msg.is_system {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&msg.content)
                                        .italics()
                                        .color(egui::Color32::GRAY),
                                );
                            });
                        } else {
                            ui.horizontal(|ui| {
                                // 时间戳
                                let time = format_timestamp(msg.timestamp);
                                ui.label(
                                    egui::RichText::new(format!("[{}]", time))
                                        .small()
                                        .color(egui::Color32::DARK_GRAY),
                                );

                                // 用户名
                                ui.label(
                                    egui::RichText::new(format!("{}:", msg.username))
                                        .strong()
                                        .color(username_color(&msg.username)),
                                );

                                // 消息内容
                                ui.label(&msg.content);
                            });
                        }
                    }
                });
        });
    }
}

/// 格式化时间戳
fn format_timestamp(timestamp: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};

    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);
    let now = std::time::SystemTime::now();

    // 简单格式化：只显示时分秒
    if let Ok(duration) = now.duration_since(datetime) {
        if duration.as_secs() < 60 {
            return "刚刚".to_string();
        }
    }

    // 使用本地时间
    let secs = timestamp % 86400;
    let hours = (secs / 3600 + 8) % 24; // UTC+8
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// 根据用户名生成颜色
fn username_color(username: &str) -> egui::Color32 {
    let hash: u32 = username.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let hue = (hash % 360) as f32;

    // HSL to RGB (简化版)
    let s = 0.7f32;
    let l = 0.4f32;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    egui::Color32::from_rgb(
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}
