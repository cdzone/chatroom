//! 帧编解码
//!
//! 帧格式:
//! ```text
//! ┌────────────┬────────────────┬────────────────────────────────┐
//! │ Version(1B)│  Length (4B)   │         Payload (bincode)      │
//! │    u8      │    u32 BE      │         Message enum           │
//! └────────────┴────────────────┴────────────────────────────────┘
//! ```

use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{ProtocolError, Result};
use crate::{MAX_FRAME_SIZE, PROTOCOL_VERSION};

/// 帧头大小: 1 字节版本 + 4 字节长度
const HEADER_SIZE: usize = 5;

/// 帧读取器
pub struct FrameReader<R> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    /// 创建新的帧读取器
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::with_capacity(MAX_FRAME_SIZE),
        }
    }

    /// 读取并解码一帧消息
    pub async fn read_frame<M: DeserializeOwned>(&mut self) -> Result<M> {
        // 读取帧头
        let mut header = [0u8; HEADER_SIZE];
        self.reader
            .read_exact(&mut header)
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    ProtocolError::ConnectionClosed
                } else {
                    ProtocolError::Io(e)
                }
            })?;

        // 解析版本号
        let version = header[0];
        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                actual: version,
            });
        }

        // 解析长度（大端序）
        let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;

        // 检查帧大小
        if length > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: length,
                max: MAX_FRAME_SIZE,
            });
        }

        // 读取消息体（仅在需要时扩容）
        if self.buffer.len() < length {
            self.buffer.resize(length, 0);
        }
        self.reader
            .read_exact(&mut self.buffer[..length])
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    ProtocolError::ConnectionClosed
                } else {
                    ProtocolError::Io(e)
                }
            })?;

        // 反序列化
        let msg = bincode::deserialize(&self.buffer[..length])?;
        Ok(msg)
    }

    /// 接收消息（read_frame 的别名）
    pub async fn recv<M: DeserializeOwned>(&mut self) -> Result<M> {
        self.read_frame().await
    }
}

/// 帧写入器
pub struct FrameWriter<W> {
    writer: W,
}

impl<W: AsyncWrite + Unpin> FrameWriter<W> {
    /// 创建新的帧写入器
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// 编码并写入一帧消息
    pub async fn write_frame<M: Serialize>(&mut self, msg: &M) -> Result<()> {
        // 序列化消息
        let payload = bincode::serialize(msg)?;

        // 检查大小
        if payload.len() > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: payload.len(),
                max: MAX_FRAME_SIZE,
            });
        }

        // 构造帧头
        let length = payload.len() as u32;
        let mut header = [0u8; HEADER_SIZE];
        header[0] = PROTOCOL_VERSION;
        header[1..5].copy_from_slice(&length.to_be_bytes());

        // 写入帧头和消息体
        self.writer.write_all(&header).await?;
        self.writer.write_all(&payload).await?;
        self.writer.flush().await?;

        Ok(())
    }

    /// 发送消息（write_frame 的别名）
    pub async fn send<M: Serialize>(&mut self, msg: &M) -> Result<()> {
        self.write_frame(msg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientMessage, ServerMessage};
    use std::io::Cursor;

    #[tokio::test]
    async fn test_frame_roundtrip() {
        // 创建一个内存缓冲区
        let mut buffer = Vec::new();

        // 写入消息
        {
            let mut writer = FrameWriter::new(&mut buffer);
            let msg = ClientMessage::Join {
                username: "test_user".to_string(),
            };
            writer.write_frame(&msg).await.unwrap();
        }

        // 读取消息
        {
            let mut reader = FrameReader::new(Cursor::new(&buffer));
            let msg: ClientMessage = reader.read_frame().await.unwrap();
            assert_eq!(
                msg,
                ClientMessage::Join {
                    username: "test_user".to_string()
                }
            );
        }
    }

    #[tokio::test]
    async fn test_server_message_frame() {
        let mut buffer = Vec::new();

        {
            let mut writer = FrameWriter::new(&mut buffer);
            let msg = ServerMessage::ChatBroadcast {
                username: "alice".to_string(),
                content: "Hello, world!".to_string(),
                timestamp: 1234567890,
            };
            writer.write_frame(&msg).await.unwrap();
        }

        {
            let mut reader = FrameReader::new(Cursor::new(&buffer));
            let msg: ServerMessage = reader.read_frame().await.unwrap();
            match msg {
                ServerMessage::ChatBroadcast {
                    username,
                    content,
                    timestamp,
                } => {
                    assert_eq!(username, "alice");
                    assert_eq!(content, "Hello, world!");
                    assert_eq!(timestamp, 1234567890);
                }
                _ => panic!("Unexpected message type"),
            }
        }
    }
}
