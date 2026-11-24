//! 执行引擎：驱动一次 run
//!
//! 主循环极简——**引擎不认识任何业务**，只做四件事：
//! `ensure_loaded` → 跑节点 → 记 occurrence → 问仲裁器下一步。
//!
//! ⚠️ 引擎不管并发：实例 per-run 独占、单线程推进；
//! Agent 级并发（awaken / settle / compact 抢同一个 Agent）仍由 `try_set_busy` 状态机负责。

use super::arbiter::{Arbiter, NodePool};
use super::blueprint::ThinkPathBlueprint;
use super::bus::ProviderSet;
use super::node::{NodeContract, NodeOutcome};
use super::observer::{GraphEdgeSpec, GraphNodeSpec, GraphTrace, RunObserver};
use super::run::{NodeOccurrence, RouteRecord, RouteVia, RunTrace, ThinkRun};
use super::types::{LoadFailureMode, NodeId, Stimulus};
use common::error::{Result, bail_err, err};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

pub struct ThinkPathEngine {
    blueprint: Arc<ThinkPathBlueprint>,
    pool: NodePool,
    arbiter: Arbiter,
    providers: ProviderSet,
    /// 执行观测者（可选）：把骨架动作实时推出去，不影响主流程
    observer: Option<Arc<dyn RunObserver>>,
}

impl ThinkPathEngine {
    pub fn new(
        blueprint: ThinkPathBlueprint,
        nodes: Vec<Arc<dyn super::node::ThinkNode>>,
        arbiter: Arbiter,
        providers: ProviderSet,
    ) -> Result<Self> {
        let pool = NodePool::build(&blueprint, nodes)?;
        Ok(Self {
            blueprint: Arc::new(blueprint),
            pool,
            arbiter,
            providers,
            observer: None,
        })
    }

