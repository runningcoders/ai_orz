//! Message 实体
//!
//! 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`
//!
//! 存储设计：
//! - Text 消息：content 直接存储文本内容，file_meta 为默认值
//! - Image/File/Audio/Video 附件：content 存储文件相对路径，file_meta 存储元数据（路径、大小、MIME类型）

use common::constants::utils;
use common::enums::{MessageRole, MessageStatus, MessageType, FileType};
use crate::models::event::{Event, EventTopic};
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

    /// 获取项目 ID（如果有）
    pub fn project_id(&self) -> Option<&str> {
        self.po.project_id.as_deref()
    }

    /// 获取任务 ID（如果有）
    pub fn task_id(&self) -> Option<&str> {
        self.po.task_id.as_deref()
    }

    /// 创建新 Message（完整参数，指定 project_id 和 task_id）
    pub fn new_with_context(
        id: String,
        project_id: Option<String>,
        task_id: Option<String>,
        from_id: String,
        to_id: String,
        from_role: MessageRole,
        to_role: MessageRole,
        message_type: MessageType,
        content: String,
        file_type: Option<FileType>,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        let po = MessagePo::new(
            id,
            project_id,
            task_id,
            from_id,
            to_id,
            from_role,
            to_role,
            message_type,
            content,
            file_type,
            file_meta,
            created_by,
        );
        Self::from_po(po)
    }

    /// 创建新 Message（兼容旧接口，向后兼容）
    #[deprecated = "Use new_with_context instead to support project context"]
    pub fn new(
        id: String,
        task_id: String,
        from_id: String,
        to_id: String,
        from_role: MessageRole,
        to_role: MessageRole,
        message_type: MessageType,
        content: String,
        file_type: Option<FileType>,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        Self::new_with_context(id, None, Some(task_id), from_id, to_id, from_role, to_role, message_type, content, file_type, file_meta, created_by)
    }
}

/// Message 实现 Event trait，可以放入事件总线
impl Event for Message {
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }

    fn id(&self) -> &str {
        self.id()
    }

    fn topic(&self) -> EventTopic {
        EventTopic::Message
    }

    fn order_key(&self) -> &str {
        // 默认按任务 ID 分组，同一个任务的消息保证顺序消费
        // 如果没有任务，则按项目 ID 分组
        // 如果也没有项目，则按消息自己的 ID 分组（单条消息消费）
        if let Some(task_id) = self.task_id() {
            task_id
        } else if let Some(project_id) = self.project_id() {
            project_id
        } else {
            self.id()
        }
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
    /// 关联项目 ID（可为空，没有项目时为 None）
    pub project_id: Option<String>,
    /// 关联任务 ID（可为空，没有任务时为 None）
    pub task_id: Option<String>,
    /// 来源 ID（如果是用户发送则为用户 ID，如果是 Agent 发送则为 Agent ID）
    pub from_id: String,
    /// 目标 ID（如果是发给用户则为用户 ID，如果是发给 Agent 则为 Agent ID）
    pub to_id: String,
    /// 发送者角色
    pub from_role: MessageRole,
    /// 接收者角色
    pub to_role: MessageRole,
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
        project_id: Option<String>,
        task_id: Option<String>,
        from_id: String,
        to_id: String,
        from_role: MessageRole,
        to_role: MessageRole,
        message_type: MessageType,
        content: String,
        file_type: Option<FileType>,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id,
            project_id,
            task_id,
            from_id,
            to_id,
            from_role,
            to_role,
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

/// 统一工具调用消息内容
/// 
/// 不管是请求还是结果，都用这个结构存储在 message.content 中
/// 对应 MessageType::ToolCallRequest 或 MessageType::ToolCallResult
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallMessage {
    /// 工具调用请求 ID（每个请求唯一，结果中需要对应）
    pub request_id: String,
    /// 工具 ID
    pub tool_id: String,
    /// 工具名称（便于日志查看）
    pub tool_name: String,
    /// 关联项目 ID
    pub project_id: Option<String>,
    /// 关联任务 ID
    pub task_id: Option<String>,
    /// 发起方 ID（谁发起的这次调用）
    pub from_id: String,
    /// 目标执行方 ID（谁来执行这个调用）
    pub to_id: String,
    /// 调用参数（请求时有效）JSON 格式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    /// 调用结果（完成后有效）JSON 格式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// 是否执行成功（结果时有效）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_success: Option<bool>,
    /// 错误信息（执行失败时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// 大结果附件元数据（当结果太大放不下 content 时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_file_meta: Option<FileMeta>,
}

impl ToolCallMessage {
    /// 创建新的工具调用请求
    pub fn new_request(
        request_id: String,
        tool_id: String,
        tool_name: String,
        project_id: Option<String>,
        task_id: Option<String>,
        from_id: String,
        to_id: String,
        args: serde_json::Value,
    ) -> Self {
        Self {
            request_id,
            tool_id,
            tool_name,
            project_id,
            task_id,
            from_id,
            to_id,
            args: Some(args),
            result: None,
            is_success: None,
            error_message: None,
            result_file_meta: None,
        }
    }

    /// 创建工具调用完成响应（成功）
    pub fn new_success_result(
        &self,
        result: serde_json::Value,
        result_file_meta: Option<FileMeta>,
    ) -> Self {
        Self {
            request_id: self.request_id.clone(),
            tool_id: self.tool_id.clone(),
            tool_name: self.tool_name.clone(),
            project_id: self.project_id.clone(),
            task_id: self.task_id.clone(),
            from_id: self.to_id.clone(), // 执行方反过来返回给原发起方
            to_id: self.from_id.clone(),
            args: self.args.clone(),
            result: Some(result),
            is_success: Some(true),
            error_message: None,
            result_file_meta,
        }
    }

    /// 创建工具调用完成响应（失败）
    pub fn new_error_result(
        &self,
        error_message: String,
    ) -> Self {
        Self {
            request_id: self.request_id.clone(),
            tool_id: self.tool_id.clone(),
            tool_name: self.tool_name.clone(),
            project_id: self.project_id.clone(),
            task_id: self.task_id.clone(),
            from_id: self.to_id.clone(), // 执行方反过来返回给原发起方
            to_id: self.from_id.clone(),
            args: self.args.clone(),
            result: None,
            is_success: Some(false),
            error_message: Some(error_message),
            result_file_meta: None,
        }
    }

    /// 创建工具调用完成响应（失败，带有错误结果数据）
    pub fn new_error_result_with_data(
        &self,
        result: serde_json::Value,
        error_message: String,
    ) -> Self {
        Self {
            request_id: self.request_id.clone(),
            tool_id: self.tool_id.clone(),
            tool_name: self.tool_name.clone(),
            project_id: self.project_id.clone(),
            task_id: self.task_id.clone(),
            from_id: self.to_id.clone(), // 执行方反过来返回给原发起方
            to_id: self.from_id.clone(),
            args: self.args.clone(),
            result: Some(result),
            is_success: Some(false),
            error_message: Some(error_message),
            result_file_meta: None,
        }
    }
}
