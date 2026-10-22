//! 数据总线（黑板）与上下文供给器
//!
//! 总线是 run 内**唯一数据面**：节点不持有跨节点可变状态，全部走总线。
//!
//! 供给器是「统一出口 + 惰性按需」的落地：节点只**声明** `requires`，
//! 引擎在执行前 `ensure_loaded` 幂等保障。不做惰性，反射弧短路就会退化成
//! 「先全量装配再判快路径」——老链路 `think_loop.rs` 的那个毛病换形式复发。
//!
//! 值是 `serde_json::Value`：总线不做强类型节点 IO，
//! 强类型在 Rust 异构图上必然撞 `Any` 下转型。类型契约由 `NodeContract` 声明式保证。

use super::types::{ContextKey, LoadDepth, LoadFailureMode, Stimulus};
use async_trait::async_trait;
use common::error::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
struct BusEntry {
    depth: LoadDepth,
    value: Value,
}

/// 数据总线：分区 + 深度分级
#[derive(Debug, Clone, Default)]
pub struct ThinkBus {
    entries: BTreeMap<ContextKey, BusEntry>,
}

impl ThinkBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: ContextKey) -> Option<&Value> {
        self.entries.get(&key).map(|e| &e.value)
    }

    pub fn depth_of(&self, key: ContextKey) -> Option<LoadDepth> {
        self.entries.get(&key).map(|e| e.depth)
    }

    /// 已达成深度 ≥ 请求档才算命中
    pub fn satisfies(&self, key: ContextKey, depth: LoadDepth) -> bool {
        self.depth_of(key).is_some_and(|d| d >= depth)
    }

    pub fn insert(&mut self, key: ContextKey, depth: LoadDepth, value: Value) {
        self.entries.insert(key, BusEntry { depth, value });
    }

    /// 失效：节点通过工具改动了业务实体后调用，后续 `requires` 会重新加载
    pub fn invalidate(&mut self, key: ContextKey) {
        self.entries.remove(&key);
    }

    pub fn loaded_keys(&self) -> Vec<ContextKey> {
        self.entries.keys().copied().collect()
    }

    /// 截断后的分区摘要：供 `RouterView` 使用，避免把全量上下文塞给大小脑
    pub fn summary(&self, key: ContextKey, max_chars: usize) -> Option<String> {
        let raw = match self.get(key)? {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if raw.chars().count() <= max_chars {
            Some(raw)
        } else {
            Some(raw.chars().take(max_chars).collect())
        }
    }
}

/// 上下文供给器（统一出口）
#[async_trait]
pub trait ContextProvider: Send + Sync + std::fmt::Debug {
    fn key(&self) -> ContextKey;
    async fn load(&self, stimulus: &Stimulus, depth: LoadDepth) -> Result<Value>;
    fn on_failure(&self) -> LoadFailureMode {
        LoadFailureMode::Degrade
    }
}

/// 供给器集合：按 `ContextKey` 索引，一个 key 一个实现
#[derive(Debug, Default)]
pub struct ProviderSet {
    providers: BTreeMap<ContextKey, Arc<dyn ContextProvider>>,
}

impl ProviderSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(mut self, provider: Arc<dyn ContextProvider>) -> Self {
        self.providers.insert(provider.key(), provider);
        self
    }

    pub fn get(&self, key: ContextKey) -> Option<&Arc<dyn ContextProvider>> {
        self.providers.get(&key)
    }
}
