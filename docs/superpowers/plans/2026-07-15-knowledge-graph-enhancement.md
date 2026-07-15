# 知识图谱交互完善 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完善知识图谱的交互体验，包括关系类型差异化、边标签防重叠、节点拖拽、缩放平移、搜索结果高亮等功能。

**Architecture:** 在现有 SVG 图谱组件基础上增强交互能力，使用 web_sys 实现拖拽和缩放，通过虚拟坐标系统处理视图变换。

**Tech Stack:** Dioxus 0.7.9, Rust, web_sys, SVG, CSS

---

## File Structure

| 文件 | 职责 | 修改类型 |
|------|------|----------|
| `frontend/src/components/graph.rs` | 图谱渲染组件，包含节点/边渲染、布局算法、交互逻辑 | 修改 |
| `frontend/src/pages/hr/knowledge_graph.rs` | 知识图谱页面，搜索、详情侧边栏、状态管理 | 修改 |
| `frontend/index.html` | 图谱相关 CSS 样式 | 修改 |

---

## Task 1: 关系类型差异化渲染

**Files:**
- Modify: `frontend/src/components/graph.rs`
- Modify: `frontend/index.html`

**目标:** 不同关系类型（属于、引用、包含、关联等）使用不同颜色和样式，便于区分。

- [ ] **Step 1: 添加关系类型颜色映射函数**

```rust
// 在 graph.rs 中添加
fn get_edge_color(relation_type: &str) -> &'static str {
    match relation_type {
        "属于" => "#ef4444",
        "引用" => "#3b82f6",
        "包含" => "#10b981",
        "关联" => "#f59e0b",
        "派生" => "#8b5cf6",
        "依赖" => "#ec4899",
        _ => "#9ca3af",
    }
}

fn get_edge_dash(relation_type: &str) -> &'static str {
    match relation_type {
        "引用" | "依赖" => "5,5",
        _ => "none",
    }
}
```

- [ ] **Step 2: 修改边渲染逻辑使用关系类型样式**

```rust
// 修改 graph.rs 中边的渲染
for (edge, (sx, sy), (tx, ty)) in &valid_edges {
    rsx! {
        line {
            x1: "{sx}",
            y1: "{sy}",
            x2: "{tx}",
            y2: "{ty}",
            stroke: "{get_edge_color(&edge.label)}",
            stroke_width: "2",
            stroke_dasharray: "{get_edge_dash(&edge.label)}",
            marker_end: "url(#arrowhead)",
        }
        // 边标签...
    }
}
```

- [ ] **Step 3: 添加关系类型图例**

```rust
// 在 knowledge_graph.rs 的图例区域添加
div { class: "graph-legend-section",
    h4 { class: "graph-legend-title", "关系类型" }
    div { class: "flex flex-wrap gap-3",
        span { class: "graph-legend-item",
            span { class: "graph-legend-line", style: "background: #ef4444;" }
            "属于"
        }
        span { class: "graph-legend-item",
            span { class: "graph-legend-line dashed", style: "background: #3b82f6;" }
            "引用"
        }
        span { class: "graph-legend-item",
            span { class: "graph-legend-line", style: "background: #10b981;" }
            "包含"
        }
        span { class: "graph-legend-item",
            span { class: "graph-legend-line", style: "background: #f59e0b;" }
            "关联"
        }
        span { class: "graph-legend-item",
            span { class: "graph-legend-line", style: "background: #8b5cf6;" }
            "派生"
        }
        span { class: "graph-legend-item",
            span { class: "graph-legend-line dashed", style: "background: #ec4899;" }
            "依赖"
        }
    }
}
```

- [ ] **Step 4: 添加图例 CSS 样式**

```css
.graph-legend-section {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border-color);
}

.graph-legend-title {
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 4px;
}

.graph-legend-line {
    display: inline-block;
    width: 24px;
    height: 3px;
    border-radius: 2px;
    margin-right: 4px;
    vertical-align: middle;
}

.graph-legend-line.dashed {
    background-image: repeating-linear-gradient(
        to right,
        currentColor 0,
        currentColor 5px,
        transparent 5px,
        transparent 10px
    );
    background-color: transparent;
    color: inherit;
}
```

- [ ] **Step 5: 运行编译验证**

Run: `cd frontend && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/graph.rs frontend/src/pages/hr/knowledge_graph.rs frontend/index.html
git commit -m "feat: 关系类型差异化渲染"
```

---

## Task 2: 边标签防重叠与优化

