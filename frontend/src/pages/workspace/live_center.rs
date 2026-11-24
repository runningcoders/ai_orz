//! Agent 实时事件中心（专注模式专用）
//!
//! 与「思考轨迹 / 最近工具」的定位完全不同：
//! - 那两者是**事后查询**（分页、按项目聚合、可回看历史）；
//! - 本组件是**现场直播**：事件到了立刻进流，离开页面即丢。
//!
//! 结构：
//! - 大脑层：根图按阶段平铺，节点按出现次数聚合；最近一次跳转走成一条边轨，
//!   新边落图时做一次激活脉冲；`think_round` 子图单独挂在 deliberate / compact
//!   节点下方，不混进根图；
//! - 动作层：工具调用（开始 / 结束）按 call_id 配对，连续同类调用 5s 内折叠
//!   成一张动作卡（`×N`），详情里仍可逐条看；
//! - 事件流：只放关键事件（轮次思考 / 工具 / 小脑 / 生命周期），最新在上；
//!   点击条目由外层决定是就地展开还是铺到左侧详情区，完整轨迹同样可交给外层。
//!
//! ## 流式渲染
//!
//! 事件先进入无渲染的 raw 缓冲，每 [`FLUSH_MS`] 批量应用到展示态：
//! - 高频节点事件只改大脑图，不进流，避免刷屏；
//! - 流长度有上限，重复事件按 key 合并并移到队首（不无限 append）；
//! - 所有入场动画走 CSS，`prefers-reduced-motion` 下自动关闭。

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::chat::AgentTracePanel;
use crate::components::collapsible_content::CollapsibleContent;
use crate::hooks::{EventSourceOptions, use_event_source};
use crate::utils::time::{format_duration, now_ms};
use common::api::focus::{
    CerebellumDecidedEvent, ThinkGraphAction, ThinkGraphEvent, ThinkOutputEvent, ToolStartedEvent,
};

/// 事件批处理间隔：10~20 条/秒的推送合并成约 8 次渲染
const FLUSH_MS: u32 = 120;
/// 流内存上限（超过丢弃最旧）
const MAX_FEED: usize = 60;
/// 流展示上限（卡片内滚动区）
const VISIBLE_FEED: usize = 24;
/// 动作层上限
const MAX_TOOLS: usize = 8;

// ============================== 对外详情 ==============================

/// 从实时事件中心提升到左侧主区展示的文本详情
#[derive(Debug, Clone, PartialEq)]
pub struct LiveDetailText {
    pub title: String,
    pub subtitle: String,
    pub body: String,
}

/// 点击「详情」时交给外层的内容；轨迹类由外层直接渲染完整轨迹面板
#[derive(Debug, Clone, PartialEq)]
pub enum LiveDetail {
    Text(LiveDetailText),
    Trace,
}

// ============================== 展示态 ==============================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeState {
    Idle,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
struct NodeStat {
    id: String,
    phase: String,
    purpose: String,
    count: usize,
    state: NodeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeedKind {
    Lifecycle,
    Brain,
    Think,
    Tool,
    Cerebellum,
}

impl FeedKind {
    fn glyph(self) -> &'static str {
        match self {
            FeedKind::Lifecycle => "◇",
            FeedKind::Brain => "◈",
            FeedKind::Think => "∴",
            FeedKind::Tool => "⌁",
            FeedKind::Cerebellum => "○",
        }
    }

    fn label(self) -> &'static str {
        match self {
            FeedKind::Lifecycle => "生命周期",
            FeedKind::Brain => "大脑",
            FeedKind::Think => "思考",
            FeedKind::Tool => "工具",
            FeedKind::Cerebellum => "小脑",
        }
    }

    fn tone(self) -> &'static str {
        match self {
            FeedKind::Lifecycle => "text-base-content/45",
            FeedKind::Brain => "text-secondary",
            FeedKind::Think => "text-primary",
            FeedKind::Tool => "text-accent",
            FeedKind::Cerebellum => "text-success",
        }
    }
}

/// 入流事件的附加信息，打包传参避免 `push_feed` 参数列表继续膨胀
struct FeedMeta {
    /// 完整载荷（详情页正文 / 内联抽屉）
    payload: String,
    /// 关联的工具调用 id（无则 None）
    call_id: Option<String>,
    /// 事件意图（工具调用 / 思考摘要）；无则回退到 title 里的工具名
    intent: Option<String>,
}

