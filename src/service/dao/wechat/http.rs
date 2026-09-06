//! 微信渠道 DAO HTTP 实现
//!
//! 出站 `push` / `test_connection` 委托 `ilink.rs` 协议客户端；
//! 入站长轮询生命周期委托受管 registry（channel_id 键控）。
//! 入站运行状态写回经 `InboundStateWriter` 窄接口注入
//! （init 时接入 message_channel DAO，测试实例不写库）。

use std::sync::{Arc, OnceLock};

use common::error::{Result, err};
use tokio::sync::RwLock;

use super::WechatDao;
use super::ilink::{
    IlinkChannelCredentials, MessageChannelStateWriter, PollLoopRegistry, send_text,
};
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;

// ==================== 工厂方法 + 单例 ====================

static WECHAT_DAO: OnceLock<Arc<dyn WechatDao>> = OnceLock::new();

/// 创建一个全新的微信 DAO 实例（不接入状态写回，用于测试）
pub fn new() -> Arc<dyn WechatDao> {
    Arc::new(WechatDaoHttpImpl::new(None))
}

/// 获取 WechatDao 单例
pub fn dao() -> Arc<dyn WechatDao> {
    WECHAT_DAO.get().cloned().unwrap()
}

/// 初始化单例（接入 message_channel DAO 的 inbound_state 写回）
pub fn init() {
    let writer = Arc::new(MessageChannelStateWriter::new(
        crate::service::dao::message_channel::dao(),
    ));
    let _ = WECHAT_DAO.set(Arc::new(WechatDaoHttpImpl::new(Some(writer))));
}

// ==================== 实现 ====================

pub struct WechatDaoHttpImpl {
    /// 受管长轮询 registry（channel_id 键控）
    poll_loops: PollLoopRegistry,
    /// 入站运行状态写回（init 时注入；测试实例为 None，循环仅内存维护）
    state_writer: Option<Arc<dyn super::ilink::InboundStateWriter>>,
    /// registry 操作锁（防 ensure/stop 并发交错）
    lifecycle: RwLock<()>,
}

impl WechatDaoHttpImpl {
    pub fn new(state_writer: Option<Arc<dyn super::ilink::InboundStateWriter>>) -> Self {
        Self {
            poll_loops: PollLoopRegistry::new(),
            state_writer,
            lifecycle: RwLock::new(()),
        }
    }
}

impl WechatDaoHttpImpl {
    /// 出站对端标识：渠道 `wechat_peer_id`，未配置时回落最近活跃会话
    fn resolve_peer(channel: &MessageChannel) -> Option<String> {
        if let Some(peer) = channel
            .config()
            .wechat_peer_id
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            return Some(peer.to_string());
        }
        // 兜底：从入站运行状态取最近活跃会话（首次入站自动回填前的过渡期）
        channel
            .po
            .inbound_state
            .as_deref()
            .and_then(common::models::InboundState::from_json)
            .and_then(|state| state.sessions.latest().map(|s| s.peer_id.clone()))
    }

    /// 出站 context_token：按 peer 取会话令牌（peer 未命中时回落最近活跃会话）
    fn resolve_context_token(channel: &MessageChannel, peer: &str) -> Option<String> {
        let state = channel
            .po
            .inbound_state
            .as_deref()
            .and_then(common::models::InboundState::from_json)?;
        let session = state
            .sessions
            .get(peer)
            .or_else(|| state.sessions.latest())?;
        session.context_token.clone().filter(|t| !t.is_empty())
    }
}

