//! 聊天室共享协议库
//!
//! 包含:
//! - 消息类型定义 (ClientMessage, ServerMessage)
//! - 传输层抽象 (Transport trait)
//! - 帧编解码 (Codec)
//! - 连接封装 (Connection)

mod message;
mod constants;
mod transport;
mod codec;
mod connection;
mod error;

pub use message::{ClientMessage, ServerMessage};
pub use constants::*;
pub use transport::{Transport, TransportListener, TransportConfig, TcpTransport, TcpListener};
pub use codec::{FrameReader, FrameWriter};
pub use connection::Connection;
pub use error::{ProtocolError, Result};
