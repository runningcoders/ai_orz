//! Message 实体
//!
//! 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`
//!
//! 存储设计：
//! - Text 消息：content 直接存储文本内容，file_meta 为默认值
//! - Image/File/Audio/Video 附件：content 存储文件相对路径，file_meta 存储元数据（路径、大小、MIME类型）

use crate::models::event::{Event, EventTopic};
use crate::models::file::FileMeta;
use crate::models::tool::ToolCallTraceRef;
use common::constants::utils;
use common::enums::{FileType, MessageRole, MessageStatus, MessageType};
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
    /// 搜索匹配元信息（搜索时填充，普通查询为 None）
    pub search_match: Option<crate::models::vector::SearchMatchInfo>,
}

impl Message {
    /// 从 Po 创建 Message
    pub fn from_po(po: MessagePo) -> Self {
        Self {
            po,
            search_match: None,
        }
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

    /// 获取消息类型
    pub fn message_type(&self) -> MessageType {
        self.po.message_type
    }

    pub fn from_role(&self) -> MessageRole {
        self.po.from_role
    }

    pub fn to_role(&self) -> MessageRole {
        self.po.to_role
    }

    /// 获取来源 ID
    pub fn from_id(&self) -> &str {
        &self.po.from_id
    }

    /// 获取目标 ID
    pub fn to_id(&self) -> &str {
        &self.po.to_id
    }

    /// 获取消息内容
    pub fn content(&self) -> &str {
        &self.po.content
    }

    /// 获取创建时间（毫秒时间戳）
    pub fn created_at(&self) -> i64 {
        self.po.created_at
    }

    /// 获取根消息 ID（消息链标识）
    pub fn root_id(&self) -> Option<&str> {
        self.po.root_id.as_deref()
    }

    /// 获取消息状态
    pub fn status(&self) -> MessageStatus {
        self.po.status
    }

    /// 获取回复的消息 ID
    pub fn reply_to_id(&self) -> Option<&str> {
        self.po.reply_to_id.as_deref()
    }

    /// 获取文件类型（附件消息才有值）
    pub fn file_type(&self) -> Option<common::enums::FileType> {
        self.po.file_type
    }

    /// 获取文件元数据（附件消息才有值）
    /// 只有当 file_type 有值时才返回 Some
    pub fn file_meta(&self) -> Option<&crate::models::file::FileMeta> {
        // file_type 存在时才视为附件消息
        if self.po.file_type.is_some() {
            Some(&self.po.file_meta.0)
        } else {
            None
        }
    }

    /// 将消息格式化为 Prompt 可读的字符串
    ///
    /// 委托给 MessagePo::to_prompt()
    pub fn to_prompt(&self) -> String {
        self.po.to_prompt()
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
        reply_to_id: Option<String>,
        root_id: Option<String>,
        organization_id: Option<String>,
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
            reply_to_id,
            root_id,
            organization_id,
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
        Self::new_with_context(
            id,
            None,
            Some(task_id),
            from_id,
            to_id,
            from_role,
            to_role,
            message_type,
            content,
            file_type,
            file_meta,
            None,
            None,
            None,
            created_by,
        )
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
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default, derive_builder::Builder)]
#[builder(setter(into), default)]
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
    /// 引用/回复的父消息 ID（支持消息链）
    /// - 用户回复某条消息时使用
    /// - 工具调用结果关联请求消息时使用
    /// - Agent 思考过程关联上下文时使用
    pub reply_to_id: Option<String>,
    /// 消息链根消息 ID（方便按链拉取）
    /// - 新消息链的首条消息 root_id = 自身 id
    /// - 后续消息继承父消息的 root_id
    pub root_id: Option<String>,
    /// 组织 ID（用于异步消费时重建上下文）
    pub organization_id: Option<String>,
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
    /// 将消息格式化为 Prompt 可读的字符串
    ///
    /// 包含所有关键字段：消息ID、发送者、消息类型、回复关联、任务/项目关联、内容
    /// 使用统一的【】标识格式，便于大模型识别和提取
    pub fn to_prompt(&self) -> String {
        let role_name = match self.from_role {
            MessageRole::User => "用户",
            MessageRole::Agent => "Agent",
            MessageRole::System => "系统",
        };

        let msg_type_label = match self.message_type {
            MessageType::Text => "文本消息",
            MessageType::Image => "图片消息",
            MessageType::File => "文件消息",
            MessageType::Audio => "音频消息",
            MessageType::Video => "视频消息",
            MessageType::ToolCallRequest => "工具调用请求",
            MessageType::ToolCallResult => "工具执行结果",
            MessageType::ConfirmRequest => "确认请求",
            MessageType::ConfirmResponse => "确认回复",
            MessageType::TaskAssignment => "任务分配通知",
        };

        let content_label = match self.message_type {
            MessageType::ToolCallResult => "执行结果",
            MessageType::ToolCallRequest => "调用详情",
            _ => "消息内容",
        };

        let mut msg_parts = vec![
            format!("【消息 ID】{}", self.id),
            format!("【发送者】{}", role_name),
            format!("【消息类型】{}", msg_type_label),
        ];

        if let Some(reply_to) = &self.reply_to_id {
            msg_parts.push(format!("【回复消息】{}", reply_to));
        }

        if let Some(task_id) = &self.task_id {
            msg_parts.push(format!("【关联任务】{}", task_id));
        }

        if let Some(project_id) = &self.project_id {
            msg_parts.push(format!("【关联项目】{}", project_id));
        }

        msg_parts.push(format!("\n【{}】\n{}", content_label, self.content));

        msg_parts.join("\n")
    }

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
        reply_to_id: Option<String>,
        root_id: Option<String>,
        organization_id: Option<String>,
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
            reply_to_id,
            root_id,
            organization_id,
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
    /// 引用的父消息 ID（支持消息链）
    pub reply_to_id: Option<String>,
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
    /// 轻量工具调用追踪引用（结果消息有效），最小包含真实 `{ tool_id, call_id }`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_ref: Option<ToolCallTraceRef>,
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
        reply_to_id: Option<String>,
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
            reply_to_id,
            args: Some(args),
            result: None,
            is_success: None,
            error_message: None,
            result_file_meta: None,
            trace_ref: None,
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
            reply_to_id: self.reply_to_id.clone(),
            args: None,
            result: Some(result),
            is_success: Some(true),
            error_message: None,
            result_file_meta,
            trace_ref: None,
        }
    }

    /// 创建工具调用完成响应（失败）
    pub fn new_error_result(&self, error_message: String) -> Self {
        Self {
            request_id: self.request_id.clone(),
            tool_id: self.tool_id.clone(),
            tool_name: self.tool_name.clone(),
            project_id: self.project_id.clone(),
            task_id: self.task_id.clone(),
            from_id: self.to_id.clone(), // 执行方反过来返回给原发起方
            to_id: self.from_id.clone(),
            reply_to_id: self.reply_to_id.clone(),
            args: None,
            result: None,
            is_success: Some(false),
            error_message: Some(error_message),
            result_file_meta: None,
            trace_ref: None,
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
            reply_to_id: self.reply_to_id.clone(),
            args: None,
            result: Some(result),
            is_success: Some(false),
            error_message: Some(error_message),
            result_file_meta: None,
            trace_ref: None,
        }
    }
}

