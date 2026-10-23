//! 思考路径图视图（图 / 列表双视图 + 缩略图 + 放大弹窗）
//!
//! # 为什么抽成公共组件
//!
//! 同一份 `occurrences` / `routes` 有两个消费点：聊天侧栏「思考」Tab（`think_traces_tab`）
//! 与 Agent 详情页轨迹面板（`agent_trace_panel`）。两处各写一套渲染必然漂移——
//! 改了节点卡片上的次数口径，另一处还是旧样子。故这里收敛为单一组件，两处只传数据。
//!
//! # 交互形态（与任务依赖图同款范式）
//!
//! - 缩略图：300×200 点线形态（小画布下卡片会互相压住，点线看结构最清楚），
//!   悬停看节点/边的明细（`×N`、`seq#s @step#t`、选路 via/reason）；
//! - 图 / 列表双视图：图不可读时退列表，列表侧永远保留（老记录降级同理）；
//! - 放大弹窗：920×620 卡片形态 + `auto_fit` 初始自适应 + 缩放上限 3.0。
//!
//! 数据为空（老记录没有 occurrences）时不画空图，直接降级成选路记录文本。

use dioxus::prelude::*;

use common::api::{ThinkTraceOccurrenceDto, ThinkTraceRouteDto};

use crate::components::graph::Graph;
use crate::components::modal::Modal;
use crate::utils::think_path_graph::{build_think_path_graph, edge_style_legend, node_detail};

use super::think_traces_tab::route_label;

/// 缩略图画布尺寸（与任务依赖图同口径：300×200 在 w-96 / w-80 侧栏都不溢出）
const MINI_W: f64 = 300.0;
const MINI_H: f64 = 200.0;

/// 放大弹窗画布尺寸（max-w-5xl 弹窗内容区，减去弹窗内边距）
const ZOOM_W: f64 = 920.0;
const ZOOM_H: f64 = 620.0;