impl FeedMeta {
    /// 只有完整载荷的常见事件（生命周期 / 思考 / 小脑决策）
    fn payload(payload: String) -> Self {
        Self {
            payload,
            call_id: None,
            intent: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct FeedItem {
    id: u64,
    /// 接收时间（本地毫秒）
    at: i64,
    /// 相对上一条事件的间隔（毫秒）
    delta_ms: i64,
    kind: FeedKind,
    title: String,
    detail: String,
    payload: String,
    /// 合并计数（同一 key 短时间内重复）
    count: u32,
    /// 合并 key（空 = 不合并）
    key: String,
    /// 关联的工具调用（用于「在轨迹中查看」定位提示）
    call_id: Option<String>,
    /// 模型自述的事件意图（工具调用 / 思考摘要）：
    /// 信息流优先展示它，缺失时才回退到 `title` 里的工具名等兜底文案
    intent: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct ToolView {
    call_id: String,
    name: String,
    intent: Option<String>,
    args_preview: String,
    /// running / done / failed
    status: &'static str,
    duration_ms: Option<u64>,
    /// 完成后的结果摘要
    detail: String,
    /// 完整载荷（详情抽屉用）
    payload: String,
    /// 最近一次调用的接收时间（用于同类工具聚合）
    at: i64,
    /// 连续同类调用的聚合次数
    burst: u32,
}

/// 轮级环：挂在根图某个节点下的子图
#[derive(Debug, Clone, PartialEq)]
struct NestedGraph {
    id: String,
    parent_node: String,
    scene: String,
    nodes: Vec<NodeStat>,
    active: Option<String>,
    last_node: Option<String>,
    route: Option<(String, String, String)>,
    /// 已走过的边（from → to），最近 8 条
    traversed: Vec<(String, String)>,
    edges: Vec<(String, String)>,
    running: bool,
    order: i64,
    /// 每次选路 +1，用于强制边轨重放高亮动画
    route_tick: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct LiveState {
    /// 事件计数（兼作 feed id）
    seq: u64,
    scene: String,
    path: String,
    stimulus: String,
    nodes: Vec<NodeStat>,
    active: Option<String>,
    last_node: Option<String>,
    /// (from, to, via)
    route: Option<(String, String, String)>,
    /// 本次根 run 的静态边与已走边
    edges: Vec<(String, String)>,
    traversed: Vec<(String, String)>,
    route_tick: u64,
    /// 轮级子图（按出现顺序，最多保留 6 个）
    nested: Vec<NestedGraph>,
    /// 本次根 run 内出现过的全部嵌套图 id（淘汰后仍保留，用于识别迟到事件）
    nested_ids: Vec<String>,
    /// 已淘汰的图 id：其后续事件必须丢弃，不能落进当前根图
    retired_ids: Vec<String>,
    /// 仍在跑的图 id（避免重复 RunFinished 把计数减成负数）
    running_ids: Vec<String>,
    /// 仍在跑的图数量（awaken / settle / 轮级图各自独立）
    running_graphs: usize,
    graphs: Vec<String>,
    outputs: Vec<(usize, String)>,
    cerebellum: Option<String>,
    tools: Vec<ToolView>,
    feed: Vec<FeedItem>,
    last_at: i64,
}

impl LiveState {
    fn nested_mut(&mut self, graph_id: &str) -> Option<&mut NestedGraph> {
        self.nested.iter_mut().find(|g| g.id == graph_id)
    }

    fn nested_count_of(&self, parent_node: &str) -> usize {
        self.nested
            .iter()
            .filter(|g| g.parent_node == parent_node)
            .count()
    }
}

impl LiveState {
    fn apply_batch(&mut self, batch: Vec<serde_json::Value>) {
        for value in batch {
            self.apply_one(value);
        }
    }

    /// 服务端广播溢出：丢帧后图状态不可信，复位图内现场（保留事件流与工具卡）
    fn resync(&mut self, skipped: u64) {
        self.graphs.clear();
        self.nodes.clear();
        self.active = None;
        self.last_node = None;
        self.route = None;
        self.edges.clear();
        self.traversed.clear();
        self.route_tick = 0;
        self.nested.clear();
        self.nested_ids.clear();
        self.running_ids.clear();
        self.running_graphs = 0;
        self.outputs.clear();
        self.cerebellum = None;
        self.push_feed(
            FeedKind::Lifecycle,
            &format!("resync:{}", self.seq + 1),
            "信号中断 · 已重置图状态".to_string(),
            format!("广播积压，跳过 {skipped} 条事件；后续事件将继续累积"),
            FeedMeta::payload(String::new()),
        );
    }

    fn apply_one(&mut self, value: serde_json::Value) {
        let kind = value
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        match kind.as_str() {
            "agent.think.graph" => {
                if let Ok(ev) = serde_json::from_value::<ThinkGraphEvent>(value) {
                    self.apply_graph(ev);
                }
            }
            "agent.think.output" => {
                if let Ok(ev) = serde_json::from_value::<ThinkOutputEvent>(value) {
                    self.apply_output(ev);
                }
            }
            "agent.think.round" => {
                if let Ok(ev) = serde_json::from_value::<ThinkRoundWire>(value) {
                    self.apply_round(ev);
                }
            }
            "agent.tool.started" => {
                if let Ok(ev) = serde_json::from_value::<ToolStartedEvent>(value) {
                    self.apply_tool_started(ev);
                }
            }
            "agent.tool.executed" => {
                if let Ok(ev) = serde_json::from_value::<ToolExecWire>(value) {
                    self.apply_tool_executed(ev);
                }
            }
            "agent.cerebellum.decided" => {
                if let Ok(ev) = serde_json::from_value::<CerebellumDecidedEvent>(value) {
                    self.apply_cerebellum(ev);
                }
            }
            // 服务端广播溢出：中间事件已丢失，图状态不再可信，就地复位
            "focus.resync" => {
                let skipped = value
                    .get("skipped")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_default();
                self.resync(skipped);
            }
            _ => {}
        }
    }

    fn apply_graph(&mut self, ev: ThinkGraphEvent) {
        let scene = ev.scene.clone();
        let graph_id = ev.graph_id.clone();
        // 已淘汰图的迟到事件：直接丢弃，不能落进当前根图
        if self.retired_ids.iter().any(|id| id == &graph_id) {
            return;
        }
        // 只有 run 级事件进详情流；节点级事件不序列化，避免高频事件的无谓开销
        let needs_payload = matches!(
            &ev.action,
            ThinkGraphAction::RunStarted { .. } | ThinkGraphAction::RunFinished { .. }
        );
        let ev_payload = if needs_payload {
            pretty(&ev)
        } else {
            String::new()
        };
        let is_new = !self.graphs.contains(&graph_id);
        if is_new {
            self.graphs.push(graph_id.clone());
            self.running_ids.push(graph_id.clone());
            self.running_graphs = self.running_ids.len();
            if self.graphs.len() > 12 {
                self.evict_old_graphs();
            }
        }

        // 轮级环没有独立蓝图 id 之外的父指针：唯一嵌套来源是 think_round，
        // 且它总在某个节点（deliberate / compact）执行期间启动。
        // 已登记过的嵌套图（含被淘汰的）不能再按「当前 active 节点」重新判父。
        let nested_known = self.nested_ids.iter().any(|id| id == &graph_id);
        let nested_parent = if nested_known {
            self.nested
                .iter()
                .find(|g| g.id == graph_id)
                .map(|g| g.parent_node.clone())
        } else {
            (ev.path == "think_round")
                .then(|| self.active.clone())
                .flatten()
        };
        if let Some(parent_node) = &nested_parent {
            if !nested_known {
                // 只淘汰已结束的子图；全部在跑时宁可多留一张，也不丢运行中状态
                if self.nested.len() >= 6
                    && let Some(idx) = self.nested.iter().position(|g| !g.running)
                {
                    let victim = self.nested.remove(idx);
                    self.retired_ids.push(victim.id);
                }
                self.nested_ids.push(graph_id.clone());
                self.nested.push(NestedGraph {
                    id: graph_id.clone(),
                    parent_node: parent_node.clone(),
                    scene: scene.clone(),
                    nodes: Vec::new(),
                    active: None,
                    last_node: None,
                    route: None,
                    traversed: Vec::new(),
                    edges: Vec::new(),
                    running: true,
                    order: now_ms(),
                    route_tick: 0,
                });
            }
        } else if is_new {
            // 新的根 run：图内状态属于上一次 run，必须整体复位。
            // 事件流（feed）与工具调用是跨 run 的现场记录，保留。
            if !scene.is_empty() {
                self.scene = scene.clone();
            }
            if !ev.path.is_empty() {
                self.path = ev.path.clone();
            }
            // 旧子图 / 旧根图的迟到事件不能再影响新图
            self.retired_ids
                .extend(self.graphs.iter().filter(|id| *id != &graph_id).cloned());
            self.retired_ids.append(&mut self.nested_ids);
            if self.retired_ids.len() > 32 {
                let drop = self.retired_ids.len() - 32;
                self.retired_ids.drain(..drop);
            }
            self.nested.clear();
            self.nodes.clear();
            self.active = None;
            self.last_node = None;
            self.route = None;
            self.edges.clear();
            self.traversed.clear();
            self.route_tick = 0;
            self.outputs.clear();
            self.cerebellum = None;
            // 新 run 的现场重新计数；旧 run 的收尾事件即使迟到也不会再减
            self.running_ids.clear();
            self.running_ids.push(graph_id.clone());
            self.running_graphs = self.running_ids.len();
        }

        // 这里的「是否是嵌套图」必须看登记表（含已淘汰项），
        // 否则被淘汰子图的后续事件会落到根图分支上。
        let is_nested = self.nested_ids.iter().any(|id| id == &graph_id);

        match ev.action {
            ThinkGraphAction::RunStarted {
                entry: _,
                nodes,
                edges,
                stimulus,
            } => {
                let edge_pairs: Vec<(String, String)> =
                    edges.into_iter().map(|e| (e.from, e.to)).collect();
                if is_nested {
                    if let Some(nested) = self.nested_mut(&graph_id) {
                        nested.nodes = nodes
                            .into_iter()
                            .map(|n| NodeStat {
                                id: n.id,
                                phase: n.phase,
                                purpose: n.purpose,
                                count: 0,
                                state: NodeState::Idle,
                            })
                            .collect();
                        nested.edges = edge_pairs;
                        nested.running = true;
                        self.push_feed(
                            FeedKind::Lifecycle,
                            &format!("run:{graph_id}"),
                            "思考开始 · 轮级环".to_string(),
                            format!("挂在 {} 下", nested_parent.as_deref().unwrap_or("")),
                            FeedMeta::payload(ev_payload.clone()),
                        );
                    }
                } else {
                    self.stimulus = stimulus.clone();
                    for node in nodes {
                        self.upsert_node(&node.id, &node.phase, &node.purpose);
                    }
                    self.edges = edge_pairs;
                    self.push_feed(
                        FeedKind::Lifecycle,
                        &format!("run:{graph_id}"),
                        format!("思考开始 · {}", scene_label(&scene)),
                        format!(
                            "{} 个节点 · {} 条边 · {}",
                            self.nodes.len(),
                            self.edges.len(),
                            brief(&stimulus, 30)
                        ),
                        FeedMeta::payload(ev_payload.clone()),
                    );
                }
            }
            ThinkGraphAction::NodeStarted {
                node,
                phase,
                step: _,
                seq,
                purpose,
            } => {
                if is_nested {
                    if let Some(nested) = self.nested_mut(&graph_id) {
                        upsert_in(&mut nested.nodes, &node, &phase, &purpose);
                        if let Some(stat) = nested.nodes.iter_mut().find(|n| n.id == node) {
                            stat.count += 1;
                            stat.state = NodeState::Running;
                        }
                        nested.active = Some(node.clone());
                        nested.last_node = Some(node);
                    }
                } else {
                    self.upsert_node(&node, &phase, &purpose);
                    if let Some(stat) = self.nodes.iter_mut().find(|n| n.id == node) {
                        stat.count += 1;
                        stat.state = NodeState::Running;
                    }
                    self.active = Some(node.clone());
                    self.last_node = Some(node);
                }
                let _ = seq;
            }
            ThinkGraphAction::NodeFinished {
                node,
                step: _,
                seq: _,
                signal,
                failed,
                duration_ms: _,
            } => {
                if is_nested {
                    if let Some(nested) = self.nested_mut(&graph_id) {
                        if let Some(stat) = nested.nodes.iter_mut().find(|n| n.id == node) {
                            stat.state = if failed {
                                NodeState::Failed
                            } else {
                                NodeState::Done
                            };
                        }
                        if nested.active.as_deref() == Some(node.as_str()) {
                            nested.active = None;
                        }
                        nested.last_node = Some(node.clone());
                    }
                } else {
                    if let Some(stat) = self.nodes.iter_mut().find(|n| n.id == node) {
                        stat.state = if failed {
                            NodeState::Failed
                        } else {
                            NodeState::Done
                        };
                    }
                    if self.active.as_deref() == Some(node.as_str()) {
                        self.active = None;
                    }
                    self.last_node = Some(node.clone());
                }
                if failed {
                    let payload = pretty(&serde_json::json!({
                        "graph_id": &graph_id,
                        "scene": &scene,
                        "node": &node,
                        "signal": &signal,
                    }));
                    self.push_feed(
                        FeedKind::Lifecycle,
                        &format!("fail:{}", self.seq + 1),
                        format!("节点失败 · {node}"),
                        "该节点未产出信号，图按失败边继续".to_string(),
                        FeedMeta::payload(payload),
                    );
                }
                let _ = signal;
            }
            ThinkGraphAction::RouteDecided {
                step: _,
                candidates: _,
                chosen,
                via,
                reason,
            } => {
                if is_nested {
                    if let Some(nested) = self.nested_mut(&graph_id) {
                        let from = nested.last_node.clone().unwrap_or_else(|| "—".to_string());
                        push_edge(&mut nested.traversed, &from, &chosen);
                        nested.route = Some((from, chosen.clone(), via.clone()));
                        nested.last_node = Some(chosen.clone());
                        nested.route_tick += 1;
                    }
                } else {
                    let from = self.last_node.clone().unwrap_or_else(|| "—".to_string());
                    push_edge(&mut self.traversed, &from, &chosen);
                    self.route = Some((from, chosen.clone(), via.clone()));
                    self.last_node = Some(chosen.clone());
                    self.route_tick += 1;
                }
                // 选路是骨架信息，保留在图上；只有非确定性选路进流（避免噪音）
                if via != "unique" {
                    let payload = pretty(&serde_json::json!({
                        "graph_id": &graph_id,
                        "scene": &scene,
                        "chosen": &chosen,
                        "via": &via,
                        "reason": &reason,
                    }));
                    self.push_feed(
                        FeedKind::Lifecycle,
                        &format!("route:{}", self.seq + 1),
                        format!("选路 · {via}"),
                        reason,
                        FeedMeta::payload(payload),
                    );
                }
            }
            ThinkGraphAction::RunFinished {
                steps: _,
                error,
                duration_ms,
            } => {
                // 同一张图只减一次；被淘汰图的迟到收尾不会再动当前计数
                if self.running_ids.iter().any(|id| id == &graph_id) {
                    self.running_ids.retain(|id| id != &graph_id);
                    self.running_graphs = self.running_ids.len();
                }
                let edge_count = if is_nested {
                    self.nested
                        .iter()
                        .find(|g| g.id == graph_id)
                        .map(|g| g.traversed.len())
                        .unwrap_or(0)
                } else {
                    self.traversed.len()
                };
                if let Some(nested) = self.nested_mut(&graph_id) {
                    nested.running = false;
                    nested.active = None;
                } else if !is_nested {
                    // 只有根图收尾才清根 active；淘汰子图的迟到事件不能误清
                    self.active = None;
                }
                let label = if is_nested {
                    "轮级环".to_string()
                } else {
                    scene_label(&scene).to_string()
                };
                if let Some(err) = error {
                    self.push_feed(
                        FeedKind::Lifecycle,
                        &format!("run:{graph_id}"),
                        format!("思考异常 · {label}"),
                        brief(&err, 60),
                        FeedMeta::payload(ev_payload),
                    );
                } else {
                    self.push_feed(
                        FeedKind::Lifecycle,
                        &format!("run:{graph_id}"),
                        format!("思考结束 · {label}"),
                        format!("{edge_count} 条边 · {}", format_duration(duration_ms)),
                        FeedMeta::payload(ev_payload),
                    );
                }
            }
        }
    }

    /// 图太多时淘汰最早的（非最新、且不是运行中的子图）及其子图
    fn evict_old_graphs(&mut self) {
        let latest = self.graphs.last().cloned();
        let Some(victim) = self
            .graphs
            .iter()
            .find(|id| {
                Some(*id) != latest.as_ref()
                    && !self.nested.iter().any(|g| &g.id == *id && g.running)
            })
            .cloned()
        else {
            return;
        };
        self.graphs.retain(|id| id != &victim);
        self.nested.retain(|g| g.id != victim);
        // 淘汰后的迟到事件必须丢弃，否则会污染仍在展示的根图
        self.running_ids.retain(|id| id != &victim);
        self.running_graphs = self.running_ids.len();
        self.retired_ids.push(victim);
        if self.retired_ids.len() > 32 {
            let drop = self.retired_ids.len() - 32;
            self.retired_ids.drain(..drop);
        }
    }

    fn apply_output(&mut self, ev: ThinkOutputEvent) {
        let mut text = String::new();
        if !ev.reasoning.trim().is_empty() {
            text.push_str(ev.reasoning.trim());
        }
        if !ev.content.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(ev.content.trim());
        }
        if text.is_empty() {
            return;
        }
        let round = ev.round + 1;
        match self.outputs.iter_mut().find(|(r, _)| *r == round) {
            Some(slot) => slot.1 = text.clone(),
            None => {
                self.outputs.push((round, text.clone()));
                if self.outputs.len() > 8 {
                    self.outputs.drain(..self.outputs.len() - 8);
                }
            }
        }
        // 信息流优先展示「思考内容摘要」，而不是「第 N 轮思考」这类占位标题；
        // 完整思考正文放在 payload 里，点击「详情」可到左侧大宽页查看。
        self.push_feed(
            FeedKind::Think,
            &format!("think:{round}"),
            brief(&text, 72),
            format!("第 {round} 轮思考 · {} 字", text.chars().count()),
            FeedMeta::payload(format!(
                "场景：{}\n轮次：第 {round} 轮\n\n{text}",
                scene_label(&ev.scene)
            )),
        );
    }

    fn apply_round(&mut self, ev: ThinkRoundWire) {
        let round = ev.round_number + 1;
        let detail = if ev.tool_call_count > 0 {
            format!(
                "{} · {} 次工具 · {} tokens",
                format_duration(ev.duration_ms),
                ev.tool_call_count,
                ev.total_tokens
            )
        } else {
            format!(
                "{} · {} tokens（入 {} / 出 {}）",
                format_duration(ev.duration_ms),
                ev.total_tokens,
                ev.tokens_input,
                ev.tokens_output
            )
        };
        self.push_feed(
            FeedKind::Brain,
            &format!("round:{round}"),
            format!("大脑返回 · 第 {round} 轮"),
            detail,
            FeedMeta::payload(pretty(&ev)),
        );
    }

    fn apply_tool_started(&mut self, ev: ToolStartedEvent) {
        let args_preview = brief(&value_text(&ev.args), 48);
        // 信息流以「调用意图」为主标题；工具名只是没有意图时的兜底值
        let intent = ev
            .intent
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| brief(s, 80));
        let title = intent
            .clone()
            .unwrap_or_else(|| format!("调用 · {}", ev.tool_name));
        let detail = if intent.is_some() {
            format!("运行中 · {}", ev.tool_name)
        } else {
            format!("运行中 · {args_preview}")
        };
        let payload = pretty(&ev);
        let _ = self.push_feed(
            FeedKind::Tool,
            &format!("tool:{}", ev.call_id),
            title,
            detail,
            FeedMeta {
                payload: payload.clone(),
                call_id: Some(ev.call_id.clone()),
                intent,
            },
        );
        let now = now_ms();
        // 连续同类工具在 5s 内折叠成一张动作卡（详情仍可在事件流里逐条看）
        let merged = self
            .tools
            .last_mut()
            // 上一张卡还在跑时不能合并：否则 call_id 被覆盖，前一次的完成事件
            // 会对不上，动作层凭空多出一张「永不完成」的卡
            .filter(|t| t.name == ev.tool_name && now - t.at < 5_000 && t.status != "running")
            .map(|last| {
                last.burst += 1;
                last.at = now;
                last.call_id = ev.call_id.clone();
                last.intent = ev.intent.clone();
                last.args_preview = args_preview.clone();
                last.status = "running";
                last.duration_ms = None;
                last.detail.clear();
                last.payload = payload.clone();
            })
            .is_some();
        if !merged {
            self.tools.push(ToolView {
                call_id: ev.call_id.clone(),
                name: ev.tool_name,
                intent: ev.intent,
                args_preview,
                status: "running",
                duration_ms: None,
                detail: String::new(),
                payload,
                at: now,
                burst: 1,
            });
            if self.tools.len() > MAX_TOOLS {
                self.tools.drain(..self.tools.len() - MAX_TOOLS);
            }
        }
    }

    fn apply_tool_executed(&mut self, ev: ToolExecWire) {
        let entry = ev.entry;
        if entry.call_id.is_empty() {
            return;
        }
        let failed =
            matches!(entry.status.as_ref(), Some(ToolStatusWire::Failed)) || entry.error.is_some();
        let status = if failed { "failed" } else { "done" };
        let duration = entry.duration_ms;
        let tool_name = entry.tool_name.clone();
        let result_tail = if let Some(err) = &entry.error {
            format!("失败 · {}", brief(err, 46))
        } else {
            let out = entry.output.as_ref().map(value_text).unwrap_or_default();
            if out.is_empty() {
                format!("完成 · {}", format_duration(duration))
            } else {
                format!("完成 · {} · {}", format_duration(duration), brief(&out, 34))
            }
        };

        let payload = pretty(&entry_payload(&entry));
        if let Some(tool) = self.tools.iter_mut().find(|t| t.call_id == entry.call_id) {
            tool.status = status;
            tool.duration_ms = Some(duration);
            tool.detail = result_tail.clone();
            tool.payload = payload.clone();
            tool.at = now_ms();
        } else {
            // 开始事件没收到（中途打开页面）：补一张完成卡
            let detail = format!("{tool_name} · {result_tail}");
            let _ = self.push_feed(
                FeedKind::Tool,
                &format!("tool:{}", entry.call_id),
                format!("完成 · {tool_name}"),
                detail,
                FeedMeta {
                    payload: payload.clone(),
                    call_id: Some(entry.call_id.clone()),
                    intent: None,
                },
            );
            self.tools.push(ToolView {
                call_id: entry.call_id.clone(),
                name: tool_name.clone(),
                intent: None,
                args_preview: brief(&value_text(&entry.input), 40),
                status,
                duration_ms: Some(duration),
                detail: result_tail.clone(),
                payload: payload.clone(),
                at: now_ms(),
                burst: 1,
            });
            if self.tools.len() > MAX_TOOLS {
                self.tools.drain(..self.tools.len() - MAX_TOOLS);
            }
        }

        // 更新对应的流条目（保持同一条，不追加新行）
        if let Some(item) = self
            .feed
            .iter_mut()
            .find(|f| f.key == format!("tool:{}", entry.call_id))
        {
            item.at = now_ms();
            // 有意图时保留意图主标题，只把状态写进副行；无意图才回退工具名
            if item.intent.is_none() {
                item.title = if failed {
                    format!("失败 · {tool_name}")
                } else {
                    format!("完成 · {tool_name}")
                };
            }
            item.detail = format!("{tool_name} · {result_tail}");
            item.payload = payload;
            item.count = 1;
        }
    }

    fn apply_cerebellum(&mut self, ev: CerebellumDecidedEvent) {
        let mut line = format!("{} · {}", ev.scenes.join("/"), ev.outcome);
        if let Some(degraded) = &ev.degraded {
            line.push_str(&format!("（{degraded}）"));
        }
        if let Some(first) = ev.answers.first() {
            line.push_str(&format!(" · {}", first.summary));
        }
        self.cerebellum = Some(line.clone());
        self.push_feed(
            FeedKind::Cerebellum,
            "cerebellum",
            "小脑决策".to_string(),
            format!("{line} · {}", format_duration(ev.latency_ms)),
            FeedMeta::payload(pretty(&ev)),
        );
    }

    fn upsert_node(&mut self, id: &str, phase: &str, purpose: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            if !phase.is_empty() {
                node.phase = phase.to_string();
            }
            if !purpose.is_empty() {
                node.purpose = purpose.to_string();
            }
            return;
        }
        self.nodes.push(NodeStat {
            id: id.to_string(),
            phase: phase.to_string(),
            purpose: purpose.to_string(),
            count: 0,
            state: NodeState::Idle,
        });
    }

    /// 入流：同 key 合并（移到最新端并计数），超限丢最旧。
    ///
    /// `meta.intent` 是事件意图（工具调用意图 / 思考摘要）；`title` 给兜底文案，
    /// 有意图时由渲染层优先展示 `intent`，工具名只在没有意图时出场。
    fn push_feed(
        &mut self,
        kind: FeedKind,
        key: &str,
        title: String,
        detail: String,
        meta: FeedMeta,
    ) -> u64 {
        let FeedMeta {
            payload,
            call_id,
            intent,
        } = meta;
        let now = now_ms();
        if !key.is_empty()
            && let Some(pos) = self.feed.iter().position(|f| f.key == key)
        {
            let mut item = self.feed.remove(pos);
            item.at = now;
            item.delta_ms = (now - self.last_at).max(0);
            item.kind = kind;
            item.title = title;
            item.detail = detail;
            if !payload.is_empty() {
                item.payload = payload;
            }
            // 意图只在有新值时覆盖：完成事件不带意图，不能把开始时的意图冲掉
            if intent.is_some() {
                item.intent = intent;
            }
            // 同一次调用的 started/executed、同一轮的 reasoning/content 只算一条；
            // 只有小脑决策是真正可重复的同一类事件，保留累计次数。
            item.count = if kind == FeedKind::Cerebellum {
                item.count + 1
            } else {
                1
            };
            item.call_id = call_id;
            let id = item.id;
            self.feed.push(item);
            self.last_at = now;
            return id;
        }

        self.seq += 1;
        let id = self.seq;
        let delta_ms = (now - self.last_at).max(0);
        self.last_at = now;
        self.feed.push(FeedItem {
            id,
            at: now,
            delta_ms,
            kind,
            title,
            detail,
            payload,
            count: 1,
            key: key.to_string(),
            call_id,
            intent,
        });
        if self.feed.len() > MAX_FEED {
            self.feed.drain(..self.feed.len() - MAX_FEED);
        }
        id
    }
}

// ============================== 线上格式 ==============================

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ThinkRoundWire {
    #[serde(default)]
    round_number: usize,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    tool_call_count: usize,
    #[serde(default)]
    total_tokens: u64,
    #[serde(default)]
    tokens_input: u64,
    #[serde(default)]
    tokens_output: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ToolExecWire {
    #[serde(default)]
    entry: ToolEntryWire,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ToolEntryWire {
    #[serde(default)]
    call_id: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    input: serde_json::Value,
    #[serde(default)]
    output: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    status: Option<ToolStatusWire>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[allow(dead_code)] // Started / Completed 只用于反序列化兜底，判定只看 Failed
enum ToolStatusWire {
    #[serde(rename = "Started")]
    Started,
    #[serde(rename = "Completed")]
    Completed,
    #[serde(rename = "Failed")]
    Failed,
}

// ============================== 组件 ==============================

/// Agent 实时事件中心（挂载即订阅，卸载即断开）
///
/// `on_open_detail` 有值时，工具 / 事件 / 轨迹的「详情」交给外层
/// （专注态左侧大宽页）渲染；无值时仍退回卡片内联抽屉，组件可独立使用。
#[component]
pub fn AgentLiveCenter(
    agent_id: String,
    #[props(default)] on_open_detail: Option<EventHandler<LiveDetail>>,
) -> Element {
    let mut state = use_signal(LiveState::default);
    let mut raw = use_signal(Vec::<serde_json::Value>::new);
    let mut open_item = use_signal(|| None::<u64>);
    let mut tool_detail = use_signal(|| Option::<String>::None);
    let mut trace_open = use_signal(|| false);

    // 换 Agent 时重置现场
    use_effect(use_reactive!(|agent_id| {
        let _ = agent_id;
        state.set(LiveState::default());
        raw.write().clear();
        open_item.set(None);
        tool_detail.set(None);
    }));

    // SSE 生命周期统一由 hook 管理：URL 随 Agent 变化自动重连，卸载自动关闭
    let sse_url = if agent_id.is_empty() {
        String::new()
    } else {
        format!("/api/v1/hr/agents/{agent_id}/live")
    };
    let on_message = EventHandler::new(move |data: String| {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) {
            raw.write().push(value);
        }
    });
    use_event_source(sse_url, on_message, EventSourceOptions::default());

    // 批处理：raw 缓冲 → 展示态（约 8 次/秒渲染上限）
    use_future(move || {
        let mut raw = raw;
        let mut state = state;
        async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(FLUSH_MS).await;
                let batch: Vec<serde_json::Value> = {
                    let mut buffer = raw.write();
                    buffer.drain(..).collect()
                };
                if !batch.is_empty() {
                    state.write().apply_batch(batch);
                }
            }
        }
    });

    let live = state.read().clone();
    let has_data = !live.nodes.is_empty() || !live.feed.is_empty();
    let scene = scene_label(&live.scene);
    let tool_calls: u32 = live.tools.iter().map(|t| t.burst).sum();
    // 每个父节点只展示最新的一张轮级子图，避免多轮多次唤醒把卡片撑爆
    let mut seen_parents: Vec<String> = Vec::new();
    let mut nested_latest: Vec<NestedGraph> = Vec::new();
    for sub in live.nested.iter().rev() {
        // 父节点已被淘汰的子图不再单独展示，避免出现「孤儿环」
        if live.nodes.iter().any(|n| n.id == sub.parent_node)
            && !seen_parents.contains(&sub.parent_node)
        {
            seen_parents.push(sub.parent_node.clone());
            nested_latest.push(sub.clone());
        }
    }

    rsx! {
        div { class: "relative overflow-hidden rounded-xl border border-base-content/10 bg-base-100/55 backdrop-blur-md shadow-[0_10px_40px_-18px_rgba(0,0,0,0.65)]",
            // 顶部扫描线：一条极细的信号带，暗示「正在流动」
            div { class: "pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/70 to-transparent" }
            div { class: "pointer-events-none absolute inset-0 overflow-hidden opacity-[0.04]",
                div { class: "al-scan absolute inset-x-0 h-16 bg-gradient-to-b from-transparent via-base-content to-transparent" }
            }

            // ---- 头部 ----
            div { class: "relative flex items-center gap-2 px-3 pt-2.5",
                span { class: "al-live-dot inline-block h-1.5 w-1.5 rounded-full bg-success" }
                span { class: "font-display text-[10px] tracking-[0.24em] text-base-content/60",
                    "AGENT SIGNAL"
                }
                span { class: "ml-auto font-mono text-[9px] text-base-content/35",
                    "{live.seq} evt · {live.running_graphs} flow"
                }
            }
            if !has_data {
                div { class: "px-3 py-6 text-center",
                    div { class: "font-display text-[11px] tracking-[0.2em] text-base-content/35",
                        "WAITING FOR SIGNAL"
                    }
                    div { class: "mt-1 text-[10px] text-base-content/25",
                        "下一次思考 / 工具调用将实时出现在这里"
                    }
                }
            } else {
                // ---- 事件流 ----
                div { class: "px-3 pt-2 pb-2",
                    div { class: "flex items-center gap-2",
                        span { class: "font-display text-[9px] tracking-[0.22em] text-base-content/35",
                            "STREAM"
                        }
                        span { class: "ml-auto font-mono text-[9px] text-base-content/30",
                            title: "{live.path}",
                            "{scene}"
                        }
                        button {
                            class: "rounded border border-base-content/15 px-1.5 py-0.5 font-mono text-[9px] text-base-content/45 hover:border-primary/50 hover:text-primary",
                            onclick: move |_| {
                                if let Some(cb) = on_open_detail {
                                    cb.call(LiveDetail::Trace);
                                } else {
                                    trace_open.set(true);
                                }
                            },
                            "TRACE"
                        }
                    }
                    // 新事件从顶部展开，旧事件被平滑下推；key 保证旧节点只移位不重建
                    div { class: "mt-1 max-h-56 overflow-y-auto pr-0.5",
                        for item in live.feed.iter().rev().take(VISIBLE_FEED) {
                            div { key: "{item.id}", class: "al-feed-enter",
                                div { class: "al-feed-enter-inner",
                                    FeedRow {
                                        item: item.clone(),
                                        expanded: open_item() == Some(item.id),
                                        on_toggle: move |id| {
                                            if open_item() == Some(id) {
                                                open_item.set(None);
                                            } else {
                                                open_item.set(Some(id));
                                            }
                                        },
                                        on_trace: move |_| {
                                            if let Some(cb) = on_open_detail {
                                                cb.call(LiveDetail::Trace);
                                            } else {
                                                trace_open.set(true);
                                            }
                                        },
                                        on_open_detail,
                                    }
                                }
                            }
                        }
                        if live.feed.is_empty() {
                            div { class: "py-1 text-[10px] text-base-content/25",
                                "等待关键事件…"
                            }
                        }
                    }
                }

                // ---- 动作层 ----
                div { class: "mt-2 border-t border-base-content/10 px-3 pt-2",
                    div { class: "flex items-center gap-2",
                        span { class: "font-display text-[9px] tracking-[0.22em] text-base-content/35",
                            "ACTION"
                        }
                        span { class: "ml-auto font-mono text-[9px] text-base-content/30",
                            "{tool_calls} calls"
                        }
                    }
                    if live.tools.is_empty() {
                        div { class: "py-1 text-[10px] text-base-content/25", "暂无工具调用" }
                    } else {
                        div { class: "mt-1 space-y-0.5",
                            for tool in live.tools.iter().rev() {
                                ToolRow {
                                    key: "{tool.call_id}",
                                    tool: tool.clone(),
                                    selected: tool_detail().as_deref() == Some(tool.call_id.as_str()),
                                    on_open_detail,
                                    on_open: {
                                        let tool = tool.clone();
                                        move |id| {
                                            // 有外层详情区时，点击工具卡直接把完整载荷铺到左侧；
                                            // 独立使用时退回右栏内联抽屉。
                                            if let Some(cb) = on_open_detail {
                                                cb.call(LiveDetail::Text(tool_detail_text(&tool)));
                                            } else {
                                                tool_detail.set(Some(id));
                                            }
                                        }
                                    },
                                }
                            }
                        }
                        // 动作详情抽屉：按 call_id 从实时列表取，状态/结果会跟着更新
                        if let Some(open_call) = tool_detail() {
                            if let Some(tool) = live.tools.iter().find(|t| t.call_id == open_call) {
                                div { class: "mt-1.5 rounded-md border border-accent/25 bg-accent/5 p-2",
                                    div { class: "flex items-center gap-2",
                                        span { class: "{tool_dot(tool.status)}" }
                                        span { class: "truncate font-mono text-[11px] text-base-content/80",
                                            "{tool.name}"
                                        }
                                        if let Some(ms) = tool.duration_ms {
                                            span { class: "font-mono text-[9px] text-base-content/35", "{format_duration(ms)}" }
                                        }
                                        button {
                                            class: "ml-auto px-1 text-[10px] text-base-content/30 hover:text-base-content/70",
                                            onclick: move |_| tool_detail.set(None),
                                            "×"
                                        }
                                    }
                                    if let Some(intent) = &tool.intent {
                                        div { class: "mt-1 text-[10px] text-base-content/50", "{intent}" }
                                    }
                                    if !tool.payload.is_empty() {
                                        CollapsibleContent {
                                            len: tool.payload.chars().count(),
                                            max_height: 170,
                                            class: "mt-1",
                                            pre { class: "whitespace-pre-wrap break-all font-mono text-[10px] leading-relaxed text-base-content/55",
                                                "{tool.payload}"
                                            }
                                        }
                                    }
                                    div { class: "mt-1 flex justify-end",
                                        button {
                                            class: "rounded border border-primary/40 px-1.5 py-0.5 text-[9px] text-primary/80 hover:bg-primary/10",
                                            onclick: move |_| {
                                                if let Some(cb) = on_open_detail {
                                                    cb.call(LiveDetail::Trace);
                                                } else {
                                                    trace_open.set(true);
                                                }
                                            },
                                            "在完整轨迹中查看"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ---- 大脑层：根图（阶段分组）+ 已走过的边 + 轮级子图 ----
                div { class: "mt-2 border-t border-base-content/10 px-3 pt-2",
                    div { class: "flex items-center gap-2",
                        span { class: "font-display text-[9px] tracking-[0.22em] text-base-content/35",
                            "CORTEX"
                        }
                        span { class: "ml-auto font-mono text-[9px] text-base-content/25",
                            "{live.traversed.len()}/{live.edges.len()} edges"
                        }
                    }
                    if !live.stimulus.is_empty() {
                        div { class: "mt-0.5 truncate text-[10px] italic text-base-content/30",
                            "{brief(&live.stimulus, 42)}"
                        }
                    }
                    div { class: "mt-1.5 space-y-1",
                        for phase in phase_order() {
                            if live.nodes.iter().any(|n| n.phase == phase) {
                                div { class: "flex items-start gap-1.5",
                                    span { class: "w-9 flex-shrink-0 pt-0.5 text-[9px] tracking-wider text-base-content/30",
                                        "{phase_label(phase)}"
                                    }
                                    div { class: "flex flex-wrap gap-1",
                                        for node in live.nodes.iter().filter(|n| n.phase == phase) {
                                            NodePill {
                                                key: "{node.id}",
                                                node: node.clone(),
                                                active: live.active.as_deref() == Some(node.id.as_str()),
                                                subs: live.nested_count_of(&node.id),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // 边轨：最近 4 个节点组成的实际路径，最后一条边带激活脉冲
                    if !live.traversed.is_empty() {
                        EdgeRail {
                            key: "{live.route_tick}",
                            traversed: live.traversed.clone(),
                            total: live.edges.len(),
                            compact: false,
                        }
                    }
                    // 轮级子图：挂在 deliberate / compact 下的环，不再混进主图
                    for sub in nested_latest.iter() {
                        SubGraphCard { key: "{sub.id}", graph: sub.clone() }
                    }
                    if let Some((round, text)) = live.outputs.last() {
                        div { class: "mt-1.5 border-l-2 border-primary/60 pl-2",
                            div { class: "text-[9px] tracking-wider text-base-content/30",
                                "R{round} · THOUGHT"
                            }
                            CollapsibleContent {
                                len: text.chars().count(),
                                max_height: 66,
                                p { class: "whitespace-pre-wrap text-[11px] leading-snug text-base-content/65",
                                    "{text}"
                                }
                            }
                        }
                    }
                    if let Some(line) = &live.cerebellum {
                        div { class: "mt-1 truncate text-[10px] text-success/80", "{line}" }
                    }
                }
            }

            // ---- 完整轨迹（按需回退到旧版详情模式） ----
            if trace_open() {
                div {
                    class: "fixed inset-0 z-[130] flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm",
                    onclick: move |_| trace_open.set(false),
                    div {
                        class: "flex h-[84vh] w-[min(1180px,94vw)] flex-col overflow-hidden rounded-xl border border-base-content/15 bg-base-200 shadow-2xl",
                        onclick: move |e| e.stop_propagation(),
                        div { class: "flex items-center gap-2 border-b border-base-content/10 px-4 py-2",
                            span { class: "font-display text-[11px] tracking-[0.2em] text-base-content/60",
                                "TRACE / 完整轨迹"
                            }
                            span { class: "text-[10px] text-base-content/30",
                                "实时流只保留关键事件；完整轮次与工具明细在这里"
                            }
                            button {
                                class: "ml-auto btn hud-btn btn-ghost btn-xs",
                                onclick: move |_| trace_open.set(false),
                                "关闭"
                            }
                        }
                        div { class: "min-h-0 flex-1 overflow-hidden",
                            AgentTracePanel { agent_id: agent_id.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NodePill(node: NodeStat, active: bool, subs: usize) -> Element {
    let tone = node_tone(&node, active);
    rsx! {
        span {
            class: "rounded px-1.5 py-0.5 font-mono text-[10px] {tone}",
            title: "{node.purpose}",
            "{node.id}"
            if node.count > 1 {
                span { class: "opacity-60", " ×{node.count}" }
            }
            if subs > 0 {
                span { class: "ml-1 text-secondary", "⊞{subs}" }
            }
        }
    }
}

/// 轮级环：以紧凑子图条挂在父节点下方
#[component]
fn SubGraphCard(graph: NestedGraph) -> Element {
    let status = if graph.running {
        "运行中"
    } else {
        "已结束"
    };
    let status_class = if graph.running {
        "text-primary/80 animate-pulse"
    } else {
        "text-base-content/30"
    };
    rsx! {
        div { class: "mt-1.5 rounded-md border border-secondary/25 bg-secondary/5 px-2 py-1.5",
            div { class: "flex items-center gap-1.5",
                span { class: "font-display text-[9px] tracking-[0.18em] text-secondary/70",
                    "SUBGRAPH"
                }
                span { class: "truncate font-mono text-[9px] text-base-content/35",
                    "{graph.parent_node} · {scene_label(&graph.scene)}"
                }
                span { class: "ml-auto flex-shrink-0 font-mono text-[9px] {status_class}", "{status}" }
            }
            div { class: "mt-1 flex flex-wrap gap-1",
                for node in graph.nodes.iter() {
                    span {
                        class: "rounded px-1 py-0.5 font-mono text-[9px] {node_tone(node, graph.active.as_deref() == Some(node.id.as_str()))}",
                        title: "{node.purpose}",
                        "{node.id}"
                        if node.count > 1 {
                            span { class: "opacity-60", " ×{node.count}" }
                        }
                    }
                }
            }
            if !graph.traversed.is_empty() {
                EdgeRail {
                    key: "{graph.route_tick}",
                    traversed: graph.traversed.clone(),
                    total: graph.edges.len(),
                    compact: true,
                }
            }
        }
    }
}

/// 实际走过的边轨：最近 4 个节点，最后一段箭头做一次激活脉冲
#[component]
fn EdgeRail(traversed: Vec<(String, String)>, total: usize, compact: bool) -> Element {
    let names = path_names(&traversed);
    let text = if compact { "text-[9px]" } else { "text-[10px]" };
    let accent = if compact {
        "text-secondary"
    } else {
        "text-primary"
    };
    rsx! {
        div { class: "mt-1 flex items-center gap-1 overflow-x-auto font-mono {text}",
            for (i, (name, last)) in names.iter().enumerate() {
                if i > 0 {
                    if *last {
                        span { class: "al-edge-flash flex-shrink-0 {accent}", "──▶" }
                    } else {
                        span { class: "flex-shrink-0 text-base-content/25", "──▶" }
                    }
                }
                if *last {
                    span { class: "flex-shrink-0 text-base-content/85", "{name}" }
                } else {
                    span { class: "truncate text-base-content/35", "{name}" }
                }
            }
            if total > 0 {
                span { class: "ml-auto flex-shrink-0 text-[9px] text-base-content/25",
                    "{traversed.len()}/{total}"
                }
            }
        }
    }
}

fn node_tone(node: &NodeStat, active: bool) -> &'static str {
    if active {
        return "al-node-running bg-primary/15 text-primary ring-1 ring-primary/50";
    }
    match node.state {
        NodeState::Failed => "bg-error/15 text-error/90",
        NodeState::Done => "bg-success/10 text-success/80",
        // 嵌套图里可能同时有多个 Running；只有当前 active 那个才脉冲
        NodeState::Running => "bg-primary/10 text-primary/75",
        NodeState::Idle => {
            "border border-dashed border-base-content/10 bg-base-200/30 text-base-content/30"
        }
    }
}

/// 动作层的一行：点击行本身看内联详情；右侧「详情」按钮把完整载荷铺到左侧
#[component]
fn ToolRow(
    tool: ToolView,
    selected: bool,
    on_open: EventHandler<String>,
    on_open_detail: Option<EventHandler<LiveDetail>>,
) -> Element {
    let call_id = tool.call_id.clone();
    let tone = if selected {
        "bg-accent/10 ring-1 ring-accent/25"
    } else {
        "hover:bg-base-content/5"
    };
    rsx! {
        div { class: "flex items-start gap-1",
            button {
                class: "group flex min-w-0 flex-1 items-center gap-2 rounded px-1 py-1 text-left {tone}",
                onclick: move |_| on_open.call(call_id.clone()),
                span { class: "{tool_dot(tool.status)}" }
                span { class: "max-w-[46%] truncate font-mono text-[11px] text-base-content/80",
                    "{tool.name}"
                    if tool.burst > 1 {
                        span { class: "ml-1 text-[9px] text-secondary", "×{tool.burst}" }
                    }
                }
                // 主行内容始终固定为「意图」，完成事件只改状态点与尾部耗时，
                // 不再切换成结果摘要（避免同一行出现两个时间 + 参数导致重复刷屏）。
                span { class: "min-w-0 flex-1 truncate text-[10px] text-base-content/35",
                    "{tool.intent.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or(tool.args_preview.as_str())}"
                }
                if let Some(ms) = tool.duration_ms {
                    span { class: "font-mono text-[9px] text-base-content/35", "{format_duration(ms)}" }
                } else if tool.status == "running" {
                    span { class: "font-mono text-[9px] text-primary/70 animate-pulse", "run" }
                }
            }
            if on_open_detail.is_some() {
                button {
                    class: "mt-0.5 flex-shrink-0 rounded border border-base-content/15 px-1.5 py-0.5 text-[9px] text-base-content/45 hover:border-primary/40 hover:text-primary",
                    onclick: {
                        let tool = tool.clone();
                        move |_| {
                            if let Some(cb) = on_open_detail {
                                cb.call(LiveDetail::Text(tool_detail_text(&tool)));
                            }
                        }
                    },
                    "详情"
                }
            }
        }
    }
}

/// 一条流事件：标题行 + 可展开详情
#[component]
fn FeedRow(
    item: FeedItem,
    expanded: bool,
    on_toggle: EventHandler<u64>,
    on_trace: EventHandler<()>,
    on_open_detail: Option<EventHandler<LiveDetail>>,
) -> Element {
    let id = item.id;
    let kind = item.kind;
    rsx! {
        div { class: "al-feed-item",
            div { class: "flex items-start gap-1",
                button {
                    class: "flex min-w-0 flex-1 items-start gap-2 rounded px-1 py-1 text-left hover:bg-base-content/5",
                    onclick: {
                        let item = item.clone();
                        move |_| {
                            if let Some(cb) = on_open_detail {
                                cb.call(LiveDetail::Text(feed_detail_text(&item)));
                            } else {
                                on_toggle.call(id);
                            }
                        }
                    },
                    span { class: "mt-1 h-1 w-1 flex-shrink-0 rounded-full {feed_dot(kind)}" }
                    div { class: "min-w-0 flex-1",
                        div { class: "flex items-center gap-1.5",
                            span { class: "font-mono text-[9px] text-base-content/25",
                                "{delta_label(item.delta_ms)}"
                            }
                            span { class: "text-[10px] {kind.tone()}", "{kind.glyph()}" }
                            span { class: "truncate text-[11px] text-base-content/75",
                                "{item.title}"
                            }
                            if item.count > 1 {
                                span { class: "font-mono text-[9px] text-base-content/35",
                                    "×{item.count}"
                                }
                            }
                        }
                        div { class: "truncate text-[10px] text-base-content/35",
                            "{item.detail}"
                        }
                    }
                    if on_open_detail.is_none() {
                        span { class: "pt-0.5 text-[9px] text-base-content/25",
                            if expanded { "▾" } else { "▸" }
                        }
                    }
                }
                if on_open_detail.is_some() {
                    button {
                        class: "mt-0.5 flex-shrink-0 rounded border border-base-content/15 px-1.5 py-0.5 text-[9px] text-base-content/45 hover:border-primary/40 hover:text-primary",
                        onclick: {
                            let item = item.clone();
                            move |_| {
                                if let Some(cb) = on_open_detail {
                                    cb.call(LiveDetail::Text(feed_detail_text(&item)));
                                }
                            }
                        },
                        "详情"
                    }
                }
            }
            if expanded {
                div { class: "mb-1 ml-3 rounded-md border border-base-content/10 bg-base-200/40 p-2",
                    if item.payload.is_empty() {
                        div { class: "text-[10px] text-base-content/40",
                            "该事件无额外载荷"
                        }
                    } else {
                        CollapsibleContent {
                            len: item.payload.chars().count(),
                            max_height: 200,
                            pre { class: "whitespace-pre-wrap break-all font-mono text-[10px] leading-relaxed text-base-content/55",
                                "{item.payload}"
                            }
                        }
                    }
                    div { class: "mt-1.5 flex items-center justify-end gap-2",
                        if let Some(call_id) = &item.call_id {
                            span { class: "mr-auto font-mono text-[9px] text-base-content/25",
                                "call {call_id}"
                            }
                        }
                        button {
                            class: "rounded border border-primary/40 px-1.5 py-0.5 text-[9px] text-primary/80 hover:bg-primary/10",
                            onclick: move |_| on_trace.call(()),
                            "在完整轨迹中查看"
                        }
                    }
                }
            }
        }
    }
}

// ============================== 工具函数 ==============================

/// 在任意节点集合中 upsert（根图与轮级子图共用同一套状态规则）
fn upsert_in(nodes: &mut Vec<NodeStat>, id: &str, phase: &str, purpose: &str) {
    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
        if !phase.is_empty() {
            node.phase = phase.to_string();
        }
        if !purpose.is_empty() {
            node.purpose = purpose.to_string();
        }
        return;
    }
    nodes.push(NodeStat {
        id: id.to_string(),
        phase: phase.to_string(),
        purpose: purpose.to_string(),
        count: 0,
        state: NodeState::Idle,
    });
}

/// 记录一条已走过的边（保留最近 8 条）
fn push_edge(traversed: &mut Vec<(String, String)>, from: &str, to: &str) {
    traversed.push((from.to_string(), to.to_string()));
    if traversed.len() > 8 {
        traversed.drain(..traversed.len() - 8);
    }
}

/// 把边序列压成节点序列，只留最后 4 个；最后一个是当前落点
fn path_names(traversed: &[(String, String)]) -> Vec<(String, bool)> {
    let mut nodes: Vec<String> = Vec::new();
    for (i, (from, to)) in traversed.iter().enumerate() {
        if i == 0 {
            nodes.push(from.clone());
        }
        nodes.push(to.clone());
    }
    if nodes.len() > 4 {
        nodes.drain(..nodes.len() - 4);
    }
    let last = nodes.len().saturating_sub(1);
    nodes
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, i == last))
        .collect()
}

fn scene_label(scene: &str) -> &str {
    match scene {
        "awaken" => "唤醒",
        "settle" => "沉淀",
        "" => "待命",
        other => other,
    }
}

fn phase_order() -> [&'static str; 7] {
    [
        "stimulus",
        "reflex",
        "triage",
        "prepare",
        "deliberate",
        "consolidate",
        "review",
    ]
}

fn phase_label(phase: &str) -> &str {
    match phase {
        "stimulus" => "刺激",
        "reflex" => "反射",
        "triage" => "分诊",
        "prepare" => "准备",
        "deliberate" => "思考",
        "consolidate" => "收尾",
        "review" => "复核",
        other => other,
    }
}

fn tool_detail_text(tool: &ToolView) -> LiveDetailText {
    let mut body = String::new();
    body.push_str(&format!("调用 ID：{}\n", tool.call_id));
    if let Some(intent) = &tool.intent {
        body.push_str(&format!("意图：{intent}\n"));
    }
    if let Some(ms) = tool.duration_ms {
        body.push_str(&format!("耗时：{}\n", format_duration(ms)));
    }
    if !tool.args_preview.is_empty() {
        body.push_str(&format!("\n[入参]\n{}\n", tool.args_preview));
    }
    if !tool.detail.is_empty() {
        body.push_str(&format!("\n[结果]\n{}\n", tool.detail));
    }
    if !tool.payload.is_empty() {
        body.push_str(&format!("\n[完整载荷]\n{}\n", tool.payload));
    }
    LiveDetailText {
        title: format!("工具调用 · {}", tool.name),
        subtitle: format!("{} · {}", tool.status, tool.call_id),
        body,
    }
}

fn feed_detail_text(item: &FeedItem) -> LiveDetailText {
    // 思考事件的 payload 已是可读正文（场景 / 轮次 / 完整思考）；
    // 不再拼「[完整载荷]」，避免把散文当 JSON 展示。
    if item.kind == FeedKind::Think && !item.payload.is_empty() {
        return LiveDetailText {
            title: item.title.clone(),
            subtitle: item.detail.clone(),
            body: item.payload.clone(),
        };
    }
    let mut body = String::new();
    if !item.detail.is_empty() {
        body.push_str(&item.detail);
    }
    if !item.payload.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str("[完整载荷]\n");
        body.push_str(&item.payload);
    }
    LiveDetailText {
        title: item.title.clone(),
        subtitle: match &item.call_id {
            Some(id) => format!("{} · call {}", item.kind.label(), id),
            None => item.kind.label().to_string(),
        },
        body,
    }
}

fn tool_dot(status: &str) -> &'static str {
    match status {
        "running" => "h-1.5 w-1.5 flex-shrink-0 rounded-full bg-primary animate-pulse",
        "failed" => "h-1.5 w-1.5 flex-shrink-0 rounded-full bg-error",
        _ => "h-1.5 w-1.5 flex-shrink-0 rounded-full bg-success/80",
    }
}

fn feed_dot(kind: FeedKind) -> &'static str {
    match kind {
        FeedKind::Lifecycle => "bg-base-content/30",
        FeedKind::Brain => "bg-secondary",
        FeedKind::Think => "bg-primary",
        FeedKind::Tool => "bg-accent",
        FeedKind::Cerebellum => "bg-success",
    }
}

/// 相对上一条事件的间隔（`+` 前缀 + 最佳单位时长；`·` = 无间隔）
fn delta_label(delta_ms: i64) -> String {
    if delta_ms <= 0 {
        "·".to_string()
    } else {
        format!("+{}", format_duration(delta_ms as u64))
    }
}

/// 按字符数截断（不切断多字节）
fn brief(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        let mut out: String = one_line.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn pretty<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn entry_payload(entry: &ToolEntryWire) -> serde_json::Value {
    serde_json::json!({
        "call_id": entry.call_id,
        "tool_name": entry.tool_name,
        "duration_ms": entry.duration_ms,
        "status": entry.status,
        "input": entry.input,
        "output": entry.output,
        "error": entry.error,
    })
}