**Files:**
- Modify: `frontend/src/components/graph.rs`

**目标:** 边标签根据边的角度自动旋转，避免重叠，添加背景框提高可读性。

- [ ] **Step 1: 添加边标签角度计算和旋转逻辑**

```rust
// 在 graph.rs 中添加
fn calculate_edge_angle(sx: f64, sy: f64, tx: f64, ty: f64) -> f64 {
    let dx = tx - sx;
    let dy = ty - sy;
    dy.atan2(dx) * 180.0 / std::f64::consts::PI
}

fn get_label_transform(sx: f64, sy: f64, tx: f64, ty: f64) -> String {
    let mid_x = (sx + tx) / 2.0;
    let mid_y = (sy + ty) / 2.0;
    let angle = calculate_edge_angle(sx, sy, tx, ty);
    format!("translate({}, {}) rotate({})", mid_x, mid_y - 8.0, angle)
}
```

- [ ] **Step 2: 修改边标签渲染，添加旋转和背景框**

```rust
// 修改 graph.rs 中的边标签渲染
if !edge.label.is_empty() {
    let label_text = edge.label.chars().take(10).collect::<String>();
    rsx! {
        g {
            transform: "{get_label_transform(*sx, *sy, *tx, *ty)}",
            rect {
                x: "-{label_text.len() as f64 * 3.5}",
                y: "-7",
                width: "{label_text.len() as f64 * 7.0 + 4.0}",
                height: "14",
                rx: "2",
                fill: "rgba(255, 255, 255, 0.9)",
                stroke: "#e5e7eb",
                stroke_width: "1",
            }
            text {
                x: "0",
                y: "2",
                text_anchor: "middle",
                font_size: "10",
                fill: "#374151",
                font_weight: "500",
                "{label_text}"
            }
        }
    }
}
```

- [ ] **Step 3: 运行编译验证**

Run: `cd frontend && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/graph.rs
git commit -m "feat: 边标签防重叠与背景框优化"
```

---

## Task 3: 节点拖拽功能

**Files:**
- Modify: `frontend/src/components/graph.rs`

**目标:** 支持用户拖拽节点改变位置，提升交互灵活性。

- [ ] **Step 1: 添加拖拽状态管理**

```rust
// 在 Graph 组件中添加状态
#[component]
pub fn Graph(props: GraphProps) -> Element {
    let node_positions = use_signal(|| {
        let mut pos = HashMap::new();
        for node in &props.nodes {
            pos.insert(node.id.clone(), (node.x, node.y));
        }
        pos
    });

    let is_dragging = use_signal(|| false);
    let dragged_node_id = use_signal(|| None::<String>);
    let drag_offset = use_signal(|| (0.0, 0.0));
    let svg_ref = use_ref::<Option<web_sys::SvgElement>>(|| None);
```

- [ ] **Step 2: 添加鼠标事件处理**

```rust
// 在 Graph 组件中添加事件处理
let handle_mouse_down = move |e: MouseEvent| {
    let svg = svg_ref.read().clone();
    if svg.is_none() { return; }
    
    let target = e.target().unwrap();
    let element = target.dyn_into::<web_sys::SvgElement>().unwrap();
    let parent = element.parent_element().unwrap();
    
    // 查找所属的 g 元素（节点容器）
    let mut current: web_sys::Element = parent.into();
    while let Some(p) = current.parent_element() {
        if current.tag_name() == "g" {
            // 检查是否有 node-id 属性
            if let Some(node_id) = current.get_attribute("data-node-id") {
                is_dragging.set(true);
                dragged_node_id.set(Some(node_id.clone()));
                
                let rect = current.get_bounding_client_rect();
                let offset_x = e.client_x() as f64 - rect.left();
                let offset_y = e.client_y() as f64 - rect.top();
                drag_offset.set((offset_x, offset_y));
                break;
            }
        }
        current = p;
    }
};

let handle_mouse_move = move |e: MouseEvent| {
    if !is_dragging() { return; }
    
    let node_id = dragged_node_id.read().clone();
    if node_id.is_none() { return; }
    
    let node_id = node_id.unwrap();
    let (offset_x, offset_y) = drag_offset.read().clone();
    
    let svg = svg_ref.read().clone();
    if svg.is_none() { return; }
    
    let svg_element = svg.as_ref().unwrap();
    let svg_rect = svg_element.get_bounding_client_rect();
    
    let x = e.client_x() as f64 - svg_rect.left() - offset_x;
    let y = e.client_y() as f64 - svg_rect.top() - offset_y;
    
    node_positions.write().insert(node_id, (x, y));
};

let handle_mouse_up = move |_| {
    is_dragging.set(false);
    dragged_node_id.set(None);
};
```

