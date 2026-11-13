//! 外部会话 → 内部渠道绑定 PO（入站路由）
//!
//! 渠道无关：`channel_type` 区分平台，`external_app_id` + `external_tenant_key`
//! + `external_chat_id` 唯一确定一个外部会话（同一应用 / 租户内 chat_id 唯一）。

use common::enums::ChannelType;

/// 外部会话与内部渠道的绑定
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct MessageChannelConversationPo {
    /// 主键
    pub id: String,
    /// 渠道类型
    pub channel_type: ChannelType,
    /// 外部应用 ID（飞书 `app_id`；无应用概念的平台留空串）
    pub external_app_id: String,
    /// 外部租户 key（飞书 `tenant_key`；无租户概念的平台留空串）
    pub external_tenant_key: String,
    /// 外部会话 ID（飞书 `chat_id`，私聊 / 群聊均有）
    pub external_chat_id: String,
    /// 绑定的内部渠道 ID
    pub channel_id: String,
    /// 创建时间（毫秒）
    pub created_at: i64,
    /// 更新时间（毫秒）
    pub updated_at: i64,
}

/// 外部会话绑定实体（PO 的领域包装）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageChannelConversation {
    /// 持久化对象
    pub po: MessageChannelConversationPo,
}

impl MessageChannelConversation {
    /// 从 PO 构造实体
    pub fn from_po(po: MessageChannelConversationPo) -> Self {
        Self { po }
    }

    /// 绑定记录 ID
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 归属渠道 ID
    pub fn channel_id(&self) -> &str {
        &self.po.channel_id
    }

    /// 渠道类型
    pub fn channel_type(&self) -> ChannelType {
        self.po.channel_type
    }

    /// 外部应用 ID
    pub fn app_id(&self) -> &str {
        &self.po.external_app_id
    }

    /// 外部租户 key
    pub fn tenant_key(&self) -> &str {
        &self.po.external_tenant_key
    }

    /// 外部会话 ID
    pub fn chat_id(&self) -> &str {
        &self.po.external_chat_id
    }

    /// 首次绑定时间（毫秒）
    pub fn created_at(&self) -> i64 {
        self.po.created_at
    }

    /// 最近一次绑定刷新时间（毫秒）
    pub fn updated_at(&self) -> i64 {
        self.po.updated_at
    }
}
