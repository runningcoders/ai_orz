//! 思考图观测者：把 `pkg::think_path` 的骨架动作翻译成 AOP 事件
//!
//! 定位：**纯桥接**。引擎不认识 AOP，前端不认识引擎；两者在这里对齐成
//! `common::api::focus` 的线格式。
//!
//! 边界：
//! - 观测是旁路，发布失败只记 debug 日志，绝不影响思考主流程；
//! - 事件只是运行时消息，不落库（历史仍由思考轨迹承担）。

use async_trait::async_trait;
use std::sync::Arc;

use crate::models::events::think_graph::{FocusGraphEdge, FocusGraphNode, graph_event};
use crate::pkg::request_context::RequestContext;
use crate::pkg::think_path::{GraphTrace, RunObserver};
use common::api::focus::ThinkGraphAction;

/// 一次 run 的图身份：构造期固定，run 内不可变
pub(crate) struct AopGraphObserver {
    ctx: RequestContext,
    agent_id: String,
    scene: String,
    graph_id: String,
    path: String,
    task_id: Option<String>,
    project_id: Option<String>,
}

impl AopGraphObserver {
    pub(crate) fn new(
        ctx: RequestContext,
        agent_id: String,
        scene: impl Into<String>,
        graph_id: String,
        path: String,
        task_id: Option<String>,
        project_id: Option<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            ctx,
            agent_id,
            scene: scene.into(),
            graph_id,
            path,
            task_id,
            project_id,
        })
    }
}

impl std::fmt::Debug for AopGraphObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AopGraphObserver")
            .field("agent_id", &self.agent_id)
            .field("scene", &self.scene)
            .field("graph_id", &self.graph_id)
            .field("path", &self.path)
            .finish()
    }
}

#[async_trait]
impl RunObserver for AopGraphObserver {
    async fn on_trace(&self, run_id: &str, trace: GraphTrace) {
        // 信封里的 path 以蓝图为权威：构造期传入的 path 只作兜底
        let mut envelope_path = self.path.clone();
        let action = match trace {
            GraphTrace::RunStarted {
                path_id,
                entry,
                nodes,
                edges,
                stimulus_brief,
            } => {
                if !path_id.is_empty() {
                    envelope_path = path_id;
                }
                ThinkGraphAction::RunStarted {
                    entry,
                    nodes: nodes
                        .into_iter()
                        .map(|n| FocusGraphNode {
                            id: n.id,
                            phase: n.phase,
                            purpose: n.purpose,
                            priority: n.priority,
                        })
                        .collect(),
                    edges: edges
                        .into_iter()
                        .map(|e| FocusGraphEdge {
                            from: e.from,
                            to: e.to,
                            on: e.on,
                        })
                        .collect(),
                    stimulus: stimulus_brief,
                }
            }
            GraphTrace::NodeStarted {
                node,
                phase,
                step,
                seq,
                purpose,
            } => ThinkGraphAction::NodeStarted {
                node,
                phase,
                step,
                seq,
                purpose,
            },
            GraphTrace::NodeFinished {
                node,
                step,
                seq,
                signal,
                failed,
                duration_ms,
            } => ThinkGraphAction::NodeFinished {
                node,
                step,
                seq,
                signal,
                failed,
                duration_ms,
            },
            GraphTrace::RouteDecided {
                step,
                candidates,
                chosen,
                via,
                reason,
            } => ThinkGraphAction::RouteDecided {
                step,
                candidates,
                chosen,
                via,
                reason,
            },
            GraphTrace::RunFinished {
                steps,
                error,
                duration_ms,
            } => ThinkGraphAction::RunFinished {
                steps,
                error,
                duration_ms,
            },
        };

        let event = graph_event(
            &self.agent_id,
            &self.scene,
            &self.graph_id,
            run_id,
            &envelope_path,
            self.task_id.clone(),
            self.project_id.clone(),
            action,
        );
        // 观测是旁路：publish 内部已吞掉序列化错误，这里无需再传播
        crate::pkg::aop::publish(&self.ctx, event).await;
    }
}
