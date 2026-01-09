//! 聊天客户端核心实现

use std::collections::VecDeque;
use std::sync::mpsc as std_mpsc;
use std::thread;

use protocol::{
    ClientMessage, Connection, ProtocolError, ServerMessage, TcpTransport, Transport,
    TransportConfig, CONNECT_TIMEOUT, HEARTBEAT_INTERVAL, MAX_USERNAME_LEN,
};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, info, warn};

/// 消息历史上限
const MAX_MESSAGES: usize = 1000;

/// UI 发送给网络线程的命令
#[derive(Debug)]
pub enum UiCommand {
    /// 连接服务器
    Connect { addr: String, username: String },
    /// 发送聊天消息
    SendChat { content: String },
    /// 断开连接
    Disconnect,
}

/// 网络线程发送给 UI 的事件
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// 连接成功
    Connected {
        user_id: u32,
        online_users: Vec<String>,
    },
    /// 连接失败
    ConnectFailed { reason: String },
    /// 收到聊天消息
    ChatMessage {
        username: String,
        content: String,
        timestamp: u64,
    },
    /// 用户加入
    UserJoined { username: String },
    /// 用户离开
    UserLeft { username: String },
    /// 错误消息
    Error { message: String },
    /// 连接断开
    Disconnected { reason: String },
}

/// 聊天消息记录
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub username: String,
    pub content: String,
    pub timestamp: u64,
    pub is_system: bool,
}

/// 客户端状态
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected { user_id: u32, username: String },
}

/// 聊天客户端
pub struct ChatClient {
    /// 连接状态
    pub state: ConnectionState,
    /// 聊天消息历史（使用 VecDeque 提高删除效率）
    pub messages: VecDeque<ChatMessage>,
    /// 在线用户列表
    pub online_users: Vec<String>,
    /// 发送命令到网络线程（使用 std::sync::mpsc，因为 UI 线程是同步的）
    cmd_tx: std_mpsc::Sender<UiCommand>,
    /// 接收网络事件（使用 std::sync::mpsc，因为 UI 线程是同步的）
    event_rx: std_mpsc::Receiver<NetworkEvent>,
    /// 输入框内容
    pub input_text: String,
    /// 服务器地址
    pub server_addr: String,
    /// 用户名
    pub username: String,
    /// 错误消息
    pub error_message: Option<String>,
}