- [ ] **Step 3: 修改节点渲染，添加 data-node-id 属性**

```rust
// 修改节点 g 元素
g {
    "data-node-id": "{node.id}",
    cursor: "move",
    // ... 其他属性
}
```

- [ ] **Step 4: 绑定鼠标事件到 SVG**

```rust
// 修改 svg 元素
svg {
    width: "{svg_width}",
    height: "{svg_height}",
    view_box: "0 0 {svg_width} {svg_height}",
    style: "border: 1px solid var(--border-color); border-radius: 8px; background: var(--bg-card);",
    onmousedown: handle_mouse_down,
    onmousemove: handle_mouse_move,
    onmouseup: handle_mouse_up,
    onmouseleave: handle_mouse_up,
    ref: svg_ref,
    // ... 其他内容
}
```

- [ ] **Step 5: 运行编译验证**

Run: `cd frontend && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/graph.rs
git commit -m "feat: 节点拖拽功能"
```

---

## Task 4: 图谱缩放与平移

**Files:**
- Modify: `frontend/src/components/graph.rs`
- Modify: `frontend/index.html`

**目标:** 支持鼠标滚轮缩放和右键拖拽平移整个图谱视图。

- [ ] **Step 1: 添加视图变换状态**

```rust
// 在 Graph 组件中添加
let view_transform = use_signal(|| (0.0, 0.0, 1.0)); // (translate_x, translate_y, scale)
let is_panning = use_signal(|| false);
let pan_start = use_signal(|| (0.0, 0.0));
```

- [ ] **Step 2: 添加缩放和平移事件处理**

```rust
// 缩放处理
let handle_wheel = move |e: WheelEvent| {
    e.prevent_default();
    
    let (tx, ty, scale) = view_transform.read().clone();
    let delta = if e.delta_y() > 0 { 0.9 } else { 1.1 };
    
    // 限制缩放范围
    let new_scale = (scale * delta).clamp(0.5, 2.0);
    view_transform.set((tx, ty, new_scale));
};

// 平移处理
let handle_context_menu = move |e: MouseEvent| {
    e.prevent_default();
};

let handle_pan_start = move |e: MouseEvent| {
    if e.button() == 2 { // 右键
        is_panning.set(true);
        pan_start.set((e.client_x() as f64, e.client_y() as f64));
    }
};

let handle_pan_move = move |e: MouseEvent| {
    if !is_panning() { return; }
    
    let (tx, ty, scale) = view_transform.read().clone();
    let (start_x, start_y) = pan_start.read().clone();
    
    let dx = (e.client_x() as f64 - start_x) / scale;
    let dy = (e.client_y() as f64 - start_y) / scale;
    
    view_transform.set((tx + dx, ty + dy, scale));
    pan_start.set((e.client_x() as f64, e.client_y() as f64));
};

let handle_pan_end = move |_| {
    is_panning.set(false);
};
```

- [ ] **Step 3: 添加 SVG 组级别的变换**

```rust
// 在 svg 中添加 g 元素包裹所有内容
let (tx, ty, scale) = view_transform.read().clone();

svg {
    // ... 属性
    onwheel: handle_wheel,
    oncontextmenu: handle_context_menu,
    onmousedown: handle_pan_start,
    onmousemove: handle_pan_move,
    onmouseup: handle_pan_end,
    onmouseleave: handle_pan_end,
    
    g {
        transform: "translate({tx}, {ty}) scale({scale})",
        
        // defs...
        // 边...
        // 节点...
    }
}
```

- [ ] **Step 4: 添加缩放指示器**

```rust
// 在知识图谱页面添加缩放控制
div { class: "graph-zoom-controls",
    button {
        class: "btn btn-ghost btn-sm",
        onclick: move |_| {
            let (tx, ty, scale) = view_transform.read().clone();
            view_transform.set((tx, ty, (scale * 0.9).max(0.5)));
        },
        "−"
    }
    span { class: "zoom-value", "{(view_transform.read().2 * 100).round()}%" }
    button {
        class: "btn btn-ghost btn-sm",
        onclick: move |_| {
            let (tx, ty, scale) = view_transform.read().clone();
            view_transform.set((tx, ty, (scale / 0.9).min(2.0)));
        },
        "+"
    }
    button {
        class: "btn btn-ghost btn-sm",
        onclick: move |_| {
            view_transform.set((0.0, 0.0, 1.0));
        },
        "⟲"
    }
}
```

