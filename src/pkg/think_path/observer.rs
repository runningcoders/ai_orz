//! 执行观测：把引擎内部的推进过程**事件化**
//!
//! 引擎只负责「跑节点 → 记 occurrence → 仲裁」，它对观察者一无所知；
//! 观察者在这里拿到与 `RunTrace` 同源的骨架动作（run 开始 / 节点开始 / 节点结束 /
//! 选路 / run 结束），用于实时推送思考图。
//!
//! ⚠️ 观测必须**无副作用**：观察者实现不得失败影响主流程（trait 返回 `()`），
//! 也不得改动 run 的任何状态。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 图中节点目录（run 开始时随 `RunStarted` 下发）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNodeSpec {
    pub id: String,
    pub phase: String,
    pub purpose: String,
    pub priority: i32,
}

/// 图中边目录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdgeSpec {
    pub from: String,
    pub to: String,
    pub on: Option<String>,
}

/// 引擎骨架动作
#[derive(Debug, Clone)]
pub enum GraphTrace {
    /// run 开始：整张图的静态目录
    RunStarted {
        path_id: String,
        entry: String,
        nodes: Vec<GraphNodeSpec>,
        edges: Vec<GraphEdgeSpec>,
        stimulus_brief: String,
    },
    /// 节点开始执行
    NodeStarted {
        node: String,
        phase: String,
        step: usize,
        seq: usize,
        purpose: String,
    },
    /// 节点执行结束（含失败）
    NodeFinished {
        node: String,
        step: usize,
        seq: usize,
        signal: Option<String>,
        failed: bool,
        duration_ms: u64,
    },
    /// 仲裁器选出下一步
    RouteDecided {
        step: usize,
        candidates: Vec<String>,
        chosen: String,
        via: String,
        reason: String,
    },
    /// run 结束（正常或异常）
    RunFinished {
        steps: usize,
        error: Option<String>,
        duration_ms: u64,
    },
}

/// 执行观测者：同步/异步实现均可，实现方自负「不影响主流程」
#[async_trait]
pub trait RunObserver: Send + Sync + std::fmt::Debug {
    async fn on_trace(&self, run_id: &str, trace: GraphTrace);
}
