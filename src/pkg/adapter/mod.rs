//! 外部消息适配者注册中心
//!
//! 作为基础设施层，提供统一的适配者注册与获取能力。
//! 各渠道 DAL（如 `LarkMessageChannelDal`）在 init 时向注册中心注册自己，
//! producer 层通过注册中心获取适配者，运行适配得到内部消息后投递。
//!
//! 设计原则：
//! - pkg/adapter 不依赖任何 DAO/DAL 层类型（纯基础设施）
//! - 适配者以 `Arc<dyn Any + Send + Sync>` 存储，producer 按渠道 downcast 取用
//! - `AdaptedMessage` 为 owned 转换结果，producer 据此构造 `SendToAgentCommand`

pub mod message;

use common::enums::ChannelType;
use common::error::{err, Result};
use once_cell::sync::Lazy;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ==================== 适配结果 ====================

/// 适配后的内部消息（owned）
///
/// 由各渠道 DAL 的 adapt 方法返回，consumer 据此构造 `SendToAgentCommand`
/// 并调用 `MessageDomain::send_to_agent` 完成发送。
#[derive(Debug, Clone)]
pub struct AdaptedMessage {
    /// 发送者 ID（外部用户在内部系统的映射 ID）
    pub from_id: String,
    /// 发送者角色（外部消息一般为 User）
    pub from_role: common::enums::MessageRole,
    /// 目标 Agent ID
    ///
    /// 渠道已绑定 agent_id 时直接填充；未绑定时为 `None`，由 consumer 层
    /// 通过角色路由策略（如 `feishu_reception` tag）决定。
    pub to_agent_id: Option<String>,
    /// 消息内容（纯文本）
    pub content: String,
    /// 关联项目 ID（可选）
    pub project_id: Option<String>,
    /// 关联任务 ID（可选）
    pub task_id: Option<String>,
    /// 引用的父消息 ID（可选，支持消息链）
    pub reply_to_id: Option<String>,
}

// ==================== 注册中心 ====================

/// 适配者注册中心
///
/// 按 `ChannelType` 注册/获取适配者。
/// 适配者以 `Arc<dyn Any + Send + Sync>` 存储，consumer 获取后 downcast 为具体类型。
pub struct AdapterRegistry {
    adapters: RwLock<HashMap<i32, Arc<dyn Any + Send + Sync>>>,
}

impl AdapterRegistry {
    fn new() -> Self {
        Self {
            adapters: RwLock::new(HashMap::new()),
        }
    }

    /// 注册适配者
    ///
    /// `T` 是具体 DAL 类型（如 `LarkMessageChannelDal`）。
    /// 重复注册同一渠道会返回 `Conflict` 错误。
    pub fn register<T>(&self, channel_type: ChannelType, adapter: Arc<T>) -> Result<()>
    where
        T: Any + Send + Sync + 'static,
    {
        let mut map = self.adapters.write().map_err(|e| {
            err!(
                Internal,
                "adapter registry lock poisoned: {}",
                e
            )
        })?;
        let key = channel_type.into();
        if map.contains_key(&key) {
            return Err(err!(
                Conflict,
                "adapter already registered for channel {:?}",
                channel_type
            ));
        }
        map.insert(key, adapter as Arc<dyn Any + Send + Sync>);
        Ok(())
    }

    /// 获取适配者（按具体类型 downcast）
    ///
    /// 返回 `Arc<T>`，类型不匹配返回 `None`。
    pub fn get<T>(&self, channel_type: ChannelType) -> Option<Arc<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        let map = self.adapters.read().ok()?;
        let any = map.get(&i32::from(channel_type))?.clone();
        any.downcast::<T>().ok()
    }

    /// 是否已注册某渠道适配者
    pub fn has(&self, channel_type: ChannelType) -> bool {
        self.adapters
            .read()
            .map(|m| m.contains_key(&i32::from(channel_type)))
            .unwrap_or(false)
    }
}

// ==================== 全局单例 ====================

static REGISTRY: Lazy<AdapterRegistry> = Lazy::new(AdapterRegistry::new);

/// 获取全局注册中心
pub fn registry() -> &'static AdapterRegistry {
    &REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeAdapter {
        name: String,
    }

    #[test]
    fn test_register_and_get() {
        let reg = AdapterRegistry::new();
        let adapter = Arc::new(FakeAdapter {
            name: "lark".to_string(),
        });
        reg.register(ChannelType::Lark, adapter).unwrap();

        let got = reg.get::<FakeAdapter>(ChannelType::Lark).unwrap();
        assert_eq!(got.name, "lark");
    }

    #[test]
    fn test_duplicate_register_returns_conflict() {
        let reg = AdapterRegistry::new();
        let a1 = Arc::new(FakeAdapter {
            name: "a1".to_string(),
        });
        let a2 = Arc::new(FakeAdapter {
            name: "a2".to_string(),
        });
        reg.register(ChannelType::Lark, a1).unwrap();
        let result = reg.register(ChannelType::Lark, a2);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_wrong_type_returns_none() {
        let reg = AdapterRegistry::new();
        let adapter = Arc::new(FakeAdapter {
            name: "lark".to_string(),
        });
        reg.register(ChannelType::Lark, adapter).unwrap();

        struct OtherAdapter;
        assert!(reg.get::<OtherAdapter>(ChannelType::Lark).is_none());
    }

    #[test]
    fn test_get_unregistered_returns_none() {
        let reg = AdapterRegistry::new();
        assert!(reg.get::<FakeAdapter>(ChannelType::Wechat).is_none());
        assert!(!reg.has(ChannelType::Wechat));
    }
}