impl ChatClient {
    pub fn new() -> Self {
        // UI <-> 网络线程桥接通道（std::sync::mpsc）
        let (cmd_tx, cmd_rx_std) = std_mpsc::channel::<UiCommand>();
        let (event_tx_std, event_rx) = std_mpsc::channel::<NetworkEvent>();

        // 启动网络线程
        thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                // 在 tokio runtime 内部创建 tokio::sync::mpsc 通道
                let (cmd_tx_tokio, cmd_rx_tokio) = mpsc::channel::<UiCommand>(32);
                let (event_tx_tokio, mut event_rx_tokio) = mpsc::channel::<NetworkEvent>(32);

                // 桥接任务：std::sync::mpsc -> tokio::sync::mpsc
                // 使用 spawn_blocking 在独立线程中阻塞等待
                let cmd_tx_clone = cmd_tx_tokio.clone();
                let cmd_bridge = tokio::task::spawn_blocking(move || {
                    while let Ok(cmd) = cmd_rx_std.recv() {
                        if cmd_tx_clone.blocking_send(cmd).is_err() {
                            break;
                        }
                    }
                });

                // 桥接任务：tokio::sync::mpsc -> std::sync::mpsc
                let event_bridge = tokio::spawn(async move {
                    while let Some(event) = event_rx_tokio.recv().await {
                        if event_tx_std.send(event).is_err() {
                            break;
                        }
                    }
                });

                // 运行网络循环
                network_loop(cmd_rx_tokio, event_tx_tokio).await;

                // 等待桥接任务结束
                let _ = cmd_bridge.await;
                let _ = event_bridge.await;
            });
        });

        Self {
            state: ConnectionState::Disconnected,
            messages: VecDeque::new(),
            online_users: Vec::new(),
            cmd_tx,
            event_rx,
            input_text: String::new(),
            server_addr: "127.0.0.1:8080".to_string(),
            username: String::new(),
            error_message: None,
        }
    }

    /// 处理网络事件，返回是否有新事件
    pub fn poll_events(&mut self) -> bool {
        let mut has_events = false;
        while let Ok(event) = self.event_rx.try_recv() {
            self.handle_event(event);
            has_events = true;
        }
        has_events
    }

    fn handle_event(&mut self, event: NetworkEvent) {
        match event {
            NetworkEvent::Connected { user_id, online_users } => {
                if let ConnectionState::Connecting = &self.state {
                    let username = self.username.clone();
                    self.state = ConnectionState::Connected { user_id, username };
                    self.error_message = None;
                    // 使用服务端返回的在线用户列表
                    self.online_users = online_users;
                    self.add_system_message("已连接到服务器".to_string());
                }
            }
            NetworkEvent::ConnectFailed { reason } => {
                self.state = ConnectionState::Disconnected;
                self.error_message = Some(reason);
            }
            NetworkEvent::ChatMessage {
                username,
                content,
                timestamp,
            } => {
                self.add_message(ChatMessage {
                    username,
                    content,
                    timestamp,
                    is_system: false,
                });
            }
            NetworkEvent::UserJoined { username } => {
                if !self.online_users.contains(&username) {
                    self.online_users.push(username.clone());
                }
                self.add_system_message(format!("{} 加入了聊天室", username));
            }
            NetworkEvent::UserLeft { username } => {
                self.online_users.retain(|u| u != &username);
                self.add_system_message(format!("{} 离开了聊天室", username));
            }
            NetworkEvent::Error { message } => {
                self.error_message = Some(message);
            }
            NetworkEvent::Disconnected { reason } => {
                self.state = ConnectionState::Disconnected;
                self.online_users.clear();
                self.add_system_message(format!("已断开连接: {}", reason));
            }
        }
    }

    fn add_message(&mut self, msg: ChatMessage) {
        // 限制消息历史数量（VecDeque::pop_front 是 O(1)）
        if self.messages.len() >= MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(msg);
    }

    fn add_system_message(&mut self, content: String) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.add_message(ChatMessage {
            username: "系统".to_string(),
            content,
            timestamp,
            is_system: true,
        });
    }

    /// 验证用户名格式
    pub fn validate_username(&self) -> Result<(), String> {
        let username = &self.username;
        if username.is_empty() {
            return Err("用户名不能为空".to_string());
        }
        if username.len() > MAX_USERNAME_LEN {
            return Err(format!("用户名不能超过 {} 个字符", MAX_USERNAME_LEN));
        }
        if !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err("用户名只能包含字母、数字、下划线和连字符".to_string());
        }
        Ok(())
    }

    /// 连接服务器
    pub fn connect(&mut self) {
        if matches!(self.state, ConnectionState::Disconnected) {
            // 客户端验证用户名
            if let Err(e) = self.validate_username() {
                self.error_message = Some(e);
                return;
            }

            self.state = ConnectionState::Connecting;
            self.error_message = None;
            let _ = self.cmd_tx.send(UiCommand::Connect {
                addr: self.server_addr.clone(),
                username: self.username.clone(),
            });
        }
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        if matches!(self.state, ConnectionState::Connected { .. }) {
            let _ = self.cmd_tx.send(UiCommand::Disconnect);
        }
    }

    /// 发送消息
    pub fn send_message(&mut self) {
        if matches!(self.state, ConnectionState::Connected { .. }) && !self.input_text.is_empty() {
            let content = self.input_text.clone();
            self.input_text.clear();
            let _ = self.cmd_tx.send(UiCommand::SendChat { content });
        }
    }

    /// 是否已连接
    pub fn is_connected(&self) -> bool {
        matches!(self.state, ConnectionState::Connected { .. })
    }
}

impl Default for ChatClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 网络循环
async fn network_loop(
    mut cmd_rx: mpsc::Receiver<UiCommand>,
    event_tx: mpsc::Sender<NetworkEvent>,
) {
    loop {
        // 等待连接命令
        let (addr, username) = match cmd_rx.recv().await {
            Some(UiCommand::Connect { addr, username }) => (addr, username),
            Some(_) => continue,
            None => break, // UI 线程已关闭
        };

        // 尝试连接
        match connect_and_run(&addr, &username, &mut cmd_rx, &event_tx).await {
            Ok(()) => {
                let _ = event_tx
                    .send(NetworkEvent::Disconnected {
                        reason: "正常断开".to_string(),
                    })
                    .await;
            }
            Err(e) => {
                let _ = event_tx
                    .send(NetworkEvent::Disconnected {
                        reason: e.to_string(),
                    })
                    .await;
            }
        }
    }
}

