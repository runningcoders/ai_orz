//! 图引擎测试夹具：占位节点 + 脚本化顾问 + 计数供给器

use super::arbiter::{RouteAdvice, RouteAdvisor, RouterView};
use super::bus::{ContextProvider, ThinkBus};
use super::node::{NodeContract, NodeOutcome, ThinkNode};
use super::types::{ContextKey, LoadDepth, NodeId, NodeSignal, PhaseId, Stimulus};
use async_trait::async_trait;
use common::error::{Result, err};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 占位节点：可选地要求上下文、写总线、返回固定信号
#[derive(Debug)]
pub struct ProbeNode {
    id: NodeId,
    phase: PhaseId,
    purpose: String,
    signal: NodeSignal,
    requires: Vec<(ContextKey, LoadDepth)>,
    writes: Vec<(ContextKey, Value)>,
}

impl ProbeNode {
    pub fn new(id: impl Into<NodeId>, signal: NodeSignal) -> Self {
        Self {
            id: id.into(),
            phase: PhaseId::Deliberate,
            purpose: String::new(),
            signal,
            requires: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub fn with_phase(mut self, phase: PhaseId) -> Self {
        self.phase = phase;
        self
    }

    pub fn with_purpose(mut self, purpose: impl Into<String>) -> Self {
        self.purpose = purpose.into();
        self
    }

    pub fn with_require(mut self, key: ContextKey, depth: LoadDepth) -> Self {
        self.requires.push((key, depth));
        self
    }

    pub fn with_write(mut self, key: ContextKey, value: Value) -> Self {
        self.writes.push((key, value));
        self
    }
}

#[async_trait]
impl ThinkNode for ProbeNode {
    fn id(&self) -> NodeId {
        self.id.clone()
    }

    fn phase(&self) -> PhaseId {
        self.phase
    }

    fn purpose(&self) -> &str {
        &self.purpose
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: self.requires.clone(),
            writes: self.writes.iter().map(|(k, _)| *k).collect(),
            reads: self.writes.iter().map(|(k, _)| *k).collect(),
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        for (key, value) in &self.writes {
            bus.insert(*key, LoadDepth::Shallow, value.clone());
        }
        Ok(NodeOutcome::new(self.signal.clone()))
    }
}

/// 脚本化顾问：按预设返回固定选择，用于验证「只能选合法候选」
#[derive(Debug)]
pub struct ScriptedAdvisor {
    pub choose: String,
    pub fail: bool,
    pub seen: std::sync::Mutex<Option<RouterView>>,
}

impl ScriptedAdvisor {
    pub fn choose(id: &str) -> Self {
        Self {
            choose: id.to_string(),
            fail: false,
            seen: std::sync::Mutex::new(None),
        }
    }

    pub fn failing() -> Self {
        Self {
            choose: String::new(),
            fail: true,
            seen: std::sync::Mutex::new(None),
        }
    }

    pub fn candidate_ids(&self) -> Vec<String> {
        self.seen
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .map(|v| v.candidates.iter().map(|c| c.id.clone()).collect())
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl RouteAdvisor for ScriptedAdvisor {
    async fn advise(&self, view: &RouterView) -> Result<RouteAdvice> {
        if let Ok(mut guard) = self.seen.lock() {
            *guard = Some(view.clone());
        }
        if self.fail {
            return Err(err!(Internal, "advisor boom"));
        }
        Ok(RouteAdvice {
            choose: self.choose.clone(),
            reason: "scripted".to_string(),
        })
    }
}

/// 计数供给器：验证惰性加载（同深度不重复取数，加深才重新取）
#[derive(Debug)]
pub struct CountingProvider {
    key: ContextKey,
    pub calls: Arc<AtomicUsize>,
}

impl CountingProvider {
    pub fn new(key: ContextKey) -> (Arc<Self>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(Self {
            key,
            calls: Arc::clone(&calls),
        });
        (provider, calls)
    }
}

#[async_trait]
impl ContextProvider for CountingProvider {
    fn key(&self) -> ContextKey {
        self.key
    }

    async fn load(&self, _stimulus: &Stimulus, _depth: LoadDepth) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Value::String("loaded".to_string()))
    }
}
