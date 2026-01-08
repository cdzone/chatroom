//! 连接封装
//!
//! 提供类型安全的消息收发接口，封装传输层和编解码。

use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::codec::{FrameReader, FrameWriter};
use crate::error::Result;
use crate::transport::Transport;

/// 连接封装
///
/// 将传输层和编解码封装在一起，提供类型安全的消息收发接口。
///
/// # Type Parameters
/// * `R` - 读取端类型
/// * `W` - 写入端类型
pub struct Connection<R, W> {
    reader: FrameReader<R>,
    writer: FrameWriter<W>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Connection<R, W> {
    /// 从传输层创建连接
    pub fn new<T: Transport<Reader = R, Writer = W>>(transport: T) -> Self {
        let (reader, writer) = transport.split();
        Self {
            reader: FrameReader::new(reader),
            writer: FrameWriter::new(writer),
        }
    }

    /// 从读写端直接创建连接
    pub fn from_parts(reader: R, writer: W) -> Self {
        Self {
            reader: FrameReader::new(reader),
            writer: FrameWriter::new(writer),
        }
    }

    /// 分离为读取端和写入端
    ///
    /// 用于需要并发读写的场景
    pub fn split(self) -> (FrameReader<R>, FrameWriter<W>) {
        (self.reader, self.writer)
    }

    /// 接收消息
    pub async fn recv<M: DeserializeOwned>(&mut self) -> Result<M> {
        self.reader.read_frame().await
    }

    /// 发送消息
    pub async fn send<M: Serialize>(&mut self, msg: &M) -> Result<()> {
        self.writer.write_frame(msg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientMessage, ServerMessage, TcpListener, TcpTransport, TransportConfig, TransportListener};

    #[tokio::test]
    async fn test_connection_send_recv() {
        // 启动服务端
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // 客户端连接
        let client_handle = tokio::spawn(async move {
            let config = TransportConfig::default();
            let transport = TcpTransport::connect(&addr.to_string(), &config)
                .await
                .unwrap();
            let mut conn = Connection::new(transport);

            // 发送消息
            conn.send(&ClientMessage::Join {
                username: "test".to_string(),
            })
            .await
            .unwrap();

            // 接收响应
            let msg: ServerMessage = conn.recv().await.unwrap();
            assert!(matches!(msg, ServerMessage::Welcome { .. }));
        });

        // 服务端接受连接
        let transport = listener.accept().await.unwrap();
        let mut conn = Connection::new(transport);

        // 接收消息
        let msg: ClientMessage = conn.recv().await.unwrap();
        assert!(matches!(msg, ClientMessage::Join { .. }));

        // 发送响应
        conn.send(&ServerMessage::Welcome { user_id: 1 })
            .await
            .unwrap();

        client_handle.await.unwrap();
    }
}