/// 连接并运行消息循环
async fn connect_and_run(
    addr: &str,
    username: &str,
    cmd_rx: &mut mpsc::Receiver<UiCommand>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) -> anyhow::Result<()> {
    // 连接服务器
    let config = TransportConfig {
        connect_timeout: CONNECT_TIMEOUT,
        nodelay: true,
    };

    let transport = match TcpTransport::connect(addr, &config).await {
        Ok(t) => t,
        Err(e) => {
            let _ = event_tx
                .send(NetworkEvent::ConnectFailed {
                    reason: format!("连接失败: {}", e),
                })
                .await;
            return Ok(());
        }
    };

    info!("Connected to {}", addr);
    let mut conn = Connection::new(transport);

    // 发送 Join 消息
    conn.send(&ClientMessage::Join {
        username: username.to_string(),
    })
    .await?;

    // 等待 Welcome 响应
    match conn.recv::<ServerMessage>().await? {
        ServerMessage::Welcome { user_id, online_users } => {
            let _ = event_tx.send(NetworkEvent::Connected { user_id, online_users }).await;
            info!("Joined as user_id={}", user_id);
        }
        ServerMessage::Error { message } => {
            let _ = event_tx
                .send(NetworkEvent::ConnectFailed {
                    reason: format!("加入失败: {}", message),
                })
                .await;
            return Ok(());
        }
        _ => {
            let _ = event_tx
                .send(NetworkEvent::ConnectFailed {
                    reason: "协议错误: 未收到 Welcome".to_string(),
                })
                .await;
            return Ok(());
        }
    }

    // 分离读写
    let (mut reader, mut writer) = conn.split();

    // 心跳定时器
    let mut heartbeat = interval(HEARTBEAT_INTERVAL);
    heartbeat.tick().await; // 跳过第一次立即触发

    loop {
        tokio::select! {
            // 接收服务器消息
            result = reader.recv::<ServerMessage>() => {
                match result {
                    Ok(msg) => {
                        match msg {
                            ServerMessage::ChatBroadcast { username, content, timestamp } => {
                                let _ = event_tx.send(NetworkEvent::ChatMessage {
                                    username,
                                    content,
                                    timestamp,
                                }).await;
                            }
                            ServerMessage::UserJoined { username } => {
                                let _ = event_tx.send(NetworkEvent::UserJoined { username }).await;
                            }
                            ServerMessage::UserLeft { username } => {
                                let _ = event_tx.send(NetworkEvent::UserLeft { username }).await;
                            }
                            ServerMessage::Error { message } => {
                                let _ = event_tx.send(NetworkEvent::Error { message }).await;
                            }
                            ServerMessage::Pong => {
                                debug!("Received pong");
                            }
                            ServerMessage::Welcome { .. } => {
                                // 忽略重复的 Welcome
                            }
                            ServerMessage::Shutdown { message } => {
                                info!("Server shutdown: {}", message);
                                let _ = event_tx.send(NetworkEvent::Disconnected {
                                    reason: format!("服务器关闭: {}", message),
                                }).await;
                                return Ok(());
                            }
                        }
                    }
                    Err(ProtocolError::ConnectionClosed) => {
                        info!("Server closed connection");
                        break;
                    }
                    Err(e) => {
                        warn!("Receive error: {}", e);
                        break;
                    }
                }
            }

            // 心跳
            _ = heartbeat.tick() => {
                if let Err(e) = writer.send(&ClientMessage::Ping).await {
                    warn!("Failed to send ping: {}", e);
                    break;
                }
                debug!("Sent ping");
            }

            // 处理 UI 命令（直接 await，不再轮询）
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(UiCommand::SendChat { content }) => {
                        if let Err(e) = writer.send(&ClientMessage::Chat { content }).await {
                            warn!("Failed to send chat: {}", e);
                            break; // 现在正确退出外层 loop
                        }
                    }
                    Some(UiCommand::Disconnect) => {
                        let _ = writer.send(&ClientMessage::Leave).await;
                        return Ok(());
                    }
                    Some(UiCommand::Connect { .. }) => {
                        // 已连接，忽略
                    }
                    None => {
                        // 通道关闭，退出
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