/// 任务分配消息内容
///
/// 存储在 message.content 中，对应 MessageType::TaskAssignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignmentMessage {
    /// 任务 ID
    pub task_id: String,
    /// 任务标题
    pub task_title: String,
    /// 任务描述（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_description: Option<String>,
    /// 关联项目 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 分配者 ID
    pub from_id: String,
    /// 接收 Agent ID
    pub to_agent_id: String,
}

impl TaskAssignmentMessage {
    pub fn new(
        task_id: String,
        task_title: String,
        task_description: Option<String>,
        project_id: Option<String>,
        from_id: String,
        to_agent_id: String,
    ) -> Self {
        Self {
            task_id,
            task_title,
            task_description,
            project_id,
            from_id,
            to_agent_id,
        }
    }
}

impl crate::pkg::request_context::EnrichContext for MessagePo {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        let agent_id = match (self.from_role, self.to_role) {
            (MessageRole::Agent, _) => Some(self.from_id.as_str()),
            (_, MessageRole::Agent) => Some(self.to_id.as_str()),
            _ => None,
        };
        builder
            .try_project_id(self.project_id.as_deref())
            .try_task_id(self.task_id.as_deref())
            .try_agent_id(agent_id)
    }
}

impl crate::pkg::request_context::EnrichContext for Message {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        self.po.enrich(builder)
    }
}

/// ✅ MessagePo 实现 Vectorizable trait，支持向量索引
///
/// 向量化文本使用 `content` 字段，对应 FTS5 索引的 content 列。
impl crate::models::vector::Vectorizable for MessagePo {
    fn vectorize_text(&self) -> String {
        self.content.clone()
    }

    fn vector_collection() -> &'static str {
        "messages"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::enums::{MessageRole, MessageStatus, MessageType};

    #[test]
    fn test_message_po_builder() {
        // 验证 Builder 模式可以正常工作 - 使用 MessagePoBuilder
        let po = MessagePoBuilder::default()
            .id("msg_001".to_string())
            .project_id(Some("proj_001".to_string()))
            .task_id(Some("task_001".to_string()))
            .from_id("user_001".to_string())
            .to_id("agent_001".to_string())
            .from_role(MessageRole::User)
            .to_role(MessageRole::Agent)
            .message_type(MessageType::Text)
            .file_type(None)
            .status(MessageStatus::Pending)
            .content("Hello, Builder!".to_string())
            .file_meta(Json(FileMeta::default()))
            .reply_to_id(None)
            .root_id(Some("msg_001".to_string()))
            .organization_id(Some("org_001".to_string()))
            .created_by("tester".to_string())
            .modified_by("tester".to_string())
            .created_at(1234567890)
            .updated_at(1234567890)
            .build()
            .unwrap();

        assert_eq!(po.id, "msg_001");
        assert_eq!(po.content, "Hello, Builder!");
        assert_eq!(po.project_id, Some("proj_001".to_string()));
        assert!(po.reply_to_id.is_none());
    }
}