/// 思考路径图视图
///
/// - `occurrences`：节点执行记录（同一节点多次执行合成一卡，卡片带 `×N`）
/// - `routes`：选路记录（派生边样式与边标签尾；与 occurrences 不一致时以 occurrence 为准）
#[component]
pub fn ThinkPathGraphView(
    occurrences: Vec<ThinkTraceOccurrenceDto>,
    routes: Vec<ThinkTraceRouteDto>,
) -> Element {
    let mut show_graph = use_signal(|| true);
    let mut zoom_open = use_signal(|| false);

    // 老记录降级：没有节点序列就不画空图（图中无节点可读，纯噪音）
    if occurrences.is_empty() {
        return rsx! {
            div { class: "space-y-1",
                div { class: "text-xs text-base-content/60",
                    "该轨迹未采集节点序列（老记录），以下为选路记录"
                }
                if routes.is_empty() {
                    div { class: "text-[11px] text-base-content/40", "无节点序列与选路记录" }
                } else {
                    for (i, r) in routes.iter().enumerate() {
                        div {
                            key: "{i}",
                            class: "text-[11px] font-mono break-all text-base-content/70",
                            "{route_label(r)}"
                        }
                    }
                }
            }
        };
    }

    // 缩略图与弹窗共用同一份映射函数，只是画布尺寸不同（曲率/布局随尺寸重算）
    let mini = build_think_path_graph(&occurrences, &routes, MINI_W, MINI_H);
    let zoom = build_think_path_graph(&occurrences, &routes, ZOOM_W, ZOOM_H);
    let legend = edge_style_legend();
    let node_count = mini.nodes.len();
    let edge_count = mini.edges.len();

    rsx! {
        div { class: "space-y-1",
            div { class: "flex items-center justify-between gap-1",
                span { class: "text-[11px] text-base-content/60",
                    "节点 {node_count} · 转移 {edge_count}"
                }
                div { class: "flex gap-1",
                    button {
                        class: if show_graph() { "btn hud-btn btn-xs btn-primary" } else { "btn hud-btn btn-xs btn-ghost" },
                        onclick: move |_| show_graph.set(true),
                        "图"
                    }
                    button {
                        class: if show_graph() { "btn hud-btn btn-xs btn-ghost" } else { "btn hud-btn btn-xs btn-primary" },
                        onclick: move |_| show_graph.set(false),
                        "列表"
                    }
                    button {
                        class: "btn hud-btn btn-ghost btn-xs",
                        title: "放大查看",
                        onclick: move |_| zoom_open.set(true),
                        "🔍"
                    }
                }
            }
            if show_graph() {
                div { class: "relative rounded-lg border border-base-300 bg-base-100 overflow-hidden",
                    Graph {
                        nodes: mini.nodes.clone(),
                        edges: mini.edges.clone(),
                        svg_width: Some(MINI_W as u32),
                        svg_height: Some(MINI_H as u32),
                        // 缩略图走 mini 点线形态（与任务依赖图小图同款口径）：
                        // 300×200 里塞矩形卡片必然互相压住，压成一团既看不出拓扑、
                        // 也认不出节点；点线形态至少把「几个节点、怎么连、什么类型色」
                        // 看干净，名称与明细交给 hover 卡，完整卡片在放大弹窗里看。
                        mini: true,
                        auto_fit: true,
                        on_node_click: move |_id: String| {},
                    }
                    // 图例：画布左下角，穿透点击不挡图
                    div { class: "absolute left-1 bottom-1 space-y-0.5 pointer-events-none",
                        for (i, item) in legend.iter().enumerate() {
                            div {
                                key: "{i}",
                                class: "text-[9px] leading-tight text-base-content/50",
                                "{item}"
                            }
                        }
                    }
                }
            } else {
                div { class: "space-y-1",
                    // 列表侧：一节点一行（同名节点聚合，`×N` 与 signal 标签同卡）
                    for agg in mini.aggs.iter() {
                        div {
                            key: "{agg.node}",
                            class: "rounded border border-base-300 bg-base-100 p-1",
                            div { class: "flex items-center gap-1 flex-wrap",
                                span { class: "badge orz-tag badge-sm", "{agg.node}" }
                                if agg.count > 1 {
                                    span { class: "badge badge-xs badge-warning badge-soft", "×{agg.count}" }
                                }
                                for (i, sig) in agg.signals.iter().enumerate() {
                                    span { key: "{i}", class: "badge badge-xs badge-ghost", "{sig}" }
                                }
                            }
                            div { class: "text-[10px] text-base-content/50 font-mono break-all",
                                "{node_detail(agg)}"
                            }
                        }
                    }
                    if !routes.is_empty() {
                        div { class: "space-y-1 pt-1",
                            label { class: "form-label", "选路记录" }
                            for (i, r) in routes.iter().enumerate() {
                                div {
                                    key: "{i}",
                                    class: "text-[11px] font-mono break-all text-base-content/70",
                                    "{route_label(r)}"
                                }
                            }
                        }
                    }
                }
            }
        }
        if zoom_open() {
            Modal {
                show: true,
                title: "思考路径图".to_string(),
                on_close: move |_| zoom_open.set(false),
                width_class: Some("max-w-5xl".to_string()),
                div { class: "space-y-1",
                    Graph {
                        nodes: zoom.nodes.clone(),
                        edges: zoom.edges.clone(),
                        svg_width: Some(ZOOM_W as u32),
                        svg_height: Some(ZOOM_H as u32),
                        // 弹窗用卡片形态 + 初始 fit：节点少时自动放大到选框内，
                        // 缩放上限已放到 3.0，弧线细节可继续放大看
                        auto_fit: true,
                        on_node_click: move |_id: String| {},
                    }
                    div { class: "flex flex-wrap gap-x-3 gap-y-0.5",
                        for (i, item) in legend.iter().enumerate() {
                            span { key: "{i}", class: "text-[10px] text-base-content/50", "{item}" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use common::api::ThinkTraceOccurrenceDto;

    use crate::utils::think_path_graph::build_think_path_graph;

    fn occ(node: &str, seq: usize, step: usize) -> ThinkTraceOccurrenceDto {
        ThinkTraceOccurrenceDto {
            node: node.to_string(),
            seq,
            step,
            signal: None,
        }
    }

    /// 缩略图与放大大弹窗共用同一份映射函数，只是画布尺寸不同：
    /// 节点/边数量必须一致（同一份数据），但布局坐标必须随画布重算
    /// （层间距按高度取比例，弹窗里层距更宽 —— 不是把缩略图坐标硬放大）。
    #[test]
    fn same_data_renders_at_both_canvas_sizes_with_recomputed_layout() {
        let occs = vec![
            occ("Triage", 1, 1),
            occ("Deliberate", 1, 2),
            occ("Execute", 1, 3),
        ];
        let mini = build_think_path_graph(&occs, &[], super::MINI_W, super::MINI_H);
        let zoom = build_think_path_graph(&occs, &[], super::ZOOM_W, super::ZOOM_H);
        assert_eq!(
            mini.nodes.len(),
            zoom.nodes.len(),
            "同一份数据节点数必须一致"
        );
        assert_eq!(mini.edges.len(), zoom.edges.len(), "同一份数据边数必须一致");

        let span = |nodes: &[crate::components::graph::GraphNode]| -> f64 {
            let top = nodes.iter().map(|n| n.y).fold(f64::MAX, f64::min);
            let bottom = nodes.iter().map(|n| n.y).fold(f64::MIN, f64::max);
            bottom - top
        };
        assert!(
            span(&zoom.nodes) > span(&mini.nodes),
            "弹窗层距应比缩略图更宽松: {} vs {}",
            span(&zoom.nodes),
            span(&mini.nodes)
        );
    }
}
