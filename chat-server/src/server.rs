//! 聊天服务器核心实现

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use protocol::{
    ClientMessage, Connection, ProtocolError, ServerMessage, TcpListener, TcpTransport,
    TransportListener, HEARTBEAT_TIMEOUT, JOIN_TIMEOUT, MAX_CONNECTIONS,
};
use tokio::sync::{broadcast, watch, RwLock};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// 广播消息类型
#[derive(Clone, Debug)]
pub enum BroadcastMsg {
    /// 聊天消息
    Chat {
        username: String,
        content: String,
        timestamp: u64,
    },
    /// 用户加入
    UserJoined { username: String },
    /// 用户离开
    UserLeft { username: String },
    /// 服务器关闭
    Shutdown { message: String },
}

/// 用户信息
#[derive(Debug)]
struct User {
    username: String,
}

/// 共享状态
struct SharedState {
    /// 在线用户列表: user_id -> User
    users: RwLock<HashMap<u32, User>>,
    /// 用户名到 user_id 的映射（用于检查重名）
    usernames: RwLock<HashMap<String, u32>>,
    /// 当前连接数
    connection_count: AtomicU32,
    /// 下一个用户 ID
    next_user_id: AtomicU32,
}

impl SharedState {
    fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
            usernames: RwLock::new(HashMap::new()),
            connection_count: AtomicU32::new(0),
            next_user_id: AtomicU32::new(1),
        }
    }

    /// 增加连接数，如果超过限制则返回 false
    fn try_add_connection(&self) -> bool {
        loop {
            let current = self.connection_count.load(Ordering::SeqCst);
            if current >= MAX_CONNECTIONS as u32 {
                return false;
            }
            if self
                .connection_count
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// 减少连接数
    fn remove_connection(&self) {
        self.connection_count.fetch_sub(1, Ordering::SeqCst);
    }

    /// 添加用户，成功返回分配的用户 ID，失败返回 None
    async fn add_user(&self, username: String) -> Option<u32> {
        let mut usernames = self.usernames.write().await;
        if usernames.contains_key(&username) {
            return None;
        }
        // 只有在确认用户名可用后才分配 ID
        let id = self.next_user_id.fetch_add(1, Ordering::SeqCst);
        usernames.insert(username.clone(), id);
        drop(usernames);

        let mut users = self.users.write().await;
        users.insert(id, User { username });
        Some(id)
    }

    /// 移除用户
    async fn remove_user(&self, id: u32) -> Option<String> {
        let mut users = self.users.write().await;
        if let Some(user) = users.remove(&id) {
            drop(users);
            let mut usernames = self.usernames.write().await;
            usernames.remove(&user.username);
            Some(user.username)
        } else {
            None
        }
    }

    /// 获取在线用户数
    #[allow(dead_code)]
    fn online_count(&self) -> u32 {
        self.connection_count.load(Ordering::SeqCst)
    }
}

/// 聊天服务器
pub struct ChatServer {
    state: Arc<SharedState>,
    broadcast_tx: broadcast::Sender<BroadcastMsg>,
    /// 关闭信号发送端
    shutdown_tx: watch::Sender<bool>,
    /// 关闭信号接收端（用于克隆给客户端处理器）
    shutdown_rx: watch::Receiver<bool>,
}

impl ChatServer {
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            state: Arc::new(SharedState::new()),
            broadcast_tx,
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// 运行服务器（支持 graceful shutdown）
    pub async fn run(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("Server listening on {}", listener.local_addr()?);

        loop {
            tokio::select! {
                // 接受新连接
                result = listener.accept() => {
                    match result {
                        Ok(transport) => {
                            // 检查连接数限制
                            if !self.state.try_add_connection() {
                                warn!("Connection limit reached, rejecting new connection");
                                // 发送错误消息后关闭
                                let mut conn = Connection::new(transport);
                                let _ = conn
                                    .send(&ServerMessage::Error {
                                        message: "服务器繁忙，请稍后重试".to_string(),
                                    })
                                    .await;
                                continue;
                            }

                            let state = Arc::clone(&self.state);
                            let broadcast_tx = self.broadcast_tx.clone();
                            let broadcast_rx = self.broadcast_tx.subscribe();
                            let shutdown_rx = self.shutdown_rx.clone();

                            tokio::spawn(async move {
                                if let Err(e) =
                                    handle_client(transport, state.clone(), broadcast_tx, broadcast_rx, shutdown_rx)
                                        .await
                                {
                                    debug!("Client handler error: {}", e);
                                }
                                state.remove_connection();
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }

                // 监听 Ctrl+C 信号
                _ = tokio::signal::ctrl_c() => {
                    info!("Received shutdown signal, initiating graceful shutdown...");
                    self.shutdown().await;
                    break;
                }
            }
        }

        Ok(())
    }

    /// 执行 graceful shutdown
    async fn shutdown(&self) {
        // 广播关闭消息给所有客户端
        let _ = self.broadcast_tx.send(BroadcastMsg::Shutdown {
            message: "服务器正在关闭".to_string(),
        });

        // 发送关闭信号
        let _ = self.shutdown_tx.send(true);

        // 等待所有连接断开（最多等待 5 秒）
        let start = std::time::Instant::now();
        let timeout_duration = std::time::Duration::from_secs(5);

        while self.state.online_count() > 0 {
            if start.elapsed() > timeout_duration {
                warn!(
                    "Shutdown timeout, {} connections still active",
                    self.state.online_count()
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        info!("Server shutdown complete");
    }
}

impl Default for ChatServer {
    fn default() -> Self {
        Self::new()
    }
}

/// 处理单个客户端连接
async fn handle_client(
    transport: TcpTransport,
    state: Arc<SharedState>,
    broadcast_tx: broadcast::Sender<BroadcastMsg>,
    mut broadcast_rx: broadcast::Receiver<BroadcastMsg>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut conn = Connection::new(transport);

    // 等待 Join 消息（带超时）
    let join_result = timeout(JOIN_TIMEOUT, conn.recv::<ClientMessage>()).await;

    let (user_id, username) = match join_result {
        Ok(Ok(ClientMessage::Join { username })) => {
            // 验证用户名
            if let Err(e) = (ClientMessage::Join {
                username: username.clone(),
            })
            .validate()
            {
                conn.send(&ServerMessage::Error {
                    message: format!("无效的用户名: {}", e),
                })
                .await?;
                return Ok(());
            }

            // 尝试添加用户（ID 在内部分配）
            let user_id = match state.add_user(username.clone()).await {
                Some(id) => id,
                None => {
                    conn.send(&ServerMessage::Error {
                        message: "用户名已存在".to_string(),
                    })
                    .await?;
                    return Ok(());
                }
            };

            // 发送欢迎消息
            conn.send(&ServerMessage::Welcome { user_id }).await?;

            // 广播用户加入
            let _ = broadcast_tx.send(BroadcastMsg::UserJoined {
                username: username.clone(),
            });

            info!("User {} (id={}) joined", username, user_id);
            (user_id, username)
        }
        Ok(Ok(_)) => {
            conn.send(&ServerMessage::Error {
                message: "请先发送 Join 消息".to_string(),
            })
            .await?;
            return Ok(());
        }
        Ok(Err(e)) => {
            debug!("Failed to receive Join message: {}", e);
            return Ok(());
        }
        Err(_) => {
            debug!("Join timeout");
            conn.send(&ServerMessage::Error {
                message: "加入超时".to_string(),
            })
            .await?;
            return Ok(());
        }
    };

    // 分离读写
    let (mut reader, mut writer) = conn.split();

    // 主消息循环
    loop {
        tokio::select! {
            // 接收客户端消息（带心跳超时）
            result = timeout(HEARTBEAT_TIMEOUT, reader.recv::<ClientMessage>()) => {
                match result {
                    Ok(Ok(msg)) => {
                        match msg {
                            ClientMessage::Chat { content } => {
                                // 验证消息
                                if let Err(e) = (ClientMessage::Chat { content: content.clone() }).validate() {
                                    writer.send(&ServerMessage::Error {
                                        message: format!("消息无效: {}", e),
                                    }).await?;
                                    continue;
                                }

                                let timestamp = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();

                                debug!("User {} sent: {}", username, content);

                                // 广播消息
                                let _ = broadcast_tx.send(BroadcastMsg::Chat {
                                    username: username.clone(),
                                    content,
                                    timestamp,
                                });
                            }
                            ClientMessage::Ping => {
                                writer.send(&ServerMessage::Pong).await?;
                            }
                            ClientMessage::Leave => {
                                info!("User {} left", username);
                                break;
                            }
                            ClientMessage::Join { .. } => {
                                // 已经加入，忽略重复的 Join
                                writer.send(&ServerMessage::Error {
                                    message: "已经加入聊天室".to_string(),
                                }).await?;
                            }
                        }
                    }
                    Ok(Err(ProtocolError::ConnectionClosed)) => {
                        info!("User {} disconnected", username);
                        break;
                    }
                    Ok(Err(e)) => {
                        warn!("Error receiving from {}: {}", username, e);
                        break;
                    }
                    Err(_) => {
                        // 心跳超时
                        warn!("Heartbeat timeout for user {}", username);
                        break;
                    }
                }
            }

            // 接收广播消息
            result = broadcast_rx.recv() => {
                match result {
                    Ok(msg) => {
                        let (server_msg, should_exit) = match msg {
                            BroadcastMsg::Chat { username, content, timestamp } => {
                                (ServerMessage::ChatBroadcast { username, content, timestamp }, false)
                            }
                            BroadcastMsg::UserJoined { username } => {
                                (ServerMessage::UserJoined { username }, false)
                            }
                            BroadcastMsg::UserLeft { username } => {
                                (ServerMessage::UserLeft { username }, false)
                            }
                            BroadcastMsg::Shutdown { message } => {
                                (ServerMessage::Shutdown { message }, true)
                            }
                        };

                        if let Err(e) = writer.send(&server_msg).await {
                            debug!("Failed to send to {}: {}", username, e);
                            break;
                        }

                        if should_exit {
                            info!("Shutdown signal received, closing connection for {}", username);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("User {} lagged {} messages", username, n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // 监听 shutdown 信号
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("Shutdown signal received for {}", username);
                    break;
                }
            }
        }
    }

    // 清理用户
    if let Some(username) = state.remove_user(user_id).await {
        let _ = broadcast_tx.send(BroadcastMsg::UserLeft { username });
    }

    Ok(())
}
