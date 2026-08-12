//! MessageChannel 实体
//!
//! 对应 SQL 建表语句：`migrations/20260508000000_message_channels.sql`
//!
//! 消息渠道配置：
//! - 支持为用户绑定多个推送渠道
//! - 支持为特定 Agent 绑定专用渠道
//!
//! MessageChannel 持久化对象和完整实体

use common::constants::utils;
use common::enums::{ChannelStatus, ChannelType};
use common::error::{Result, err};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use std::collections::HashMap;

/// 渠道推送选项（沿 deliver → push_to_channel → 各渠道 DAO 传透）
///
/// 上层已加载的实体附带下传，避免下游重复查库；
/// 当前仅飞书渠道消费（按 `lark_credential_id` 从用户凭证库解析应用凭证），
/// 其余渠道接收但不消费。
#[derive(Debug, Clone, Default)]
pub struct ChannelPushOptions {
    /// 归属用户实体（含 identity_credentials 列，飞书凭证解析首选路径）
    pub user: Option<crate::models::user::UserPo>,
}

/// MessageChannel 业务实体
#[derive(Debug, Clone)]
pub struct MessageChannel {
    /// 底层持久化对象
    pub po: MessageChannelPo,
}

impl MessageChannel {
    /// 从 Po 创建 MessageChannel
    pub fn from_po(po: MessageChannelPo) -> Self {
        Self { po }
    }

    /// 转换为 Po
    pub fn into_po(self) -> MessageChannelPo {
        self.po
    }

    /// 获取渠道 ID
    pub fn id(&self) -> &str {
        self.po.id.as_str()
    }

    /// 获取用户 ID
    pub fn user_id(&self) -> &str {
        self.po.user_id.as_str()
    }

    /// 获取 Agent ID（如果有）
    pub fn agent_id(&self) -> Option<&str> {
        self.po.agent_id.as_deref()
    }

    /// 获取渠道类型
    pub fn channel_type(&self) -> ChannelType {
        self.po.channel_type
    }

    /// 获取渠道状态
    pub fn status(&self) -> ChannelStatus {
        self.po.status
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        matches!(self.po.status, ChannelStatus::Active)
    }

    /// 当前状态下允许通过状态更新 Action 切换到的目标状态。
    ///
    /// `Deleted` 是删除 Action 的结果，不允许通过普通状态更新产生；
    /// 已删除渠道为终态，不再提供可切换状态。
    pub fn available_statuses(&self) -> Vec<ChannelStatus> {
        match self.po.status {
            ChannelStatus::Active => vec![ChannelStatus::Active, ChannelStatus::Disabled],
            ChannelStatus::Disabled => vec![ChannelStatus::Disabled, ChannelStatus::Active],
            ChannelStatus::Deleted => vec![],
        }
    }

    /// 判断是否允许通过状态更新 Action 切换到目标状态。
    pub fn can_transition_to(&self, target: ChannelStatus) -> bool {
        self.available_statuses().contains(&target)
    }

    /// 切换渠道状态。
    ///
    /// 只处理依赖自身字段即可判断的简单状态迁移；如果未来规则涉及 Agent 绑定、
    /// 权限、套餐或连通性测试结果，应上移到 Domain service 编排。
    pub fn transition_status(
        &mut self,
        target: ChannelStatus,
        modified_by: impl Into<String>,
    ) -> Result<()> {
        if !self.can_transition_to(target) {
            return Err(err!(
                InvalidRequest,
                "MessageChannel {} cannot transition from {:?} to {:?}",
                self.po.id,
                self.po.status,
                target
            ));
        }

        self.po.status = target;
        self.po.modified_by = modified_by.into();
        self.po.updated_at = utils::current_timestamp_ms();
        Ok(())
    }

    /// 获取配置
    pub fn config(&self) -> &ChannelConfig {
        &self.po.config_json.0
    }
}

/// MessageChannelPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default, derive_builder::Builder)]
#[builder(setter(into), default)]
pub struct MessageChannelPo {
    /// 渠道 ID
    pub id: String,
    /// 组织 ID（多租户隔离）
    pub org_id: String,
    /// 绑定的用户 ID
    pub user_id: String,
    /// 关联的 Agent ID（NULL 表示用户全局默认渠道，不绑定特定 Agent）
    pub agent_id: Option<String>,
    /// 渠道类型
    pub channel_type: ChannelType,
    /// 用户自定义的渠道名称（便于区分多个同类型渠道）
    pub channel_name: String,
    /// Webhook 地址（飞书、Slack、通用 Webhook 等使用）
    pub webhook_url: Option<String>,
    /// 访问 Token（需要鉴权的渠道）
    pub access_token: Option<String>,
    /// 签名密钥/Secret
    pub secret: Option<String>,
    /// 扩展配置 JSON（各渠道的详细配置）
    pub config_json: Json<ChannelConfig>,
    /// 渠道状态
    pub status: ChannelStatus,
    /// 消息范围：项目 ID（NULL=所有项目，非空=仅该项目）
    pub scope_project: Option<String>,
    /// 最后成功推送的时间戳（毫秒）
    pub last_pushed_at: Option<i64>,
    /// 最后一次推送的错误信息
    pub last_error: Option<String>,
    /// 创建人 ID
    pub created_by: String,
    /// 最后修改人 ID
    pub modified_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

impl MessageChannelPo {
    /// 创建新的 MessageChannelPo
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        org_id: String,
        user_id: String,
        agent_id: Option<String>,
        channel_type: ChannelType,
        channel_name: String,
        webhook_url: Option<String>,
        access_token: Option<String>,
        secret: Option<String>,
        config: ChannelConfig,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id,
            org_id,
            user_id,
            agent_id,
            channel_type,
            channel_name,
            webhook_url,
            access_token,
            secret,
            config_json: Json(config),
            status: ChannelStatus::Active,
            scope_project: None,
            last_pushed_at: None,
            last_error: None,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 渠道配置结构体（对应 config_json 字段）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelConfig {
    // 飞书配置
    /// 飞书凭证引用 ID（指向 users.identity_credentials 中的凭证关键 ID）
    ///
    /// 飞书应用凭证为用户级资产，渠道仅存引用；解析时按 ID 查用户凭证库（kind=LarkApp）
    pub lark_credential_id: Option<String>,
    /// 飞书身份模式（auto/bot/user，缺省 auto，落 lark-cli config default-as）
    pub lark_identity_mode: Option<String>,
    /// 飞书用户 Open ID（渠道归属用户的飞书标识）
    ///
    /// 用于私信接入场景：管理员预先创建 User + MessageChannel，
    /// 将用户的飞书 open_id 绑定到渠道，事件到达时按 open_id 查找渠道与归属用户。
    pub lark_open_id: Option<String>,
    /// 飞书用户名称（便于日志和展示，可选）
    pub lark_user_name: Option<String>,
    /// 是否监听该应用的飞书入站私信（缺省视为 true）
    ///
    /// 关闭后渠道仅用于出站推送与 lark_cli 工具身份，不建立 WS 长连接。
    pub lark_listen_inbound: Option<bool>,

    // 微信配置
    /// 微信 App ID
    pub wechat_app_id: Option<String>,
    /// 微信 App Secret
    pub wechat_app_secret: Option<String>,
    /// 微信 Open ID
    pub wechat_open_id: Option<String>,

    // 邮件配置
    /// SMTP 服务器地址
    pub email_smtp_host: Option<String>,
    /// SMTP 服务器端口
    pub email_smtp_port: Option<u16>,
    /// 邮箱用户名
    pub email_username: Option<String>,
    /// 邮箱密码
    pub email_password: Option<String>,
    /// 发件人邮箱
    pub email_from_address: Option<String>,
    /// 收件人邮箱
    pub email_to_address: Option<String>,

    // Slack 配置
    /// Slack Bot Token
    pub slack_bot_token: Option<String>,
    /// Slack 频道 ID
    pub slack_channel_id: Option<String>,

    // 通用 Webhook 配置
    /// HTTP 方法（GET / POST / PUT 等）
    pub webhook_method: Option<String>,
    /// HTTP 请求头
    pub webhook_headers: Option<HashMap<String, String>>,
    /// 请求体模板（支持占位符替换）
    pub webhook_body_template: Option<String>,

    // 其他扩展字段
    pub extra: Option<HashMap<String, String>>,
}

#[cfg(test)]
#[path = "message_channel_tests.rs"]
mod message_channel_tests;
