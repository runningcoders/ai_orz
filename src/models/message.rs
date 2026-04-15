//! Message 实体
//!
//! 对应 SQL 建表语句：[`crate::pkg::storage::sql::SQLITE_CREATE_TABLE_MESSAGES`]
//!
//! 存储设计：
//! - Text 消息：content 直接存储文本内容，file_meta 为默认值
//! - Image/File/Audio/Video 附件：content 存储文件相对路径，file_meta 存储元数据（路径、大小、MIME类型）

use common::constants::utils;
use common::enums::{MessageRole, MessageStatus, MessageType, FileType};
use crate::models::event::{Event, EventType};
use crate::models::file::FileMeta;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;

/// Message 业务实体
///
/// 组合 MessagePo，作为业务层核心对象，实现 Event trait 可以放入事件总线
#[derive(Debug, Clone)]
pub struct Message {
    /// 底层持久化对象
    pub po: MessagePo,
}

impl Message {
    /// 从 Po 创建 Message
    pub fn from_po(po: MessagePo) -> Self {
        Self { po }
    }

    /// 转换为 Po
    pub fn into_po(self) -> MessagePo {
        self.po
    }

    /// 获取消息 ID
    pub fn id(&self) -> &str {
        self.po.id.as_str()
    }

    /// 获取任务 ID
    pub fn task_id(&self) -> &str {
        self.po.task_id.as_str()
    }

    /// 创建新 Message
    pub fn new(
        id: String,
        task_id: String,
        from_id: String,
        to_id: String,
        role: MessageRole,
        message_type: MessageType,
        content: String,
        file_type: Option<FileType>,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        let po = MessagePo::new(id, task_id, from_id, to_id, role, message_type, content, file_type, file_meta, created_by);
        Self::from_po(po)
    }
}

/// Message 实现 Event trait，可以放入事件总线
impl Event for Message {
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn id(&self) -> &str {
        self.id()
    }

    fn event_type(&self) -> EventType {
        EventType::Message
    }

    fn order_key(&self) -> &str {
        // 默认按任务 ID 分组，同一个任务的消息保证顺序消费
        self.task_id()
    }

    fn priority(&self) -> u8 {
        // 默认优先级 5，可根据需求新增优先级字段覆盖
        5
    }

    fn created_at(&self) -> i64 {
        self.po.created_at
    }
}

/// MessagePo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessagePo {
    /// 消息 ID
    pub id: String,
    /// 关联任务 ID（一个任务下有多条消息）
    pub task_id: String,
    /// 来源 Agent ID（如果是用户发送则为用户 ID）
    pub from_id: String,
    /// 目标 Agent ID（如果是发给用户则为用户 ID）
    pub to_id: String,
    /// 发送者角色
    pub role: MessageRole,
    /// 消息类型
    pub message_type: MessageType,
    /// 文件类型（附件消息才有值，None 表示纯文本消息）
    pub file_type: Option<FileType>,
    /// 消息处理状态（事件总线跟踪用）
    pub status: MessageStatus,
    /// 消息内容
    /// - Text: 存储完整文本
    /// - 附件: 存储文件相对路径（相对于附件存储根目录）
    pub content: String,
    /// 文件元数据 JSON
    /// - Text: 默认空结构
    /// - 附件: 存储文件路径、大小、MIME 类型等元信息
    pub file_meta: Json<FileMeta>,
    /// 创建人 ID
    pub created_by: String,
    /// 最后修改人 ID
    pub modified_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

impl MessagePo {
    /// 创建新的 MessagePo
    pub fn new(
        id: String,
        task_id: String,
        from_id: String,
        to_id: String,
        role: MessageRole,
        message_type: MessageType,
        content: String,
        file_type: Option<FileType>,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id,
            task_id,
            from_id,
            to_id,
            role,
            message_type,
            file_type,
            status: MessageStatus::default(),
            content,
            file_meta: Json(file_meta),
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }
}