    /// 挂执行观测者（链式）
    pub fn with_observer(mut self, observer: Arc<dyn RunObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn blueprint(&self) -> &ThinkPathBlueprint {
        &self.blueprint
    }

    /// 节点池（含编译后的准入守卫）：供诊断与测试断言拓扑编译结果
    pub fn pool(&self) -> &NodePool {
        &self.pool
    }

    pub async fn run(&self, run_id: impl Into<String>, stimulus: Stimulus) -> Result<ThinkRun> {
        let mut run = ThinkRun {
            run_id: run_id.into(),
            blueprint: Arc::clone(&self.blueprint),
            stimulus,
            bus: super::bus::ThinkBus::new(),
            trace: RunTrace::default(),
            overlay: super::strategy::StrategyOverlay::default(),
        };
        self.run_into(&mut run).await?;
        Ok(run)
    }

    /// 由**调用方持有实例**地跑一次 run
    ///
    /// 与 [`run`](Self::run) 的唯一差别：节点报错时返回 `Err`，但 `run` 仍在调用方手上，
    /// 总线、轨迹、发生的 occurrence 都在里面。
    ///
    /// ⚠️ 这不是便利 API，是刚需：老链路 `awakening.rs` 为此专门发明了
    /// `ThinkLoopProgress`（因为 `messages` 是局部变量，`?` 一传播就没了），
    /// 而实例原本就该是这唯一的真相来源。任何需要「异常中断也要留下做过什么」
    /// 的调用方都应该用这个入口（也从若来的中介代理层是按提纲说的直接可用）。
    pub async fn run_into(&self, run: &mut ThinkRun) -> Result<()> {
        let started = Instant::now();
        self.emit(
            &run.run_id,
            GraphTrace::RunStarted {
                path_id: self.blueprint.id.clone(),
                entry: self.blueprint.entry.clone(),
                nodes: self.graph_nodes(),
                edges: self.graph_edges(),
                stimulus_brief: truncate_chars(&run.stimulus.text, 300),
            },
        )
        .await;

        let result = self.drive(run).await;

        let duration_ms = started.elapsed().as_millis() as u64;
        self.emit(
            &run.run_id,
            GraphTrace::RunFinished {
                steps: run.trace.steps,
                error: result.as_ref().err().map(|e| e.to_string()),
                duration_ms,
            },
        )
        .await;
        result
    }

    /// 主循环：`ensure_loaded` → 跑节点 → 记 occurrence → 仲裁下一步
    async fn drive(&self, run: &mut ThinkRun) -> Result<()> {
        let mut current = NodeId::new(self.blueprint.entry.clone());
        loop {
            if run.trace.steps >= self.blueprint.limits.max_steps {
                bail_err!(
                    Internal,
                    "think path exceeded max_steps: path={} run_id={} max_steps={}",
                    self.blueprint.id,
                    run.run_id,
                    self.blueprint.limits.max_steps
                );
            }

            let node = self.pool.get(&current).ok_or_else(|| {
                err!(
                    Internal,
                    "think path references an unknown node: path={} node={}",
                    self.blueprint.id,
                    current.as_str()
                )
            })?;

            let contract = node.contract();
            self.ensure_loaded(&mut *run, &contract).await?;

            run.trace.steps += 1;
            let step_index = run.trace.steps;
            let seq = run.trace.count(&current) + 1;

            self.emit(
                &run.run_id,
                GraphTrace::NodeStarted {
                    node: current.as_str().to_string(),
                    phase: node.phase().as_str().to_string(),
                    step: step_index,
                    seq,
                    purpose: node.purpose().to_string(),
                },
            )
            .await;

            let node_started = Instant::now();
            let outcome: NodeOutcome = match node.run(&mut run.bus).await {
                Ok(outcome) => outcome,
                Err(e) => {
                    self.emit(
                        &run.run_id,
                        GraphTrace::NodeFinished {
                            node: current.as_str().to_string(),
                            step: step_index,
                            seq,
                            signal: None,
                            failed: true,
                            duration_ms: node_started.elapsed().as_millis() as u64,
                        },
                    )
                    .await;
                    return Err(e);
                }
            };
            let node_duration_ms = node_started.elapsed().as_millis() as u64;
            for patch in &outcome.patches {
                run.overlay.apply(patch.clone());
            }
            run.trace.occurrences.push(NodeOccurrence {
                def: current.clone(),
                seq,
                step_index,
                signal: Some(outcome.signal.clone()),
            });
            self.emit(
                &run.run_id,
                GraphTrace::NodeFinished {
                    node: current.as_str().to_string(),
                    step: step_index,
                    seq,
                    signal: Some(outcome.signal.tag().to_string()),
                    failed: matches!(outcome.signal, super::types::NodeSignal::Fail(_)),
                    duration_ms: node_duration_ms,
                },
            )
            .await;

            let decided = self
                .arbiter
                .select(
                    &self.pool,
                    run,
                    Some((&current, &outcome.signal)),
                    &self.blueprint.limits,
                )
                .await?;

            let Some(decided) = decided else {
                break;
            };
            if decided.via == RouteVia::Advisor {
                run.trace.advisor_calls += 1;
            }
            self.emit(
                &run.run_id,
                GraphTrace::RouteDecided {
                    step: step_index,
                    candidates: decided
                        .candidates
                        .iter()
                        .map(|c| c.as_str().to_string())
                        .collect(),
                    chosen: decided.chosen.as_str().to_string(),
                    via: decided.via.as_str().to_string(),
                    reason: decided.reason.clone(),
                },
            )
            .await;
            run.trace.routes.push(RouteRecord {
                step: step_index,
                candidates: decided.candidates,
                chosen: decided.chosen.clone(),
                via: decided.via,
                reason: decided.reason,
            });
            current = decided.chosen;
        }

        Ok(())
    }

    /// 观测动作出口：无观察者时零开销
    async fn emit(&self, run_id: &str, trace: GraphTrace) {
        if let Some(observer) = &self.observer {
            observer.on_trace(run_id, trace).await;
        }
    }

    /// 静态节点目录（run 开始时一次性下发）
    fn graph_nodes(&self) -> Vec<GraphNodeSpec> {
        self.blueprint
            .nodes
            .iter()
            .map(|spec| GraphNodeSpec {
                id: spec.id.clone(),
                phase: spec.phase.clone(),
                purpose: spec.purpose.clone(),
                priority: spec.priority,
            })
            .collect()
    }

    /// 静态边目录
    fn graph_edges(&self) -> Vec<GraphEdgeSpec> {
        self.blueprint
            .edges
            .iter()
            .map(|edge| GraphEdgeSpec {
                from: edge.from.clone(),
                to: edge.to.clone(),
                on: edge.on.clone(),
            })
            .collect()
    }

    /// 惰性加载：已达成深度 ≥ 请求档才跳过
    ///
    /// 节点的 `requires` + 策略覆盖抬高的下限共同决定实际请求档位。
    async fn ensure_loaded(&self, run: &mut ThinkRun, contract: &NodeContract) -> Result<()> {
        for (key, depth) in &contract.requires {
            let want = match run.overlay.min_depth(*key) {
                Some(floor) if floor > *depth => floor,
                _ => *depth,
            };
            if run.bus.satisfies(*key, want) {
                continue;
            }
            let Some(provider) = self.providers.get(*key) else {
                continue;
            };
            match provider.load(&run.stimulus, want).await {
                Ok(value) => run.bus.insert(*key, want, value),
                Err(e) => match provider.on_failure() {
                    LoadFailureMode::HardFail => return Err(e),
                    LoadFailureMode::Degrade => {
                        crate::log_warn!("think path: 上下文供给失败，按 Degrade 降级继续");
                        run.bus.insert(*key, want, Value::Null);
                    }
                },
            }
        }
        Ok(())
    }
}

/// 按字符截断（避免在多字节字符中间切断）
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}

/// 供测试与灰度对比使用：把 run 的终止原因说清楚
pub fn summarize(run: &ThinkRun) -> String {
    format!(
        "path={} steps={} advisor_calls={} fingerprint={}",
        run.blueprint.id,
        run.trace.steps,
        run.trace.advisor_calls,
        run.trace.fingerprint()
    )
}
