//! 微信凭据面 DAL 子 trait（WechatCredentialDal）
//!
//! 凭证解析（引用模式）+ 渠道定位查询，消费方：
//! - finance domain 凭证删除联动（`find_channels_by_credential_id`）
//! - message_channel DAL 出站凭证解析（`resolve_channel_credentials`）

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::service::dao::wechat::ilink::IlinkChannelCredentials;
use common::error::Result;

/// 微信凭据面 DAL 子 trait
#[async_trait::async_trait]
pub trait WechatCredentialDal: Send + Sync {
    /// 解析渠道引用的 iLink 凭证（引用 ID → 凭证行，缺失/失败返回 None）
    ///
    /// 凭证查询复用调用方上下文的连接池（测试隔离友好）。
    async fn resolve_channel_credentials(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Option<IlinkChannelCredentials>;

    /// 查找引用指定凭证的微信渠道（供 Domain 凭证变更/删除联动编排）
    ///
    /// 内存过滤渠道 config_json 的 `wechat_credential_id`（渠道数量有限，可接受）。
    async fn find_channels_by_credential_id(
        &self,
        credential_id: &str,
    ) -> Result<Vec<MessageChannel>>;
}