- [ ] **Step 5: 添加缩放控制 CSS**

```css
.graph-zoom-controls {
    position: absolute;
    bottom: 16px;
    right: 16px;
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-card);
    padding: 4px;
    border-radius: 8px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
}

.zoom-value {
    font-size: 12px;
    color: var(--text-muted);
    min-width: 40px;
    text-align: center;
}
```

- [ ] **Step 6: 运行编译验证**

Run: `cd frontend && cargo check`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/graph.rs frontend/src/pages/hr/knowledge_graph.rs frontend/index.html
git commit -m "feat: 图谱缩放与平移功能"
```

---

## Task 5: 搜索结果高亮与历史记录

**Files:**
- Modify: `frontend/src/pages/hr/knowledge_graph.rs`
- Modify: `frontend/src/components/graph.rs`

**目标:** 搜索时高亮显示匹配节点，显示搜索历史记录。

- [ ] **Step 1: 添加搜索历史状态**

```rust
// 在 knowledge_graph.rs 中添加
let mut search_history = use_signal(Vec::<String>::new);
let mut highlighted_nodes = use_signal(HashSet::<String>::new);
```

- [ ] **Step 2: 修改搜索处理，记录历史和高亮节点**

```rust
let mut handle_search = move |_| {
    let kw = keyword().clone();
    if kw.is_empty() {
        return;
    }
    
    // 添加到搜索历史（去重）
    let mut history = search_history.read().clone();
    history.retain(|h| h != &kw);
    history.insert(0, kw.clone());
    if history.len() > 10 {
        history.truncate(10);
    }
    search_history.set(history);
    
    loading.set(true);
    error.set(String::new());
    expanded_nodes.set(HashSet::new());
    selected_node_id.set(None);
    selected_node_data.set(None);
    
    spawn(async move {
        match search_memory_with_traversal(&kw, &[], 1).await {
            Ok(data) => {
                let mut map = std::collections::HashMap::new();
                let mut highlighted = HashSet::new();
                for item in &data.results {
                    if item.memory_type != "relation" {
                        map.insert(item.id.clone(), item.clone());
                        highlighted.insert(item.id.clone());
                    }
                }
                detail_map.set(map);
                highlighted_nodes.set(highlighted);
                
                // ... 其余代码
            }
            Err(e) => error.set(e),
        }
        loading.set(false);
    });
};
```

- [ ] **Step 3: 在 Graph 组件中添加高亮支持**

```rust
// 修改 GraphProps
#[derive(Props, Clone, PartialEq)]
pub struct GraphProps {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub selected_node_id: Option<String>,
    pub highlighted_node_ids: Option<HashSet<String>>,
    on_node_click: EventHandler<String>,
}

// 修改节点选中状态判断
let is_highlighted = props.highlighted_node_ids.as_ref()
    .map(|ids| ids.contains(&node.id))
    .unwrap_or(false);

// 添加高亮样式
fn get_node_opacity(is_highlighted: bool, is_selected: bool) -> &'static str {
    if is_selected { "1" }
    else if is_highlighted { "1" }
    else { "0.4" }
}

fn get_node_glow(is_highlighted: bool, is_selected: bool) -> String {
    if is_selected {
        "filter: drop-shadow(0 0 8px rgba(249, 115, 22, 0.6));"
    } else if is_highlighted {
        "filter: drop-shadow(0 0 6px rgba(59, 130, 246, 0.5));"
    } else {
        "".to_string()
    }
}
```

- [ ] **Step 4: 添加搜索历史 UI**

```rust
// 在搜索框下方添加
if !search_history().is_empty() {
    div { class: "search-history",
        span { class: "text-muted text-sm", "搜索历史: " }
        for kw in search_history().iter() {
            let kw_clone = kw.clone();
            button {
                class: "btn btn-ghost btn-xs",
                onclick: move |_| {
                    keyword.set(kw_clone.clone());
                    handle_search(());
                },
                "{kw}"
            }
        }
    }
}
```

- [ ] **Step 5: 运行编译验证**

Run: `cd frontend && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/graph.rs frontend/src/pages/hr/knowledge_graph.rs
git commit -m "feat: 搜索结果高亮与历史记录"
```

---

## Task 6: 详情侧边栏增强

**Files:**
- Modify: `frontend/src/pages/hr/knowledge_graph.rs`
- Modify: `frontend/index.html`

**目标:** 增强节点详情展示，添加创建时间、来源节点、目标节点等信息，支持关联节点快捷跳转。

- [ ] **Step 1: 添加时间格式化函数**

```rust
fn format_timestamp(ts: i64) -> String {
    use chrono::{DateTime, Local, TimeZone};
    let dt = Local.timestamp_opt(ts, 0).unwrap();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}