#[async_trait::async_trait]
impl WechatDao for WechatDaoHttpImpl {
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
        credentials: &IlinkChannelCredentials,
    ) -> Result<()> {
        let content = message.po.content.trim();
        if content.is_empty() {
            return Ok(());
        }
        let peer = Self::resolve_peer(channel).ok_or_else(|| {
            err!(
                InvalidRequest,
                "微信渠道缺少对端标识 wechat_peer_id 且无历史会话 channel_id={}，请先在微信里发一条消息",
                channel.po.id
            )
        })?;
        let context_token = Self::resolve_context_token(channel, &peer).ok_or_else(|| {
            err!(
                InvalidRequest,
                "微信渠道缺少 context_token（会话令牌滚动刷新）channel_id={} peer={}，请让对端先发一条消息再回复",
                channel.po.id,
                peer
            )
        })?;

        send_text(credentials, &peer, &context_token, content).await?;
        log_info!(
            &ctx,
            "wechat_push",
            "推送消息到微信 channel_id={} bot_id={} peer={} len={}",
            channel.po.id,
            credentials.bot_id,
            peer,
            content.len()
        );
        Ok(())
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        credentials: &IlinkChannelCredentials,
    ) -> Result<()> {
        // iLink 无廉价无副作用的探测接口（getconfig 会影响会话态），
        // 阶段一做凭证完整性校验；真实连通性由长轮询循环运行态观察。
        if credentials.bot_token.is_empty() || credentials.bot_id.is_empty() {
            return Err(err!(
                InvalidRequest,
                "微信凭证缺少 bot_token / bot_id，请重新扫码授权"
            ));
        }
        log_info!(
            &ctx,
            "wechat_test_connection",
            "微信凭证校验通过 bot_id={} base_url={}",
            credentials.bot_id,
            credentials.base_url
        );
        Ok(())
    }

    async fn start_polling(
        &self,
        channel: &MessageChannel,
        credentials: &IlinkChannelCredentials,
    ) -> Result<()> {
        // 生命周期操作串行化（ensure 内部读检查 + 写插入需防并发交错）
        let _guard = self.lifecycle.write().await;
        // ensure 失败重建时，旧循环 abort 后给 tokio 一拍回收（join 句柄已 detach，无需等待）
        self.poll_loops
            .ensure(channel, credentials, self.state_writer.clone())
            .await
    }

    async fn stop_polling(&self, channel_id: &str) -> Result<()> {
        let _guard = self.lifecycle.write().await;
        self.poll_loops.stop(channel_id).await;
        Ok(())
    }

    async fn stop_all_polling(&self) -> Result<()> {
        let _guard = self.lifecycle.write().await;
        self.poll_loops.stop_all().await;
        Ok(())
    }

    async fn is_polling(&self, channel_id: &str) -> bool {
        self.poll_loops.is_running(channel_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_with_state(inbound_state: Option<String>, peer: Option<String>) -> MessageChannel {
        let mut po = crate::models::message_channel::MessageChannelPo::new(
            "ch_wx_1".to_string(),
            "org_1".to_string(),
            "user_1".to_string(),
            None,
            common::enums::ChannelType::Wechat,
            "我的微信".to_string(),
            None,
            None,
            None,
            Default::default(),
            "user_1".to_string(),
        );
        po.inbound_state = inbound_state;
        po.config_json.0.wechat_peer_id = peer;
        MessageChannel::from_po(po)
    }

    fn state_with(peer: &str, token: &str) -> String {
        let mut state = common::models::InboundState::default();
        state.sessions.upsert(peer, Some(token.into()), None, 100);
        state.to_json()
    }

    /// 出站对端解析：显式 wechat_peer_id 优先 → 最近活跃会话兜底 → 无
    #[test]
    fn test_resolve_peer_priority() {
        assert_eq!(
            WechatDaoHttpImpl::resolve_peer(&channel_with_state(None, Some("peer_cfg".into())))
                .as_deref(),
            Some("peer_cfg")
        );
        assert_eq!(
            WechatDaoHttpImpl::resolve_peer(&channel_with_state(
                Some(state_with("peer_hist", "tok")),
                None
            ))
            .as_deref(),
            Some("peer_hist")
        );
        assert_eq!(
            WechatDaoHttpImpl::resolve_peer(&channel_with_state(None, None)),
            None
        );
    }

    /// context_token 解析：按 peer 命中；peer 未命中回落最近活跃会话；缺失 None
    #[test]
    fn test_resolve_context_token() {
        let state = state_with("peer_a", "tok_a");
        let ch = channel_with_state(Some(state), None);
        assert_eq!(
            WechatDaoHttpImpl::resolve_context_token(&ch, "peer_a").as_deref(),
            Some("tok_a")
        );
        // peer 未命中 → latest 兜底
        assert_eq!(
            WechatDaoHttpImpl::resolve_context_token(&ch, "peer_other").as_deref(),
            Some("tok_a")
        );
        // 无状态 → None
        assert_eq!(
            WechatDaoHttpImpl::resolve_context_token(&channel_with_state(None, None), "peer_a"),
            None
        );
    }
}
