//! 消息入站适配器注册中心（基础设施层）
//!
//! 作为通用适配器基础设施，提供统一的消息入站适配能力。
//! 各渠道 DAL（如 `LarkMessageChannelDal`）实现 `MessageInboundAdapter` trait，
//! 在 init 时向 `MessageAdapterRegistry` 注册自己。
//!
//! producer 层（`producer/message_channel.rs`）通过中台提供的
//! `start_all` / `stop_all` 统一管理所有渠道监听，
//! 收到的消息统一为 `AdaptedMessage` 格式，通过回调投递。
//!
//! # 分层解耦
//!
//! ```text
//! producer 层（message_channel）
//!     │  只依赖中台（基础设施层）
//!     ▼
//! pkg/adapter/message （中台）
//!     ▲  ▲
//!     │  │  各渠道 DAL 实现 trait 并注册
//! DAL 层（Lark / Wechat / Slack / ...）
//! ```
//!
//! # 新增渠道的步骤
//!
//! 1. DAL 层实现 `MessageInboundAdapter` trait
//! 2. DAL init 时调用 `registry().register(adapter)`
//! 3. producer 零改动，自动获得该渠道的入站消息

use common::enums::ChannelType;
use common::error::{err, Result};
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

use super::AdaptedMessage;

// ==================== 回调接口 ====================

/// 消息适配回调
///
/// 中台将适配后的 `AdaptedMessage` 通过此回调投递到 producer 层。
/// producer 层实现此 trait，负责消息的最终投递（如调用 MessageDomain）。
#[async_trait::async_trait]
pub trait MessageAdapterCallback: Send + Sync {
    async fn on_message(&self, msg: AdaptedMessage) -> Result<()>;
}

// ==================== 适配器接口 ====================

/// 消息入站适配器 trait
///
/// 各渠道 DAL 实现此 trait，向中台注册。
/// 中台通过统一接口管理所有渠道的生命周期。
#[async_trait::async_trait]
pub trait MessageInboundAdapter: Send + Sync {
    /// 渠道类型
    fn channel_type(&self) -> ChannelType;

    /// 启动入站监听
    ///
    /// 收到消息后，调用 `callback.on_message(adapted_msg)` 投递。
    /// 重复启动应返回 `Conflict` 错误。
    async fn start(&self, callback: Arc<dyn MessageAdapterCallback>) -> Result<()>;

    /// 停止入站监听
    ///
    /// 未启动时返回 `Ok(())`。
    async fn stop(&self) -> Result<()>;

    /// 是否正在监听
    fn is_running(&self) -> bool;
}

// ==================== 注册中台 ====================

/// 消息入站适配器注册中台
///
/// 管理所有已注册的渠道适配器，提供统一的启停接口。
pub struct MessageAdapterRegistry {
    adapters: RwLock<Vec<Arc<dyn MessageInboundAdapter>>>,
}

impl MessageAdapterRegistry {
    fn new() -> Self {
        Self {
            adapters: RwLock::new(Vec::new()),
        }
    }

    /// 注册适配器
    ///
    /// 同一渠道类型重复注册返回 `Conflict` 错误。
    pub fn register(&self, adapter: Arc<dyn MessageInboundAdapter>) -> Result<()> {
        let mut list = self.adapters.write().map_err(|e| {
            err!(
                Internal,
                "message adapter registry lock poisoned: {}",
                e
            )
        })?;

        let ct = adapter.channel_type();
        if list.iter().any(|a| a.channel_type() == ct) {
            return Err(err!(
                Conflict,
                "message adapter already registered for channel {:?}",
                ct
            ));
        }

        list.push(adapter);
        Ok(())
    }

    /// 启动所有已注册的适配器
    ///
    /// 逐个调用 `adapter.start(callback)`，某个失败仅记日志，不中断其他渠道。
    pub async fn start_all(&self, callback: Arc<dyn MessageAdapterCallback>) -> Result<()> {
        let adapters = {
            let list = self.adapters.read().map_err(|e| {
                err!(
                    Internal,
                    "message adapter registry lock poisoned: {}",
                    e
                )
            })?;
            list.clone()
        };

        for adapter in &adapters {
            let ct = adapter.channel_type();
            match adapter.start(callback.clone()).await {
                Ok(_) => {
                    sys_info!("message adapter started: {:?}", ct);
                }
                Err(e) => {
                    sys_warn!("message adapter start failed for {:?}: {}", ct, e);
                }
            }
        }

        Ok(())
    }

    /// 停止所有适配器
    pub async fn stop_all(&self) -> Result<()> {
        let adapters = {
            let list = self.adapters.read().map_err(|e| {
                err!(
                    Internal,
                    "message adapter registry lock poisoned: {}",
                    e
                )
            })?;
            list.clone()
        };

        for adapter in &adapters {
            let ct = adapter.channel_type();
            match adapter.stop().await {
                Ok(_) => {
                    sys_info!("message adapter stopped: {:?}", ct);
                }
                Err(e) => {
                    sys_warn!("message adapter stop error for {:?}: {}", ct, e);
                }
            }
        }

        Ok(())
    }

    /// 已注册适配器数量
    pub fn len(&self) -> usize {
        self.adapters.read().map(|l| l.len()).unwrap_or(0)
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 是否已注册某渠道
    pub fn has(&self, channel_type: ChannelType) -> bool {
        self.adapters
            .read()
            .map(|l| l.iter().any(|a| a.channel_type() == channel_type))
            .unwrap_or(false)
    }
}

// ==================== 全局单例 ====================

static REGISTRY: Lazy<MessageAdapterRegistry> = Lazy::new(MessageAdapterRegistry::new);

/// 获取全局消息适配中台
pub fn registry() -> &'static MessageAdapterRegistry {
    &REGISTRY
}