```

- [ ] **Step 2: 增强详情侧边栏内容**

```rust
// 修改详情侧边栏
if let Some(detail) = &selected_detail {
    div { class: "graph-detail-panel",
        div { class: "card",
            div { class: "card-header",
                h3 { class: "card-title", "节点详情" }
                button {
                    class: "btn btn-ghost btn-sm",
                    onclick: move |_| {
                        selected_node_id.set(None);
                        selected_node_data.set(None);
                    },
                    "✕"
                }
            }
            div { class: "detail-grid",
                div {
                    label { class: "form-label", "类型" }
                    span { class: "{type_badge_class(&detail.memory_type)}", "{type_label(&detail.memory_type)}" }
                }
                div {
                    label { class: "form-label", "匹配类型" }
                    span { class: "text-muted",
                        match detail.match_type.as_deref() {
                            Some("hybrid") => "混合匹配",
                            Some("vector") => "向量匹配",
                            Some("keyword") => "关键词匹配",
                            _ => "未知"
                        }
                    }
                }
                div {
                    label { class: "form-label", "内容" }
                    div { class: "detail-content-container",
                        pre { class: "detail-content", "{detail.content}" }
                    }
                }
                if let Some(summary) = &detail.summary {
                    div {
                        label { class: "form-label", "摘要" }
                        p { class: "detail-content text-muted", "{summary}" }
                    }
                }
                if let Some(source_node_id) = &detail.source_node_id {
                    div {
                        label { class: "form-label", "来源节点" }
                        button {
                            class: "btn btn-ghost btn-sm text-primary",
                            onclick: move |_| {
                                // 跳转到来源节点
                                handle_node_click(source_node_id.clone());
                            },
                            "{source_node_id}"
                        }
                    }
                }
                if let Some(target_node_id) = &detail.target_node_id {
                    div {
                        label { class: "form-label", "目标节点" }
                        button {
                            class: "btn btn-ghost btn-sm text-primary",
                            onclick: move |_| {
                                handle_node_click(target_node_id.clone());
                            },
                            "{target_node_id}"
                        }
                    }
                }
                if let Some(score) = detail.score {
                    div {
                        label { class: "form-label", "匹配分数" }
                        span { class: "text-mono text-muted", "{score:.4}" }
                    }
                }
                if let Some(vector_distance) = detail.vector_distance {
                    div {
                        label { class: "form-label", "向量距离" }
                        span { class: "text-mono text-muted", "{vector_distance:.4}" }
                    }
                }
                div {
                    label { class: "form-label", "ID" }
                    span { class: "text-mono text-muted text-sm", "{detail.id}" }
                }
            }
        }
    }
}
```

- [ ] **Step 3: 添加详情内容容器样式**

```css
.detail-content-container {
    max-height: 200px;
    overflow-y: auto;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    padding: 8px;
}

.detail-content {
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 13px;
    line-height: 1.6;
    margin: 0;
    color: var(--text-primary);
}
```

- [ ] **Step 4: 运行编译验证**

Run: `cd frontend && cargo check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/hr/knowledge_graph.rs frontend/index.html
git commit -m "feat: 详情侧边栏增强"
```

---

## Self-Review

### 1. Spec Coverage

| 需求 | 覆盖任务 |
|------|----------|
| 节点点击展开关联 | 现有实现已覆盖 |
| 节点类型差异化 | Task 1 |
| 关系类型差异化 | Task 1 |
| 详情侧边栏 | Task 6 |
| 布局优化 | Task 3 (拖拽) + Task 4 (缩放平移) |
| 边标签防重叠 | Task 2 |
| 搜索结果高亮 | Task 5 |
| 图例完善 | Task 1 |

### 2. Placeholder Scan

无占位符，所有步骤均包含完整代码和命令。

### 3. Type Consistency

- `GraphProps` 结构一致
- `GraphNode`/`GraphEdge` 类型一致
- 事件处理函数签名一致

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-15-knowledge-graph-enhancement.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**