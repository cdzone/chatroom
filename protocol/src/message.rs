//! 消息类型定义

use serde::{Deserialize, Serialize};

use crate::error::{ProtocolError, Result};
use crate::{MAX_MESSAGE_LEN, MAX_USERNAME_LEN};

/// 客户端发送给服务端的消息
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ClientMessage {
    /// 加入聊天室
    Join { username: String },
    /// 发送聊天消息
    Chat { content: String },
    /// 离开聊天室
    Leave,
    /// 心跳请求
    Ping,
}

impl ClientMessage {
    /// 校验消息内容是否符合约束
    pub fn validate(&self) -> Result<()> {
        match self {
            ClientMessage::Join { username } => {
                if username.is_empty() {
                    return Err(ProtocolError::UsernameTooLong {
                        len: 0,
                        max: MAX_USERNAME_LEN,
                    });
                }
                if username.len() > MAX_USERNAME_LEN {
                    return Err(ProtocolError::UsernameTooLong {
                        len: username.len(),
                        max: MAX_USERNAME_LEN,
                    });
                }
            }
            ClientMessage::Chat { content } => {
                if content.len() > MAX_MESSAGE_LEN {
                    return Err(ProtocolError::MessageTooLong {
                        len: content.len(),
                        max: MAX_MESSAGE_LEN,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// 服务端发送给客户端的消息
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ServerMessage {
    /// 欢迎消息，包含分配的用户 ID
    Welcome { user_id: u32 },
    /// 用户加入通知
    UserJoined { username: String },
    /// 用户离开通知
    UserLeft { username: String },
    /// 聊天消息广播
    ChatBroadcast {
        username: String,
        content: String,
        /// Unix 时间戳（毫秒）
        timestamp: u64,
    },
    /// 错误消息
    Error { message: String },
    /// 心跳响应
    Pong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_serialize() {
        let msg = ClientMessage::Join {
            username: "alice".to_string(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_message_serialize() {
        let msg = ServerMessage::ChatBroadcast {
            username: "bob".to_string(),
            content: "Hello!".to_string(),
            timestamp: 1234567890,
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ServerMessage = bincode::deserialize(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_validate_username_empty() {
        let msg = ClientMessage::Join {
            username: "".to_string(),
        };
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_validate_username_too_long() {
        let msg = ClientMessage::Join {
            username: "a".repeat(MAX_USERNAME_LEN + 1),
        };
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_validate_username_ok() {
        let msg = ClientMessage::Join {
            username: "valid_user".to_string(),
        };
        assert!(msg.validate().is_ok());
    }

    #[test]
    fn test_validate_message_too_long() {
        let msg = ClientMessage::Chat {
            content: "a".repeat(MAX_MESSAGE_LEN + 1),
        };
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_validate_message_ok() {
        let msg = ClientMessage::Chat {
            content: "Hello!".to_string(),
        };
        assert!(msg.validate().is_ok());
    }
}
