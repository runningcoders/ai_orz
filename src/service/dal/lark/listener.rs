//! 飞书监听面 DAL 子 trait（LarkListenerDal）
//!
//! WS 监听生命周期 + 运行监控，消费方：
//! - finance domain 渠道联动（`sync_listener_for_channel` / `release_listener_for_channel`）
//! - finance domain 凭证变更联动（`handover_listeners_after_credential_change`）
//! - health metrics 聚合（`listener_stats`）
//! - consumer 层推送回复（`base`，基础渠道能力透传）

use common::api::LarkWsMetrics;
use common::error::Result;

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::service::dal::message_channel::MessageChannelDal;

/// 飞书监听面 DAL 子 trait
#[async_trait::async_trait]
pub trait LarkListenerDal: Send + Sync {
    /// 确保指定应用的事件监听已建立（幂等）
    ///
    /// 该 app 存在启用且开启入站监听的渠道 → 建连；否则无操作。
    /// 由 Domain 层在渠道创建/启用/开启监听后调用。
    async fn ensure_listener_for(&self, app_id: &str) -> Result<()>;

    /// 该应用已无启用且开启入站监听的渠道引用时停止监听
    ///
    /// 由 Domain 层在渠道禁用/删除/关闭监听后调用。
    async fn release_listener_if_unused(&self, app_id: &str) -> Result<()>;

    /// 单渠道状态变化后的监听同步（启用+开监听 → ensure；否则 release）
    ///
    /// 渠道创建/更新后的联动入口；解析或建停失败仅告警，不影响主操作。
    async fn sync_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel);

    /// 渠道删除后的监听释放（该 app 无其他引用时才真正停连）
    ///
    /// 解析或停连失败仅告警，不影响主操作。
    async fn release_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel);

    /// 凭证变更后的监听移交（失败仅告警）
    ///
    /// - app_id 变化：release 旧 app（引用计数）→ ensure 新 app；
    /// - app_id 不变但 secret 轮换：强制断连重建（旧连接 token 缓存持有旧 secret，
    ///   不重建会导致出站静默失败直到下次断线才自愈）。
    async fn handover_listeners_after_credential_change(
        &self,
        old_app_id: &str,
        new_app_id: &str,
        secret_changed: bool,
    );

    /// WS 连接监控快照透传（供 health metrics 聚合）
    async fn listener_stats(&self) -> LarkWsMetrics;

    /// 基础渠道能力引用（供 consumer 层推送消息时使用）
    fn base(&self) -> &dyn MessageChannelDal;
}
