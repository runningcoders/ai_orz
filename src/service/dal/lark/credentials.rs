//! 飞书凭据面 DAL 子 trait（LarkCredentialDal）
//!
//! 凭据解析 + 渠道定位查询，消费方：
//! - runtime domain 凭据编排（`resolve_credentials_for_user`）
//! - finance domain 凭证删除联动（`find_channels_by_credential_id`）
//! - 入站消息归属定位（`find_channel_by_lark_identity`，见 impl.rs `adapt_lark`）
//!
//! 另含 `LarkDalCredentialResolver`：lark_cli 工具的凭证解析器实现，
//! 在 `service::init` 阶段注册到 pkg 层全局注入口。

use common::error::Result;

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::service::dao::lark::LarkAppCredentials;

/// 飞书凭据面 DAL 子 trait
#[async_trait::async_trait]
pub trait LarkCredentialDal: Send + Sync {
    /// 解析指定用户的飞书应用凭证（lark_cli 工具身份来源）
    ///
    /// 按 `ctx.user_id` 查询该用户启用的 Lark 渠道，经凭证引用解析取可用凭证（已解密），
    /// 附带渠道身份模式（auto/bot/user，缺省 auto）。
    /// 优先取引用**用户默认凭证**（`find_default` 解析链：个人默认 > 个人其他 >
    /// 组织默认 > 组织其他 public）的渠道；默认凭证未被渠道引用时回退第一条可用渠道。
    /// 未绑定或凭证不完整返回 `None`，由调用方给出引导性错误。
    async fn resolve_credentials_for_user(
        &self,
        ctx: &RequestContext,
    ) -> Result<Option<(LarkAppCredentials, String)>>;

    /// 解析渠道引用凭证的 app_id（供按应用分组/过滤与 Domain 生命周期联动）
    async fn resolve_channel_app_id(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> Option<String>;

    /// 查找引用指定凭证的飞书渠道（供 Domain 凭证变更联动编排）
    ///
    /// 内存过滤渠道 config_json 的 `lark_credential_id`（渠道数量有限，可接受）。
    async fn find_channels_by_credential_id(
        &self,
        credential_id: &str,
    ) -> Result<Vec<MessageChannel>>;

    /// 按 app_id + open_id 二维定位启用的飞书渠道
    ///
    /// 多应用模型下同一 open_id 可能存在于不同应用中，
    /// 先按渠道引用凭证解析出的 app_id 过滤再匹配 open_id。
    /// 渠道数量有限（每个用户最多几条），内存过滤可接受。
    async fn find_channel_by_lark_identity(
        &self,
        ctx: RequestContext,
        app_id: &str,
        open_id: &str,
    ) -> Result<Option<MessageChannel>>;
}

// ==================== lark_cli 凭证解析器 ====================

/// lark_cli 工具的凭证解析器实现
///
/// 按 `ctx.user_id` 查该用户启用的 Lark 渠道，经凭证引用解析取解密后凭证 + 身份模式；
/// 在 `service::init` 阶段注册到 pkg 层全局注入口。
pub struct LarkDalCredentialResolver;

#[async_trait::async_trait]
impl crate::pkg::tool_registry::lark_cli::LarkCredentialResolver for LarkDalCredentialResolver {
    async fn resolve(&self, ctx: &RequestContext) -> Result<Option<(String, String, String)>> {
        Ok(super::dal()
            .resolve_credentials_for_user(ctx)
            .await?
            .map(|(c, mode)| (c.app_id, c.app_secret, mode)))
    }
}
