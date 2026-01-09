//! egui 界面实现

use eframe::egui;

use crate::client::{ChatClient, ConnectionState};

/// 聊天室应用
pub struct ChatApp {
    client: ChatClient,
    /// 是否自动滚动到底部
    auto_scroll: bool,
    /// 是否显示在线用户列表
    show_users: bool,
}

impl ChatApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 加载中文字体
        setup_fonts(&cc.egui_ctx);

        // 设置深色主题
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        Self {
            client: ChatClient::new(),
            auto_scroll: true,
            show_users: true,
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
        egui::TopBottomPanel::top("top_panel")
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(30, 30, 40)).inner_margin(8.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("💬 聊天室").color(egui::Color32::WHITE));
                    ui.separator();

                    match &self.client.state {
                        ConnectionState::Disconnected => {
                            ui.label(egui::RichText::new("● 未连接").color(egui::Color32::GRAY));
                        }
                        ConnectionState::Connecting => {
                            ui.spinner();
                            ui.label(egui::RichText::new("连接中...").color(egui::Color32::YELLOW));
                        }
                        ConnectionState::Connected { username, .. } => {
                            ui.label(egui::RichText::new("● 已连接").color(egui::Color32::GREEN));
                            ui.separator();
                            ui.label(egui::RichText::new(format!("👤 {}", username)).color(egui::Color32::WHITE));
                        }
                    }

                    // 右侧工具栏
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.client.is_connected() {
                            ui.toggle_value(&mut self.show_users, "👥 用户列表");
                        }
                    });
                });
            });

        // 底部面板：输入框
        egui::TopBottomPanel::bottom("bottom_panel")
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(35, 35, 45)).inner_margin(8.0))
            .show(ctx, |ui| {
                if self.client.is_connected() {
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.client.input_text)
                                .hint_text("输入消息，按 Enter 发送...")
                                .desired_width(ui.available_width() - 80.0)
                                .frame(true),
                        );

                        // 按 Enter 发送
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.client.send_message();
                            response.request_focus();
                        }

                        if ui.add(egui::Button::new("发送").min_size(egui::vec2(60.0, 24.0))).clicked() {
                            self.client.send_message();
                        }
                    });
                } else {
                    // 登录界面
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("服务器:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.client.server_addr)
                                    .desired_width(180.0),
                            );

                            ui.add_space(16.0);

                            ui.label("用户名:");
                            let username_response = ui.add(
                                egui::TextEdit::singleline(&mut self.client.username)
                                    .desired_width(120.0)
                                    .hint_text("字母/数字/下划线"),
                            );

                            ui.add_space(8.0);

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
                                .add_enabled(can_connect, egui::Button::new("🔗 连接").min_size(egui::vec2(70.0, 24.0)))
                                .clicked()
                            {
                                self.client.connect();
                            }

                            if matches!(self.client.state, ConnectionState::Connecting) {
                                ui.spinner();
                            }
                        });

                        if let Some(err) = &self.client.error_message {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(format!("⚠ {}", err)).color(egui::Color32::from_rgb(255, 100, 100)));
                        }
                    });
                }
            });

        // 右侧面板：在线用户列表
        if self.client.is_connected() && self.show_users {
            egui::SidePanel::right("users_panel")
                .resizable(true)
                .default_width(150.0)
                .min_width(100.0)
                .frame(egui::Frame::new().fill(egui::Color32::from_rgb(25, 25, 35)).inner_margin(8.0))
                .show(ctx, |ui| {
                    ui.heading(egui::RichText::new("在线用户").size(14.0));
                    ui.label(egui::RichText::new(format!("{} 人在线", self.client.online_users.len())).small().color(egui::Color32::GRAY));
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for user in &self.client.online_users {
                            let is_self = self.client.username == *user;
                            let text = if is_self {
                                egui::RichText::new(format!("👤 {} (我)", user)).color(egui::Color32::from_rgb(100, 200, 255))
                            } else {
                                egui::RichText::new(format!("👤 {}", user)).color(username_color(user))
                            };
                            ui.label(text);
                        }
                    });
                });
        }

        // 中间区域：消息列表
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(20, 20, 28)).inner_margin(8.0))
            .show(ctx, |ui| {
                // 断开按钮和选项
                if self.client.is_connected() {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new("🔌 断开连接").fill(egui::Color32::from_rgb(150, 50, 50))).clicked() {
                            self.client.disconnect();
                        }
                        ui.checkbox(&mut self.auto_scroll, "自动滚动");
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);
                }

                // 消息滚动区域
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(self.auto_scroll)
                    .show(ui, |ui| {
                        for msg in &self.client.messages {
                            if msg.is_system {
                                // 系统消息：居中显示
                                ui.horizontal(|ui| {
                                    ui.add_space(20.0);
                                    egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(40, 40, 50))
                                        .corner_radius(4.0)
                                        .inner_margin(egui::vec2(8.0, 4.0))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(&msg.content)
                                                    .italics()
                                                    .size(12.0)
                                                    .color(egui::Color32::from_rgb(150, 150, 160)),
                                            );
                                        });
                                });
                            } else {
                                // 用户消息
                                ui.horizontal(|ui| {
                                    // 时间戳
                                    let time = format_timestamp(msg.timestamp);
                                    ui.label(
                                        egui::RichText::new(format!("[{}]", time))
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(100, 100, 110)),
                                    );

                                    // 用户名
                                    ui.label(
                                        egui::RichText::new(format!("{}:", &msg.username))
                                            .strong()
                                            .color(username_color(&msg.username)),
                                    );

                                    // 消息内容
                                    ui.label(egui::RichText::new(&msg.content).color(egui::Color32::from_rgb(220, 220, 230)));
                                });
                            }
                            ui.add_space(2.0);
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
