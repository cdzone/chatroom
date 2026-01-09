//! 协议常量定义

use std::time::Duration;

/// 协议版本号
pub const PROTOCOL_VERSION: u8 = 1;

/// 用户名最大长度
pub const MAX_USERNAME_LEN: usize = 20;

/// 单条消息最大长度
pub const MAX_MESSAGE_LEN: usize = 4096;

/// 消息帧最大大小
pub const MAX_FRAME_SIZE: usize = 8192;

/// 服务端最大连接数
pub const MAX_CONNECTIONS: usize = 100;

/// 客户端心跳间隔（秒）
pub const HEARTBEAT_INTERVAL_SECS: u64 = 10;

/// 服务端心跳超时（秒）- 超过此时间无消息则断开
pub const HEARTBEAT_TIMEOUT_SECS: u64 = 30;

/// 加入超时（秒）- 连接后必须在此时间内发送 Join
pub const JOIN_TIMEOUT_SECS: u64 = 30;

/// 连接超时（秒）
pub const CONNECT_TIMEOUT_SECS: u64 = 10;

/// 心跳间隔 Duration
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(HEARTBEAT_INTERVAL_SECS);

/// 心跳超时 Duration
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(HEARTBEAT_TIMEOUT_SECS);

/// 连接超时 Duration
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(CONNECT_TIMEOUT_SECS);

/// 加入超时 Duration
pub const JOIN_TIMEOUT: Duration = Duration::from_secs(JOIN_TIMEOUT_SECS);
