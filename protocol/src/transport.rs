//! 传输层抽象
//!
//! 提供 Transport trait 使上层协议与具体传输实现解耦，
//! 便于未来从 TCP 切换到 QUIC 等其他传输协议。

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::error::{ProtocolError, Result};
use crate::CONNECT_TIMEOUT;

/// 传输层配置
#[derive(Clone, Debug)]
pub struct TransportConfig {
    /// 连接超时时间
    pub connect_timeout: Duration,
    /// 是否禁用 Nagle 算法（TCP nodelay）
    pub nodelay: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout: CONNECT_TIMEOUT,
            nodelay: true, // 聊天应用建议开启，减少延迟
        }
    }
}

/// 传输层抽象 trait
///
/// 定义了客户端连接和读写分离的基本操作。
/// 通过实现此 trait，可以支持不同的传输协议（TCP、QUIC 等）。
pub trait Transport: Send + Sync + Sized {
    /// 读取端类型
    type Reader: AsyncRead + Unpin + Send;
    /// 写入端类型
    type Writer: AsyncWrite + Unpin + Send;

    /// 建立连接（客户端使用）
    ///
    /// # Arguments
    /// * `addr` - 服务器地址，格式为 "host:port"
    /// * `config` - 传输配置
    fn connect(
        addr: &str,
        config: &TransportConfig,
    ) -> impl std::future::Future<Output = Result<Self>> + Send;

    /// 分离读写端
    ///
    /// 将连接分离为独立的读取端和写入端，便于并发读写。
    fn split(self) -> (Self::Reader, Self::Writer);
}

/// 传输层监听器抽象 trait（服务端使用）
pub trait TransportListener: Send + Sync + Sized {
    /// 对应的传输类型
    type Transport: Transport;

    /// 绑定地址并开始监听
    ///
    /// # Arguments
    /// * `addr` - 监听地址，格式为 "host:port"
    fn bind(addr: &str) -> impl std::future::Future<Output = Result<Self>> + Send;

    /// 接受新连接
    fn accept(&self) -> impl std::future::Future<Output = Result<Self::Transport>> + Send;
}

// ============================================================================
// TCP 实现
// ============================================================================

/// TCP 传输实现
#[derive(Debug)]
pub struct TcpTransport {
    stream: TcpStream,
}

impl Transport for TcpTransport {
    type Reader = OwnedReadHalf;
    type Writer = OwnedWriteHalf;

    async fn connect(addr: &str, config: &TransportConfig) -> Result<Self> {
        // 带超时的连接
        let stream = timeout(config.connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| ProtocolError::ConnectionTimeout)?
            .map_err(ProtocolError::Io)?;

        // 设置 TCP nodelay
        stream.set_nodelay(config.nodelay)?;

        Ok(Self { stream })
    }

    fn split(self) -> (Self::Reader, Self::Writer) {
        self.stream.into_split()
    }
}

impl TcpTransport {
    /// 从已有的 TcpStream 创建（服务端 accept 后使用）
    pub fn from_stream(stream: TcpStream) -> Result<Self> {
        stream.set_nodelay(true)?;
        Ok(Self { stream })
    }
}

/// TCP 监听器实现
pub struct TcpListener {
    listener: tokio::net::TcpListener,
}

impl TransportListener for TcpListener {
    type Transport = TcpTransport;

    async fn bind(addr: &str) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(ProtocolError::Io)?;
        Ok(Self { listener })
    }

    async fn accept(&self) -> Result<TcpTransport> {
        let (stream, _addr) = self.listener.accept().await.map_err(ProtocolError::Io)?;
        TcpTransport::from_stream(stream)
    }
}

impl TcpListener {
    /// 获取本地绑定地址
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_listener_bind() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(addr.port() > 0);
    }

    #[tokio::test]
    async fn test_tcp_connect_and_accept() {
        // 启动监听
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // 客户端连接
        let client_handle = tokio::spawn(async move {
            let config = TransportConfig::default();
            TcpTransport::connect(&addr.to_string(), &config).await
        });

        // 服务端接受
        let server_transport = listener.accept().await.unwrap();
        let client_transport = client_handle.await.unwrap().unwrap();

        // 验证连接成功
        assert!(format!("{:?}", server_transport).contains("TcpTransport"));
        assert!(format!("{:?}", client_transport).contains("TcpTransport"));
    }
}
