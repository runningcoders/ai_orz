//! 微信监听面 DAL 子 trait（WechatListenerDal）
//!
//! 长轮询生命周期，消费方：
//! - finance domain 渠道联动（`sync_listener_for_channel` / `release_listener_for_channel`）
//! - finance domain 凭证变更联动（`rebuild_listeners_for_credential`）

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;

/// 微信监听面 DAL 子 trait
///
/// 与 lark 的差异：iLink 长轮询按 **channel_id** 键控（一个 bot 微信号 = 一个 channel），
/// 凭证变化（bot_id / bot_token / base_url 任一）由 DAO 层凭证指纹机制统一处理为
/// "停旧重建"——上层只需对受影响渠道重新 ensure，无需区分变化维度。
#[async_trait::async_trait]
pub trait WechatListenerDal: Send + Sync {
    /// 单渠道状态变化后的监听同步（启用+开监听 → ensure；否则停止）
    ///
    /// 渠道创建/更新后的联动入口；解析或建停失败仅告警，不影响主操作。
    async fn sync_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel);

    /// 渠道删除后的监听释放
    ///
    /// 解析或停连失败仅告警，不影响主操作。
    async fn release_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel);

    /// 凭证变更后的监听重建（失败仅告警）
    ///
    /// 遍历引用该凭证的渠道：启用且开监听的重新 ensure（凭证指纹变化时
    /// 停旧重建，覆盖 bot_id / bot_token / base_url 任一维度）。
    async fn rebuild_listeners_for_credential(&self, ctx: RequestContext, credential_id: &str);
}
