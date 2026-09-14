//! 邮件监听面 DAL 子 trait（EmailListenerDal）
//!
//! IMAP 受管轮询生命周期，消费方：
//! - finance domain 渠道联动（`sync_listener_for_channel` / `release_listener_for_channel`）
//! - finance domain 凭证变更联动（`rebuild_listeners_for_credential`）

use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;

/// 邮件监听面 DAL 子 trait
///
/// 与微信的差异：IMAP 轮询按 **credential_id** 键控（一个代理邮箱 = 一个轮询单元，
/// 可被 N 个渠道共用），因此停轮询前需确认该凭证不再被任何"启用 + 开监听"的
/// 渠道引用——共享邮箱场景下单个渠道的停用/删除不能误停其他渠道的入站。
#[async_trait::async_trait]
pub trait EmailListenerDal: Send + Sync {
    /// 单渠道状态变化后的监听同步（启用+开监听 → ensure；否则按"仍被引用"检查决定是否停止）
    ///
    /// 渠道创建/更新后的联动入口；解析或建停失败仅告警，不影响主操作。
    async fn sync_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel);

    /// 渠道删除后的监听释放（该凭证无其他引用渠道时停止轮询）
    ///
    /// 解析或停轮询失败仅告警，不影响主操作。
    async fn release_listener_for_channel(&self, ctx: RequestContext, channel: &MessageChannel);

    /// 凭证变更后的监听重建（失败仅告警）
    ///
    /// 仍有启用+开监听渠道引用该凭证 → 重新 ensure（凭证指纹变化时停旧重建）；
    /// 已无引用 → 停止轮询。
    async fn rebuild_listeners_for_credential(&self, ctx: RequestContext, credential_id: &str);
}
