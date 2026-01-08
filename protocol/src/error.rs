//! 错误类型定义

use thiserror::Error;

/// 协议错误类型
#[derive(Error, Debug)]
pub enum ProtocolError {
    /// IO 错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 序列化错误
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    /// 协议版本不匹配
    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u8, actual: u8 },

    /// 帧大小超限
    #[error("Frame too large: {size} bytes (max: {max})")]
    FrameTooLarge { size: usize, max: usize },

    /// 连接超时
    #[error("Connection timeout")]
    ConnectionTimeout,

    /// 连接已关闭
    #[error("Connection closed")]
    ConnectionClosed,

    /// 用户名过长
    #[error("Username too long: {len} chars (max: {max})")]
    UsernameTooLong { len: usize, max: usize },

    /// 消息过长
    #[error("Message too long: {len} bytes (max: {max})")]
    MessageTooLong { len: usize, max: usize },
}

/// 协议操作结果类型
pub type Result<T> = std::result::Result<T, ProtocolError>;
