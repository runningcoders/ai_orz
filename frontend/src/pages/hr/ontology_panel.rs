//! 本体词表管理页（/hr/ontology-lexicon）
//!
//! 5 个子标签 + 四个辅助 Modal：
//! 1. **总览**：词表注入视图（与神经技能注入共用同一契约的只读快照）+ 漂移看板
//!    （词表覆盖率 / 漂移关系数 / 漂移节点占比 + Top 漂移词，可下钻明细）
//! 2. **实体类词表**：分页列表 + 状态/关键词筛选 + 新增 / 编辑 / 退役（软删除）
//! 3. **关系类型词表**：同上，附域/值域约束、边权重基数、反向关系
//! 4. **属性定义**：分页列表 + 状态/关键词筛选 + 新增 / 编辑 / 退役；表单承载
//!    值类型 / 基数 / 允许载体 / 取值约束（这些会随词表注入提示词，是模型写值依据）
//! 5. **同义映射**：分页列表 + 目标种类筛选 + 新增 / 删除（物理删除）
//!
//! 各标签共享同一组 signals，初始加载一次性拉取全部数据，切换标签仅切换渲染不重复请求。
//!
//! 同步 Modal：seed 预置词表 preview → sync（仅补缺策略，不覆盖本地修改）。
//! 下钻 Modal：漂移词的关系（边）明细 + 节点明细。

use common::api::{
    CreateOntologyClassRelationRequest, CreateOntologyClassRequest, CreateOntologyPropertyRequest,
    CreateOntologyRelationTypeRequest, CreateOntologySynonymRequest, DriftClassDetail,
    DriftRelationDetail, GetDriftDashboardRequest, GetDriftDashboardResponse,
    GetOntologySkeletonResponse, ListDriftClassDetailsRequest, ListDriftRelationDetailsRequest,
    ListOntologyClassesRequest, ListOntologyLexiconResponse, ListOntologyPropertiesRequest,
    ListOntologyRelationTypesRequest, ListOntologySynonymsRequest, OntologyClassItem,
    OntologyClassRelationItem, OntologyPropertyItem, OntologyRelationTypeItem, OntologySynonymItem,
    PaginationParams, PresetOntologySyncStrategy, PreviewPresetOntologyResponse,
    SyncPresetOntologyRequest, SyncPresetOntologyResponse, UpdateOntologyClassRelationRequest,
    UpdateOntologyClassRequest, UpdateOntologyPropertyRequest, UpdateOntologyRelationTypeRequest,
};
use common::enums::OntologyStatus;
use common::enums::ontology::{Cardinality, PropertyDataType, PropertyTargetType};
use common::ontology::{TermKind, WILDCARD_CLASS};
use common::ontology_property::ValueRange;
use dioxus::prelude::*;

use crate::api::hr::{
    create_ontology_class, create_ontology_class_relation, create_ontology_property,
    create_ontology_relation_type, create_ontology_synonym, delete_ontology_synonym,
    get_ontology_drift_dashboard, get_ontology_skeleton, list_ontology_classes,
    list_ontology_drift_class_details, list_ontology_drift_relation_details, list_ontology_lexicon,
    list_ontology_properties, list_ontology_relation_types, list_ontology_synonyms,
    retire_ontology_class, retire_ontology_class_relation, retire_ontology_property,
    retire_ontology_relation_type, update_ontology_class, update_ontology_class_relation,
    update_ontology_property, update_ontology_relation_type,
};
use crate::api::seed::{preview_preset_ontology, sync_preset_ontology};
use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene, card_size};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudPanel, HudSection, PageHeader, StatGrid, StatReadout};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::dagre_layout::{DagreEdge, DagreNode, dagre_layout};
use crate::utils::number::{format_compact_count, format_relevance};
use crate::utils::time::format_datetime;

/// 词表列表分页大小
const PAGE_SIZE: usize = 20;
/// 漂移下钻单侧明细条数上限
const DRILL_DETAIL_LIMIT: usize = 20;
/// 看板 Top 漂移词数量
const DASHBOARD_TOP_N: usize = 10;

/// 状态筛选下拉值 → OntologyStatus
fn parse_status_filter(v: &str) -> Option<OntologyStatus> {
    match v {
        "Active" => Some(OntologyStatus::Active),
        "Retired" => Some(OntologyStatus::Retired),
        _ => None,
    }
}

/// 种类筛选下拉值 → TermKind
fn parse_kind_filter(v: &str) -> Option<TermKind> {
    match v {
        "class" => Some(TermKind::Class),
        "relation" => Some(TermKind::Relation),
        _ => None,
    }
}

/// 同义映射表单的种类下拉值 → TermKind（默认关系）
fn parse_term_kind(v: &str) -> TermKind {
    if v == "class" {
        TermKind::Class
    } else {
        TermKind::Relation
    }
}

/// trim 后非空才保留
fn non_empty(v: &str) -> Option<String> {
    let t = v.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// 逗号分隔清单 → 去空白去空项
fn split_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ==================== 属性定义表单：字符串 ↔ 强类型枚举 ====================
//
// 表单用字符串 signal（Dioxus 受控输入的通用形态），提交前转强类型枚举。
// 非法值走 `parse().unwrap_or_default()` 而非报错：下拉框已限定取值，
// 这里只防手改 DOM 与未来新增变体时的旧快照，与后端软门禁同口径。

/// 值类型下拉值 → PropertyDataType
fn parse_data_type(v: &str) -> PropertyDataType {
    v.parse().unwrap_or_default()
}

/// 基数下拉值 → Cardinality
fn parse_cardinality(v: &str) -> Cardinality {
    v.parse().unwrap_or_default()
}

/// 载体清单（逗号分隔 node / relation）→ PropertyTargetType 清单；拼错项丢弃
fn parse_applies_to(v: &str) -> Vec<PropertyTargetType> {
    split_list(v)
        .into_iter()
        .filter_map(|s| s.parse::<PropertyTargetType>().ok())
        .collect()
}

/// PropertyTargetType 清单 → 表单回填文本
fn applies_to_text(v: &[PropertyTargetType]) -> String {
    v.iter()
        .map(|t| t.as_str().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// 取值约束三输入（min / max / 枚举白名单）→ ValueRange
///
/// 数字解析失败即视为未填（不做静默回退）：写侧据此校验，猜出来的边界比
/// 没有边界更危险；空则退化为「只校验数据类型」。
fn parse_value_range_form(min: &str, max: &str, enum_text: &str) -> ValueRange {
    ValueRange {
        min: min.trim().parse::<f64>().ok(),
        max: max.trim().parse::<f64>().ok(),
        enum_values: split_list(enum_text),
    }
}

fn status_badge(status: OntologyStatus) -> Element {
    match status {
        OntologyStatus::Active => {
            rsx! { span { class: "badge hud-badge badge-success", "启用中" } }
        }
        OntologyStatus::Retired => {
            rsx! { span { class: "badge hud-badge badge-warning", "已退役" } }
        }
    }
}

fn kind_badge(kind: TermKind) -> Element {
    match kind {
        TermKind::Class => rsx! { span { class: "badge orz-tag badge-sm", "实体" } },
        TermKind::Relation => rsx! { span { class: "badge orz-tag badge-sm", "关系" } },
    }
}

// ===== 骨架图：dagre 分层布局（复用 CanvasScene 渲染引擎） =====
//
// 布局交给 dagre（Sugiyama）：继承边 + 类-关系-类三元组全部参与有向布局，
// 节点尺寸取自真实卡片宽高，同层由坐标层面保证不重叠，跨层边按 dummy node
// 路由绕开卡片。坐标一次算好并关闭力导向，选中某类只筛选**渲染**的边，
// 节点布局不随选中跳动。
//
// 世界尺寸怎么定：dagre 适配层会把坐标等比缩放塞进给定画布，世界设小了
// 密集图会被整体压扁（卡片坐标间距缩到小于卡片宽 → 互压）。所以先丢进一个
// 超大画布探测自然包围盒，再按「自然尺寸 + 余量」定世界大小。

/// 世界最小尺寸（小图按此铺开，dagre 会等比放大铺满）
const SKELETON_MIN_WORLD_W: f64 = 1200.0;
const SKELETON_MIN_WORLD_H: f64 = 700.0;
/// 探测画布尺寸（只用于读取自然包围盒，不参与最终渲染）
const SKELETON_PROBE_WORLD: f64 = 1_000_000.0;
/// dagre 适配层坐标映射的缩放上限（见 utils::dagre_layout，探测结果需按此还原）
const SKELETON_DAGRE_MAX_SCALE: f64 = 2.5;
/// 世界尺寸余量：探测包围盒只覆盖卡片中心，还原后需补回卡片自身宽高
const SKELETON_WORLD_SLACK: f64 = 160.0;
/// 同一对类的多条平行关系（含双向）错开间距
const SKELETON_PARALLEL_GAP: f64 = 24.0;
/// 节点卡片配色（按词表序稳定轮转，仅用于区分卡片，不表达语义）
const SKELETON_PALETTE: [&str; 6] = [
    "#22d3ee", "#818cf8", "#34d399", "#fbbf24", "#f472b6", "#60a5fa",
];

/// 无向对的字典序 key：同一对类的正反向关系共用一条 dagre 边与路由
fn skeleton_pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// 布局结果包围盒（含卡片半宽高与边 waypoints），用于反推自然世界尺寸
fn skeleton_layout_extent(
    positions: &std::collections::HashMap<String, (f64, f64)>,
    routes: &std::collections::HashMap<(String, String), Vec<(f64, f64)>>,
    nodes: &[CanvasNode],
) -> (f64, f64) {
    let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
    let (mut max_x, mut max_y) = (f64::MIN, f64::MIN);
    for n in nodes {
        let Some(&(x, y)) = positions.get(&n.id) else {
            continue;
        };
        let (w, h) = card_size(n);
        min_x = min_x.min(x - w / 2.0);
        max_x = max_x.max(x + w / 2.0);
        min_y = min_y.min(y - h / 2.0);
        max_y = max_y.max(y + h / 2.0);
    }
    for pts in routes.values() {
        for (x, y) in pts {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
    }
    if min_x > max_x || min_y > max_y {
        return (0.0, 0.0);
    }
    (max_x - min_x, max_y - min_y)
}

/// 组装 CanvasScene 节点：卡片承载「名称 + term_key + 描述 + 父类 / 必填 / 关联数」
fn skeleton_canvas_nodes(
    classes: &[OntologyClassItem],
    class_relations: &[OntologyClassRelationItem],
) -> Vec<CanvasNode> {
    let mut relation_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for rel in class_relations {
        *relation_counts
            .entry(rel.subject_class_key.as_str())
            .or_default() += 1;
        *relation_counts
            .entry(rel.object_class_key.as_str())
            .or_default() += 1;
    }
    classes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut tags = vec![match c.parent_term_key.as_deref() {
                Some(parent) => format!("父 {parent}"),
                None => "根类".to_string(),
            }];
            if !c.required_fields.is_empty() {
                tags.push(format!("必填 {}", c.required_fields.len()));
            }
            let relation_count = relation_counts
                .get(c.term_key.as_str())
                .copied()
                .unwrap_or(0);
            if relation_count > 0 {
                tags.push(format!("关联 {relation_count}"));
            }
            let mut node = CanvasNode {
                id: c.term_key.clone(),
                x: 0.0,
                y: 0.0,
                radius: 0.0,
                label: if c.display_name.is_empty() {
                    c.term_key.clone()
                } else {
                    c.display_name.clone()
                },
                color: SKELETON_PALETTE[i % SKELETON_PALETTE.len()].to_string(),
                node_type: Some("ontology_class".to_string()),
                layer: None,
                summary: Some(c.term_key.clone()),
                description: c.description.clone(),
                tags,
                highlighted: false,
            };
            // 卡片节点的等效半径 = 外接圆半径（箭头回退与力导向碰撞都以它为准）
            let (w, h) = card_size(&node);
            node.radius = w.max(h) / 2.0;
            node
        })
        .collect()
}

/// 基数文案（词表口径 single / many → 单值 / 多值）
fn skeleton_cardinality_label(cardinality: &str) -> &'static str {
    match cardinality {
        "single" => "单值",
        "many" => "多值",
        _ => "未设",
    }
}

/// 组装 CanvasScene 连线（全量）：继承边 + 所有两侧都是具体类的三元组
///
/// 全量参与 dagre 布局，选中某类时只在渲染层筛选边——否则布局会随选中重排。
fn skeleton_canvas_edges(
    classes: &[OntologyClassItem],
    class_relations: &[OntologyClassRelationItem],
    relation_types: &[OntologyRelationTypeItem],
) -> Vec<CanvasEdge> {
    let known: std::collections::HashSet<&str> =
        classes.iter().map(|c| c.term_key.as_str()).collect();
    let mut edges = Vec::new();

    // 继承边：父 → 子（无父的根类没有入边）
    for c in classes {
        let Some(parent) = c.parent_term_key.as_deref() else {
            continue;
        };
        if !known.contains(parent) {
            continue;
        }
        edges.push(CanvasEdge {
            from_id: parent.to_string(),
            to_id: c.term_key.clone(),
            tag: Some("继承".to_string()),
            description: Some("子类继承父类：骨架约束沿祖先链向上解析".to_string()),
            weight: Some(0.45),
            directional: true,
            ..Default::default()
        });
    }

    // 关系边：仅两侧都是具体类时连边（通配符侧无法定位）
    for rel in class_relations {
        if !known.contains(rel.subject_class_key.as_str())
            || !known.contains(rel.object_class_key.as_str())
        {
            continue;
        }
        let display = relation_types
            .iter()
            .find(|r| r.term_key == rel.relation_key)
            .map(|r| r.display_name.clone())
            .unwrap_or_else(|| rel.relation_key.clone());
        edges.push(CanvasEdge {
            from_id: rel.subject_class_key.clone(),
            to_id: rel.object_class_key.clone(),
            tag: Some(rel.relation_key.clone()),
            description: Some(format!(
                "{} · {} · {} · {} → {}",
                display,
                skeleton_cardinality_label(&rel.cardinality),
                if rel.required { "必填" } else { "可选" },
                rel.subject_class_key,
                rel.object_class_key
            )),
            weight: None,
            directional: true,
            bulge: None,
            points: None,
        });
    }

    edges
}

/// 折线按**局部法向**整体平移：每个顶点的法向取其前后两段方向的平均，
/// 平移后的折线与原路由通道保持平行。
///
/// 不能用「两端点连线的全局垂线」平推：折线一旦有拐（dagre 绕卡路由必有），
/// 全局垂线在拐角处会把整条折线推离自己的通道，看起来线头乱飞、甚至压到别的
/// 卡片上；按局部法向平移则任何形状都只是「平行地挪开一档」。
fn skeleton_offset_route(pts: &[(f64, f64)], off: f64) -> Vec<(f64, f64)> {
    if off.abs() <= f64::EPSILON || pts.len() < 2 {
        return pts.to_vec();
    }
    let n = pts.len();
    pts.iter()
        .enumerate()
        .map(|(i, p)| {
            let prev = if i == 0 { *p } else { pts[i - 1] };
            let next = if i + 1 >= n { *p } else { pts[i + 1] };
            let (dx, dy) = (next.0 - prev.0, next.1 - prev.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < f64::EPSILON {
                return *p;
            }
            (p.0 - dy / len * off, p.1 + dx / len * off)
        })
        .collect()
}

/// 把 dagre 路由回写到边上；同一对类的多条关系按**局部法向**错开
///
/// dagre 对同一对节点只给一条路由（正反向共用），平行关系若共用同一条折线会
/// 完全重叠、标签糊成一团。这里按无向对分组，逐条平移 `SKELETON_PARALLEL_GAP`。
/// 自环 / 无路由的边退回 `bulge` 弧线（有错开量才画得出）。
fn skeleton_apply_routes(
    edges: &mut [CanvasEdge],
    routes: &std::collections::HashMap<(String, String), Vec<(f64, f64)>>,
) {
    let mut groups: std::collections::HashMap<(String, String), Vec<usize>> =
        std::collections::HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        groups
            .entry(skeleton_pair_key(&e.from_id, &e.to_id))
            .or_default()
            .push(i);
    }
    for (key, idxs) in groups {
        let n = idxs.len();
        for (k, &i) in idxs.iter().enumerate() {
            let off = if n > 1 {
                (k as f64 - (n.saturating_sub(1)) as f64 / 2.0) * SKELETON_PARALLEL_GAP
            } else {
                0.0
            };
            let Some(route) = routes.get(&key) else {
                edges[i].bulge = if off.abs() > f64::EPSILON {
                    Some(off)
                } else {
                    None
                };
                continue;
            };
            // 先按 key 方向错开再反转：同组所有边共用一套法向，正反向才会分居两侧
            let mut pts = skeleton_offset_route(route, off);
            if edges[i].from_id != key.0 {
                pts.reverse();
            }
            edges[i].points = Some(pts);
            edges[i].bulge = None;
        }
    }
}

/// 渲染骨架图：dagre 分层坐标 + CanvasScene（关闭力导向）
///
/// 卡片承载类名 / term_key / 描述与「父类、必填、关联数」标签；连线按关系词着色，
/// 悬停出关系明细。选中某个类时只保留与它相关的三元组边（布局坐标不重算）。
fn render_skeleton_graph(
    classes: &[OntologyClassItem],
    class_relations: &[OntologyClassRelationItem],
    relation_types: &[OntologyRelationTypeItem],
    selected: Option<String>,
    on_select: EventHandler<String>,
) -> Element {
    let mut nodes = skeleton_canvas_nodes(classes, class_relations);
    let mut edges = skeleton_canvas_edges(classes, class_relations, relation_types);

    // dagre 输入：卡片真实宽高 + 每个无向对只喂一条边（平行边在渲染层错开）
    let dagre_nodes: Vec<DagreNode> = nodes
        .iter()
        .map(|n| {
            let (width, height) = card_size(n);
            DagreNode {
                id: n.id.clone(),
                width,
                height,
            }
        })
        .collect();
    let mut seen_pairs: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut dagre_edges = Vec::new();
    for e in &edges {
        if e.from_id == e.to_id {
            continue; // dagre 不布局自环
        }
        let key = skeleton_pair_key(&e.from_id, &e.to_id);
        if seen_pairs.insert(key.clone()) {
            dagre_edges.push(DagreEdge {
                source: key.0,
                target: key.1,
            });
        }
    }

    // 先探测自然包围盒（大画布会把坐标等比放大到上限），再按自然尺寸定世界
    let probe = dagre_layout(
        &dagre_nodes,
        &dagre_edges,
        SKELETON_PROBE_WORLD,
        SKELETON_PROBE_WORLD,
        false,
    );
    let (extent_w, extent_h) = skeleton_layout_extent(&probe.positions, &probe.routes, &nodes);
    let world_w =
        (extent_w / SKELETON_DAGRE_MAX_SCALE + SKELETON_WORLD_SLACK).max(SKELETON_MIN_WORLD_W);
    let world_h =
        (extent_h / SKELETON_DAGRE_MAX_SCALE + SKELETON_WORLD_SLACK).max(SKELETON_MIN_WORLD_H);

    let layout = dagre_layout(&dagre_nodes, &dagre_edges, world_w, world_h, false);
    for n in &mut nodes {
        if let Some(&(x, y)) = layout.positions.get(&n.id) {
            n.x = x;
            n.y = y;
        }
    }
    skeleton_apply_routes(&mut edges, &layout.routes);
    // 选中只在渲染层筛边：继承边常驻，三元组只留与选中类相关的
    let selected_for_render = selected.as_deref();
    let render_edges: Vec<CanvasEdge> = edges
        .into_iter()
        .filter(|e| match selected_for_render {
            Some(sel) => e.tag.as_deref() == Some("继承") || e.from_id == sel || e.to_id == sel,
            None => true,
        })
        .collect();

    rsx! {
        div { class: "relative w-full h-[560px]",
            CanvasScene {
                width: world_w,
                height: world_h,
                nodes,
                edges: render_edges,
                transparent: true,
                enable_force_layout: false,
                selected_node_id: selected,
                on_node_click: move |key: String| on_select.call(key),
            }
        }
    }
}

/// 页面子标签
#[derive(Debug, Clone, Copy, PartialEq)]
enum OntologyTab {
    /// 总览：词表注入视图 + 漂移看板
    Overview,
    /// 本体骨架：实体类继承链 + 类-关系-类三元组
    Skeleton,
    /// 实体类词表
    Classes,
    /// 关系类型词表
    Relations,
    /// 属性定义词表
    Properties,
    /// 同义映射
    Synonyms,
}

impl OntologyTab {
    fn label(self) -> &'static str {
        match self {
            OntologyTab::Overview => "总览",
            OntologyTab::Skeleton => "本体骨架",
            OntologyTab::Classes => "实体类词表",
            OntologyTab::Relations => "关系类型词表",
            OntologyTab::Properties => "属性定义",
            OntologyTab::Synonyms => "同义映射",
        }
    }
}

#[component]
pub fn HrOntologyLexicon() -> Element {
    let toast = use_toast();

    // ===== 子标签 =====
    let mut active_tab = use_signal(|| OntologyTab::Overview);

    // ===== 实体类列表 =====
    let mut classes = use_signal(Vec::<OntologyClassItem>::new);
    let mut classes_total = use_signal(|| 0usize);
    let mut classes_loading = use_signal(|| false);
    let mut class_seq = use_signal(|| 0u64);
    let mut class_status_filter = use_signal(String::new);
    let mut class_keyword = use_signal(String::new);
    let mut class_offset = use_signal(|| 0usize);

    // ===== 关系类型列表 =====
    let mut relations = use_signal(Vec::<OntologyRelationTypeItem>::new);
    let mut relations_total = use_signal(|| 0usize);
    let mut relations_loading = use_signal(|| false);
    let mut relation_seq = use_signal(|| 0u64);
    let mut relation_status_filter = use_signal(String::new);
    let mut relation_keyword = use_signal(String::new);
    let mut relation_offset = use_signal(|| 0usize);

    // ===== 属性定义列表 =====
    let mut properties = use_signal(Vec::<OntologyPropertyItem>::new);
    let mut properties_total = use_signal(|| 0usize);
    let mut properties_loading = use_signal(|| false);
    let mut property_seq = use_signal(|| 0u64);
    let mut property_status_filter = use_signal(String::new);
    let mut property_keyword = use_signal(String::new);
    let mut property_offset = use_signal(|| 0usize);

    // ===== 同义映射列表 =====
    let mut synonyms = use_signal(Vec::<OntologySynonymItem>::new);
    let mut synonyms_total = use_signal(|| 0usize);
    let mut synonyms_loading = use_signal(|| false);
    let mut synonym_seq = use_signal(|| 0u64);
    let mut synonym_kind_filter = use_signal(String::new);
    let mut synonym_offset = use_signal(|| 0usize);

    // ===== 漂移看板 + 下钻 =====
    let mut dashboard = use_signal(|| None::<GetDriftDashboardResponse>);
    let mut dashboard_loading = use_signal(|| false);
    let mut drill_term = use_signal(|| None::<String>);
    let mut drill_relations = use_signal(Vec::<DriftRelationDetail>::new);
    let mut drill_classes = use_signal(Vec::<DriftClassDetail>::new);
    let mut drill_loading = use_signal(|| false);
    let mut drill_seq = use_signal(|| 0u64);

    // ===== 本体骨架（继承链 + 三元组） =====
    let mut skeleton = use_signal(|| None::<GetOntologySkeletonResponse>);
    let mut skeleton_loading = use_signal(|| false);
    let mut skeleton_seq = use_signal(|| 0u64);
    let mut skeleton_selected_class = use_signal(|| None::<String>);

    // ===== 实体类表单 Modal =====
    let mut show_class_modal = use_signal(|| false);
    let mut class_edit_id = use_signal(|| None::<String>);
    let mut class_form_key = use_signal(String::new);
    let mut class_form_name = use_signal(String::new);
    let mut class_form_desc = use_signal(String::new);
    let mut class_form_fields = use_signal(String::new);
    let mut class_form_parent = use_signal(String::new);
    let mut class_saving = use_signal(|| false);

    // ===== 关系类型表单 Modal =====
    let mut show_relation_modal = use_signal(|| false);
    let mut relation_edit_id = use_signal(|| None::<String>);
    let mut relation_form_key = use_signal(String::new);
    let mut relation_form_name = use_signal(String::new);
    let mut relation_form_desc = use_signal(String::new);
    let mut relation_form_weight = use_signal(String::new);
    let mut relation_form_inverse = use_signal(String::new);
    let mut relation_saving = use_signal(|| false);

    // ===== 骨架三元组表单 Modal =====
    let mut show_triple_modal = use_signal(|| false);
    let mut triple_edit_id = use_signal(|| None::<String>);
    let mut triple_form_subject = use_signal(String::new);
    let mut triple_form_relation = use_signal(String::new);
    let mut triple_form_object = use_signal(String::new);
    let mut triple_form_cardinality = use_signal(|| "many".to_string());
    let mut triple_form_required = use_signal(|| false);
    let mut triple_saving = use_signal(|| false);
    let mut triple_retire_id = use_signal(|| None::<String>);

    // ===== 属性定义表单 Modal =====
    let mut show_property_modal = use_signal(|| false);
    let mut property_edit_id = use_signal(|| None::<String>);
    let mut property_form_key = use_signal(String::new);
    let mut property_form_name = use_signal(String::new);
    let mut property_form_desc = use_signal(String::new);
    let mut property_form_data_type = use_signal(|| "string".to_string());
    let mut property_form_cardinality = use_signal(|| "single".to_string());
    let mut property_form_applies_to = use_signal(String::new);
    let mut property_form_domain = use_signal(String::new);
    let mut property_form_min = use_signal(String::new);
    let mut property_form_max = use_signal(String::new);
    let mut property_form_enum = use_signal(String::new);
    let mut property_form_unit = use_signal(String::new);
    let mut property_form_required = use_signal(|| false);
    let mut property_saving = use_signal(|| false);

    // ===== 同义映射表单 Modal =====
    let mut show_synonym_modal = use_signal(|| false);
    let mut synonym_form_raw = use_signal(String::new);
    let mut synonym_form_kind = use_signal(|| "relation".to_string());
    let mut synonym_form_target_key = use_signal(String::new);
    let mut synonym_saving = use_signal(|| false);

    // ===== 确认框目标 =====
    let mut class_retire_id = use_signal(|| None::<String>);
    let mut relation_retire_id = use_signal(|| None::<String>);
    let mut property_retire_id = use_signal(|| None::<String>);
    let mut synonym_delete_id = use_signal(|| None::<String>);

    // ===== 预置词表同步 Modal =====
    let mut show_sync_modal = use_signal(|| false);
    let mut sync_preview = use_signal(|| None::<PreviewPresetOntologyResponse>);
    let mut sync_preview_loading = use_signal(|| false);
    let mut syncing = use_signal(|| false);
    let mut sync_result = use_signal(|| None::<SyncPresetOntologyResponse>);
    let mut sync_strategy = use_signal(PresetOntologySyncStrategy::default);
    let mut sync_confirm_pending = use_signal(|| false);

    // ===== 词表注入视图 =====
    let mut lexicon_summary = use_signal(|| None::<ListOntologyLexiconResponse>);
    let mut lexicon_loading = use_signal(|| false);

    // ===== 数据加载闭包 =====

    let request_classes = move |offset: usize| {
        list_ontology_classes(ListOntologyClassesRequest {
            status: parse_status_filter(&class_status_filter()),
            keyword: non_empty(&class_keyword()),
            pagination: PaginationParams {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            },
        })
    };

    let mut fetch_classes = move |reset_offset: bool| {
        if reset_offset {
            class_offset.set(0);
        }
        class_seq.set(class_seq() + 1);
        let seq = class_seq();
        classes_loading.set(true);
        spawn(async move {
            let offset = class_offset();
            let res = request_classes(offset).await;
            if class_seq() == seq {
                match res {
                    Ok(page) => {
                        classes_total.set(page.total);
                        // 末页条目因写入（退役/删除）缩减后旧 offset 可能越界：
                        // 钳回最后一个非空页并重拉一次，避免落地误导性的空列表
                        if page.items.is_empty() && page.total > 0 && offset >= page.total {
                            class_offset.set((page.total - 1) / PAGE_SIZE * PAGE_SIZE);
                            if let Ok(page2) = request_classes(class_offset()).await
                                && class_seq() == seq
                            {
                                classes.set(page2.items);
                            }
                        } else {
                            classes.set(page.items);
                        }
                    }
                    Err(e) => toast.error(format!("加载实体类失败：{e}")),
                }
                classes_loading.set(false);
            }
        });
    };

    let request_relations = move |offset: usize| {
        list_ontology_relation_types(ListOntologyRelationTypesRequest {
            status: parse_status_filter(&relation_status_filter()),
            keyword: non_empty(&relation_keyword()),
            pagination: PaginationParams {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            },
        })
    };

    let mut fetch_relations = move |reset_offset: bool| {
        if reset_offset {
            relation_offset.set(0);
        }
        relation_seq.set(relation_seq() + 1);
        let seq = relation_seq();
        relations_loading.set(true);
        spawn(async move {
            let offset = relation_offset();
            let res = request_relations(offset).await;
            if relation_seq() == seq {
                match res {
                    Ok(page) => {
                        relations_total.set(page.total);
                        // 同 fetch_classes：写入缩减末页后钳回最后一个非空页重拉
                        if page.items.is_empty() && page.total > 0 && offset >= page.total {
                            relation_offset.set((page.total - 1) / PAGE_SIZE * PAGE_SIZE);
                            if let Ok(page2) = request_relations(relation_offset()).await
                                && relation_seq() == seq
                            {
                                relations.set(page2.items);
                            }
                        } else {
                            relations.set(page.items);
                        }
                    }
                    Err(e) => toast.error(format!("加载关系类型失败：{e}")),
                }
                relations_loading.set(false);
            }
        });
    };

    let request_properties = move |offset: usize| {
        list_ontology_properties(ListOntologyPropertiesRequest {
            status: parse_status_filter(&property_status_filter()),
            domain_class: None,
            target_type: None,
            keyword: non_empty(&property_keyword()),
            pagination: PaginationParams {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            },
        })
    };

    let mut fetch_properties = move |reset_offset: bool| {
        if reset_offset {
            property_offset.set(0);
        }
        property_seq.set(property_seq() + 1);
        let seq = property_seq();
        properties_loading.set(true);
        spawn(async move {
            let offset = property_offset();
            let res = request_properties(offset).await;
            if property_seq() == seq {
                match res {
                    Ok(page) => {
                        properties_total.set(page.total);
                        // 同 fetch_classes：写入缩减末页后钳回最后一个非空页重拉
                        if page.items.is_empty() && page.total > 0 && offset >= page.total {
                            property_offset.set((page.total - 1) / PAGE_SIZE * PAGE_SIZE);
                            if let Ok(page2) = request_properties(property_offset()).await
                                && property_seq() == seq
                            {
                                properties.set(page2.items);
                            }
                        } else {
                            properties.set(page.items);
                        }
                    }
                    Err(e) => toast.error(format!("加载属性定义失败：{e}")),
                }
                properties_loading.set(false);
            }
        });
    };

    let request_synonyms = move |offset: usize| {
        list_ontology_synonyms(ListOntologySynonymsRequest {
            target_kind: parse_kind_filter(&synonym_kind_filter()),
            target_key: None,
            pagination: PaginationParams {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            },
        })
    };

    let mut fetch_synonyms = move |reset_offset: bool| {
        if reset_offset {
            synonym_offset.set(0);
        }
        synonym_seq.set(synonym_seq() + 1);
        let seq = synonym_seq();
        synonyms_loading.set(true);
        spawn(async move {
            let offset = synonym_offset();
            let res = request_synonyms(offset).await;
            if synonym_seq() == seq {
                match res {
                    Ok(page) => {
                        synonyms_total.set(page.total);
                        // 同 fetch_classes：同义映射物理删除缩减末页后钳回最后一个非空页重拉
                        if page.items.is_empty() && page.total > 0 && offset >= page.total {
                            synonym_offset.set((page.total - 1) / PAGE_SIZE * PAGE_SIZE);
                            if let Ok(page2) = request_synonyms(synonym_offset()).await
                                && synonym_seq() == seq
                            {
                                synonyms.set(page2.items);
                            }
                        } else {
                            synonyms.set(page.items);
                        }
                    }
                    Err(e) => toast.error(format!("加载同义映射失败：{e}")),
                }
                synonyms_loading.set(false);
            }
        });
    };

    // 骨架一次性全量拉取（词表量级小），供图视图与三元组清单共用
    let mut fetch_skeleton = move || {
        skeleton_seq.set(skeleton_seq() + 1);
        let seq = skeleton_seq();
        skeleton_loading.set(true);
        spawn(async move {
            match get_ontology_skeleton().await {
                Ok(data) => {
                    if skeleton_seq() == seq {
                        skeleton.set(Some(data));
                        skeleton_loading.set(false);
                    }
                }
                Err(e) => {
                    if skeleton_seq() == seq {
                        toast.error(format!("加载本体骨架失败：{e}"));
                        skeleton_loading.set(false);
                    }
                }
            }
        });
    };

    let mut fetch_dashboard = move || {
        dashboard_loading.set(true);
        spawn(async move {
            match get_ontology_drift_dashboard(GetDriftDashboardRequest {
                top_n: Some(DASHBOARD_TOP_N),
                agent_id: None,
            })
            .await
            {
                Ok(d) => dashboard.set(Some(d)),
                Err(e) => {
                    // 清掉旧数据：失败后展示失败占位，避免残留过期看板误导
                    dashboard.set(None);
                    toast.error(format!("加载漂移看板失败：{e}"));
                }
            }
            dashboard_loading.set(false);
        });
    };

    let fetch_lexicon = move || {
        spawn(async move {
            lexicon_loading.set(true);
            match list_ontology_lexicon().await {
                Ok(s) => lexicon_summary.set(Some(s)),
                Err(e) => {
                    // 清掉旧数据：失败后展示失败占位，避免残留过期词表误导
                    lexicon_summary.set(None);
                    toast.error(format!("加载词表注入视图失败：{e}"));
                }
            }
            lexicon_loading.set(false);
        });
    };

    let mut open_drill = move |term: String| {
        drill_term.set(Some(term.clone()));
        drill_relations.set(Vec::new());
        drill_classes.set(Vec::new());
        drill_seq.set(drill_seq() + 1);
        let seq = drill_seq();
        drill_loading.set(true);
        spawn(async move {
            let rel = list_ontology_drift_relation_details(ListDriftRelationDetailsRequest {
                raw_term: term.clone(),
                agent_id: None,
                pagination: PaginationParams {
                    limit: Some(DRILL_DETAIL_LIMIT),
                    offset: Some(0),
                },
            })
            .await;
            let cls = list_ontology_drift_class_details(ListDriftClassDetailsRequest {
                raw_term: term.clone(),
                agent_id: None,
                pagination: PaginationParams {
                    limit: Some(DRILL_DETAIL_LIMIT),
                    offset: Some(0),
                },
            })
            .await;
            // seq 守卫：快速切换下钻词时只让最新一次请求落地，防止旧响应覆盖新视图
            if drill_seq() == seq {
                match rel {
                    Ok(page) => drill_relations.set(page.items),
                    Err(e) => toast.error(format!("加载关系明细失败：{e}")),
                }
                match cls {
                    Ok(page) => drill_classes.set(page.items),
                    Err(e) => toast.error(format!("加载节点明细失败：{e}")),
                }
                drill_loading.set(false);
            }
        });
    };

    // ===== 预置词表同步 =====

    let mut open_sync_modal = move || {
        show_sync_modal.set(true);
        sync_result.set(None);
        sync_preview.set(None);
        sync_preview_loading.set(true);
        sync_strategy.set(PresetOntologySyncStrategy::default());
        sync_confirm_pending.set(false);
        spawn(async move {
            match preview_preset_ontology().await {
                Ok(p) => sync_preview.set(Some(p)),
                Err(e) => toast.error(format!("加载预置词表预览失败：{e}")),
            }
            sync_preview_loading.set(false);
        });
    };

    let mut handle_sync = move || {
        syncing.set(true);
        sync_confirm_pending.set(false);
        spawn(async move {
            match sync_preset_ontology(SyncPresetOntologyRequest {
                strategy: sync_strategy(),
            })
            .await
            {
                Ok(r) => {
                    let msg = format!(
                        "同步完成：新建 {} 条，更新 {} 条，跳过 {} 条",
                        r.created, r.updated, r.skipped
                    );
                    sync_result.set(Some(r));
                    toast.success(msg);
                    fetch_classes(true);
                    fetch_relations(true);
                    fetch_properties(true);
                    fetch_synonyms(true);
                    fetch_dashboard();
                    fetch_lexicon();
                    // 同步成功后刷新 preview：失败要留痕——modal 会继续显示
                    // 同步前的旧预览，静默吞掉会让用户误读为最新状态
                    match preview_preset_ontology().await {
                        Ok(p) => sync_preview.set(Some(p)),
                        Err(e) => toast.warning(format!("同步成功，但刷新预览失败：{e}")),
                    }
                }
                Err(e) => toast.error(format!("同步失败：{e}")),
            }
            syncing.set(false);
        });
    };

    // ===== 实体类写入 =====

    let mut handle_save_class = move || {
        let display_name = class_form_name().trim().to_string();
        if display_name.is_empty() {
            toast.warning("展示名不能为空");
            return;
        }
        let edit_id = class_edit_id();
        let is_edit = edit_id.is_some();
        if edit_id.is_none() && class_form_key().trim().is_empty() {
            toast.warning("规范词 key 不能为空（小写 snake_case）");
            return;
        }
        let required_fields = split_list(&class_form_fields());
        let parent_term_key = non_empty(&class_form_parent());
        class_saving.set(true);
        spawn(async move {
            let res = match edit_id {
                Some(id) => {
                    update_ontology_class(UpdateOntologyClassRequest {
                        id,
                        display_name,
                        description: class_form_desc(),
                        required_fields,
                        parent_term_key,
                    })
                    .await
                }
                None => {
                    create_ontology_class(CreateOntologyClassRequest {
                        term_key: class_form_key().trim().to_string(),
                        display_name,
                        description: class_form_desc(),
                        required_fields,
                        parent_term_key,
                    })
                    .await
                }
            };
            class_saving.set(false);
            match res {
                Ok(_) => {
                    toast.success(if is_edit {
                        "实体类已更新"
                    } else {
                        "实体类已创建"
                    });
                    show_class_modal.set(false);
                    fetch_classes(false);
                    fetch_skeleton();
                    fetch_dashboard();
                    fetch_lexicon();
                }
                Err(e) => toast.error(format!("保存失败：{e}")),
            }
        });
    };

    let mut handle_retire_class = move || {
        if let Some(id) = class_retire_id() {
            class_retire_id.set(None);
            spawn(async move {
                match retire_ontology_class(&id).await {
                    Ok(_) => {
                        toast.success("实体类已退役");
                        fetch_classes(false);
                        fetch_skeleton();
                        fetch_dashboard();
                        fetch_lexicon();
                    }
                    Err(e) => toast.error(format!("退役失败：{e}")),
                }
            });
        }
    };

    // ===== 关系类型写入 =====

    let mut handle_save_relation = move || {
        let display_name = relation_form_name().trim().to_string();
        if display_name.is_empty() {
            toast.warning("展示名不能为空");
            return;
        }
        let edit_id = relation_edit_id();
        let is_edit = edit_id.is_some();
        if edit_id.is_none() && relation_form_key().trim().is_empty() {
            toast.warning("规范词 key 不能为空（小写 snake_case）");
            return;
        }
        let inverse_key = non_empty(&relation_form_inverse());
        let weight_text = relation_form_weight().trim().to_string();
        // 编辑分支权重必填且须可解析：非法值显式拒绝而非静默回退 1.0
        // （创建分支留空传 None 走后端默认，语义不同）
        if is_edit && weight_text.parse::<f64>().is_err() {
            toast.warning("权重必须是合法数字（如 1.0）");
            return;
        }
        relation_saving.set(true);
        spawn(async move {
            let res = match edit_id {
                Some(id) => {
                    let weight_base = weight_text.parse::<f64>().unwrap_or(1.0);
                    update_ontology_relation_type(UpdateOntologyRelationTypeRequest {
                        id,
                        display_name,
                        description: relation_form_desc(),
                        weight_base,
                        inverse_key,
                    })
                    .await
                }
                None => {
                    let weight_base = weight_text.parse::<f64>().ok();
                    create_ontology_relation_type(CreateOntologyRelationTypeRequest {
                        term_key: relation_form_key().trim().to_string(),
                        display_name,
                        description: relation_form_desc(),
                        weight_base,
                        inverse_key,
                        direction: None,
                    })
                    .await
                }
            };
            relation_saving.set(false);
            match res {
                Ok(_) => {
                    toast.success(if is_edit {
                        "关系类型已更新"
                    } else {
                        "关系类型已创建"
                    });
                    show_relation_modal.set(false);
                    fetch_relations(false);
                    fetch_skeleton();
                    fetch_dashboard();
                    fetch_lexicon();
                }
                Err(e) => toast.error(format!("保存失败：{e}")),
            }
        });
    };

    let mut handle_retire_relation = move || {
        if let Some(id) = relation_retire_id() {
            relation_retire_id.set(None);
            spawn(async move {
                match retire_ontology_relation_type(&id).await {
                    Ok(_) => {
                        toast.success("关系类型已退役");
                        fetch_relations(false);
                        fetch_skeleton();
                        fetch_dashboard();
                        fetch_lexicon();
                    }
                    Err(e) => toast.error(format!("退役失败：{e}")),
                }
            });
        }
    };

    // ===== 骨架三元组写入 =====

    let mut open_triple_create = move |subject: String| {
        triple_edit_id.set(None);
        triple_form_subject.set(if subject.is_empty() {
            WILDCARD_CLASS.to_string()
        } else {
            subject
        });
        triple_form_relation.set(String::new());
        triple_form_object.set(WILDCARD_CLASS.to_string());
        triple_form_cardinality.set("many".to_string());
        triple_form_required.set(false);
        show_triple_modal.set(true);
    };

    let mut handle_save_triple = move || {
        let subject = triple_form_subject().trim().to_string();
        let relation = triple_form_relation().trim().to_string();
        let object = triple_form_object().trim().to_string();
        if subject.is_empty() || relation.is_empty() || object.is_empty() {
            toast.warning("主语类、关系词、宾语类都不能为空（不限用 *）");
            return;
        }
        let cardinality_text = triple_form_cardinality();
        let required = triple_form_required();
        let edit_id = triple_edit_id();
        triple_saving.set(true);
        spawn(async move {
            let res = match edit_id {
                Some(id) => {
                    update_ontology_class_relation(UpdateOntologyClassRelationRequest {
                        id,
                        subject_class_key: subject,
                        relation_key: relation,
                        object_class_key: object,
                        cardinality: cardinality_text.clone(),
                        required,
                    })
                    .await
                }
                None => {
                    create_ontology_class_relation(CreateOntologyClassRelationRequest {
                        subject_class_key: subject,
                        relation_key: relation,
                        object_class_key: object,
                        cardinality: Some(cardinality_text),
                        required,
                    })
                    .await
                }
            };
            triple_saving.set(false);
            match res {
                Ok(_) => {
                    toast.success("骨架三元组已保存");
                    show_triple_modal.set(false);
                    fetch_skeleton();
                }
                Err(e) => toast.error(format!("保存失败：{e}")),
            }
        });
    };

    let mut handle_retire_triple = move || {
        if let Some(id) = triple_retire_id() {
            triple_retire_id.set(None);
            spawn(async move {
                match retire_ontology_class_relation(&id).await {
                    Ok(_) => {
                        toast.success("骨架三元组已退役");
                        fetch_skeleton();
                    }
                    Err(e) => toast.error(format!("退役失败：{e}")),
                }
            });
        }
    };

    // ===== 属性定义写入 =====

    let mut handle_save_property = move || {
        let display_name = property_form_name().trim().to_string();
        if display_name.is_empty() {
            toast.warning("展示名不能为空");
            return;
        }
        let edit_id = property_edit_id();
        let is_edit = edit_id.is_some();
        if edit_id.is_none() && property_form_key().trim().is_empty() {
            toast.warning("规范词 key 不能为空（小写 snake_case）");
            return;
        }
        let data_type = parse_data_type(&property_form_data_type());
        let cardinality = parse_cardinality(&property_form_cardinality());
        let applies_to = parse_applies_to(&property_form_applies_to());
        let domain_classes = split_list(&property_form_domain());
        let value_range = parse_value_range_form(
            &property_form_min(),
            &property_form_max(),
            &property_form_enum(),
        );
        let unit = non_empty(&property_form_unit());
        let required = property_form_required();
        property_saving.set(true);
        spawn(async move {
            let res = match edit_id {
                Some(id) => {
                    update_ontology_property(UpdateOntologyPropertyRequest {
                        id,
                        display_name,
                        description: property_form_desc(),
                        domain_classes,
                        applies_to,
                        data_type,
                        value_range,
                        cardinality,
                        unit,
                        required,
                    })
                    .await
                }
                None => {
                    create_ontology_property(CreateOntologyPropertyRequest {
                        term_key: property_form_key().trim().to_string(),
                        display_name,
                        description: property_form_desc(),
                        domain_classes,
                        applies_to,
                        data_type,
                        value_range,
                        cardinality,
                        unit,
                        required,
                    })
                    .await
                }
            };
            property_saving.set(false);
            match res {
                Ok(_) => {
                    toast.success(if is_edit {
                        "属性定义已更新"
                    } else {
                        "属性定义已创建"
                    });
                    show_property_modal.set(false);
                    fetch_properties(false);
                    fetch_lexicon();
                }
                Err(e) => toast.error(format!("保存失败：{e}")),
            }
        });
    };

    let mut handle_retire_property = move || {
        if let Some(id) = property_retire_id() {
            property_retire_id.set(None);
            spawn(async move {
                match retire_ontology_property(&id).await {
                    Ok(_) => {
                        toast.success("属性定义已退役");
                        fetch_properties(false);
                        fetch_lexicon();
                    }
                    Err(e) => toast.error(format!("退役失败：{e}")),
                }
            });
        }
    };

    // ===== 同义映射写入 =====

    let mut handle_save_synonym = move || {
        let raw_term = synonym_form_raw().trim().to_string();
        if raw_term.is_empty() {
            toast.warning("漂移原文不能为空");
            return;
        }
        let target_key = synonym_form_target_key().trim().to_string();
        if target_key.is_empty() {
            toast.warning("目标规范词不能为空");
            return;
        }
        let target_kind = parse_term_kind(&synonym_form_kind());
        synonym_saving.set(true);
        spawn(async move {
            let res = create_ontology_synonym(CreateOntologySynonymRequest {
                raw_term,
                target_kind,
                target_key,
            })
            .await;
            synonym_saving.set(false);
            match res {
                Ok(_) => {
                    toast.success("同义映射已创建");
                    show_synonym_modal.set(false);
                    synonym_form_raw.set(String::new());
                    synonym_form_target_key.set(String::new());
                    fetch_synonyms(false);
                    fetch_dashboard();
                    fetch_lexicon();
                }
                Err(e) => toast.error(format!("创建失败：{e}")),
            }
        });
    };

    let mut handle_delete_synonym = move || {
        if let Some(id) = synonym_delete_id() {
            synonym_delete_id.set(None);
            spawn(async move {
                match delete_ontology_synonym(&id).await {
                    Ok(_) => {
                        toast.success("同义映射已删除");
                        fetch_synonyms(false);
                        fetch_dashboard();
                        fetch_lexicon();
                    }
                    Err(e) => toast.error(format!("删除失败：{e}")),
                }
            });
        }
    };

    // ===== 初始加载 =====
    // fetch_* 闭包的同步段会读写 *_seq 信号（seq++ 防陈旧响应竞态）。若在 effect
    // 同步段直接调用，Dioxus 0.7 会把读到的 seq 信号自动收集为 effect 依赖，且每轮
    // 递增必变 → effect 无限自触发 → 5 个请求/轮的请求风暴卡死页面。
    // 移入 spawn 异步段执行：异步段内的信号读取不注册 effect 依赖，effect 仅挂载执行一次。

    use_effect(move || {
        spawn(async move {
            fetch_classes(true);
            fetch_relations(true);
            fetch_properties(true);
            fetch_synonyms(true);
            fetch_skeleton();
            fetch_dashboard();
            fetch_lexicon();
        });
    });

    rsx! {
        AppLayout {
            HudPanel { signal: Some(true),
                div { class: "card-body",
                    PageHeader {
                        eyebrow: Some("HR".to_string()),
                        title: "本体词表管理".to_string(),
                        actions: Some(rsx! {
                            button { class: "btn hud-btn btn-primary",
                                onclick: move |_| open_sync_modal(),
                                "⟳ 同步预置词表"
                            }
                        }),
                    }

                    // ==================== 子标签栏 ====================
                    div { class: "flex flex-wrap gap-2 mb-4",
                        for tab in [
                            OntologyTab::Overview,
                            OntologyTab::Skeleton,
                            OntologyTab::Classes,
                            OntologyTab::Relations,
                            OntologyTab::Properties,
                            OntologyTab::Synonyms,
                        ] {
                            button {
                                class: if active_tab() == tab {
                                    "btn hud-btn btn-sm btn-primary"
                                } else {
                                    "btn hud-btn btn-sm btn-ghost"
                                },
                                onclick: move |_| active_tab.set(tab),
                                "{tab.label()}"
                            }
                        }
                    }

                    if active_tab() == OntologyTab::Overview {
                        // ==================== 词表注入视图 ====================
                        HudSection {
                            eyebrow: Some("LEXICON".to_string()),
                            title: "词表注入视图".to_string(),
                            actions: Some(rsx! {
                                button { class: "btn hud-btn btn-ghost btn-sm",
                                    onclick: move |_| fetch_lexicon(),
                                    "刷新"
                                }
                            }),
                            if lexicon_loading() {
                                Loading { size: "md" }
                            } else if let Some(s) = lexicon_summary() {
                                if s.is_empty() {
                                    EmptyState {
                                        icon: Some("📖".to_string()),
                                        message: "词表为空，Agent 技能暂不注入本体词表，可点击右上角同步预置词表"
                                            .to_string(),
                                    }
                                } else {
                                    div { class: "space-y-4",
                                        div { class: "flex flex-wrap items-center gap-3 text-sm",
                                            span { class: "badge hud-badge badge-primary", "关系词 {s.relation_types.len()}" }
                                            span { class: "badge hud-badge badge-secondary", "实体类 {s.classes.len()}" }
                                            span { class: "badge hud-badge badge-ghost", "同义样例 {s.synonyms.len()}" }
                                            span { class: "opacity-70", "与神经技能注入共用同一契约，Token 超限时按 关系词 > 实体类 > 同义 顺序裁剪" }
                                        }
                                        div { class: "space-y-1",
                                            div { class: "text-xs font-semibold uppercase tracking-wider opacity-60", "关系类型（注入优先级最高）" }
                                            div { class: "flex flex-wrap gap-2",
                                                for t in s.relation_types {
                                                    span { class: "badge hud-badge badge-outline", "{t.term_key} · {t.display_name}" }
                                                }
                                            }
                                        }
                                        div { class: "space-y-1",
                                            div { class: "text-xs font-semibold uppercase tracking-wider opacity-60", "实体类" }
                                            div { class: "flex flex-wrap gap-2",
                                                for t in s.classes {
                                                    span { class: "badge hud-badge badge-outline", "{t.term_key} · {t.display_name}" }
                                                }
                                            }
                                        }
                                        div { class: "space-y-1",
                                            div { class: "text-xs font-semibold uppercase tracking-wider opacity-60", "同义映射样例" }
                                            div { class: "flex flex-wrap gap-2",
                                                for x in s.synonyms {
                                                    span { class: "badge hud-badge badge-ghost font-mono", "{x.raw_term} → {x.target_key}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                EmptyState {
                                    icon: Some("⚠️".to_string()),
                                    message: "词表注入视图加载失败，请点击右上角刷新重试".to_string(),
                                }
                            }
                        }
                    }

                    if active_tab() == OntologyTab::Overview {
                        // ==================== 漂移看板 ====================
                        HudSection {
                            eyebrow: Some("DRIFT".to_string()),
                            title: "漂移看板".to_string(),
                            actions: Some(rsx! {
                                button { class: "btn hud-btn btn-ghost btn-sm",
                                    onclick: move |_| fetch_dashboard(),
                                    "刷新"
                                }
                            }),
                            StatGrid {
                                if let Some(d) = dashboard() {
                                    StatReadout {
                                        label: "词表覆盖率".to_string(),
                                        value: format_relevance(
                                            d.relation_coverage.coverage_ratio as f32,
                                        ),
                                        icon: Some("🎯".to_string()),
                                        delta: Some(format!(
                                            "规范 {} · 同义 {} · 漂移 {}",
                                            d.relation_coverage.canonical_relations,
                                            d.relation_coverage.via_synonym_relations,
                                            d.relation_coverage.drift_relations
                                        )),
                                    }
                                    StatReadout {
                                        label: "漂移关系数".to_string(),
                                        value: format_compact_count(
                                            d.relation_coverage.drift_relations,
                                        ),
                                        icon: Some("🌊".to_string()),
                                        delta: Some("词表外关系边总数".to_string()),
                                    }
                                    StatReadout {
                                        label: "漂移节点".to_string(),
                                        value: format_compact_count(d.drift_node_count),
                                        unit: Some("个".to_string()),
                                        icon: Some("🧩".to_string()),
                                        delta: Some(format!(
                                            "占节点总数 {:.0}%",
                                            if d.total_node_count > 0 {
                                                d.drift_node_count as f64
                                                    / d.total_node_count as f64
                                                    * 100.0
                                            } else {
                                                0.0
                                            }
                                        )),
                                    }
                                } else if dashboard_loading() {
                                    Loading { size: "md" }
                                } else {
                                    EmptyState {
                                        icon: Some("⚠️".to_string()),
                                        message: "漂移看板加载失败，请点击右上角刷新重试".to_string(),
                                    }
                                }
                            }
                            if let Some(d) = dashboard() {
                                if d.top_drift_words.is_empty() {
                                    EmptyState {
                                        icon: Some("✨".to_string()),
                                        message: "暂无漂移词，词表覆盖良好".to_string(),
                                    }
                                } else {
                                    div { class: "overflow-x-auto",
                                        table { class: "table hud-table table-fixed table-zebra table-pin-rows w-full",
                                            colgroup {
                                                col {}
                                                col { class: "w-[96px]" }
                                                col { class: "w-[80px]" }
                                                col { class: "w-[96px]" }
                                                col { class: "w-[80px]" }
                                            }
                                            thead {
                                                tr {
                                                    th { "漂移原文" }
                                                    th { "种类" }
                                                    th { "词频" }
                                                    th { "Agent 数" }
                                                    th { "" }
                                                }
                                            }
                                            tbody {
                                                for w in d.top_drift_words {
                                                    tr { key: "{w.raw_term}",
                                                        td { span { class: "font-mono text-sm block truncate", title: "{w.raw_term}", "{w.raw_term}" } }
                                                        td { {kind_badge(w.kind)} }
                                                        td { "{format_compact_count(w.count)}" }
                                                        td { "{w.agent_count}" }
                                                        td {
                                                            button { class: "btn hud-btn btn-ghost btn-xs",
                                                                onclick: move |_| open_drill(w.raw_term.clone()),
                                                                "下钻"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if active_tab() == OntologyTab::Skeleton {
                        // ==================== 本体骨架 ====================
                        HudSection {
                            eyebrow: Some("SKELETON".to_string()),
                            title: "本体骨架".to_string(),
                            actions: Some(rsx! {
                                button { class: "btn hud-btn btn-primary btn-sm",
                                    onclick: move |_| open_triple_create(String::new()),
                                    "+ 新增三元组"
                                }
                                button { class: "btn hud-btn btn-ghost btn-sm",
                                    onclick: move |_| fetch_skeleton(),
                                    "刷新"
                                }
                            }),
                            if skeleton_loading() && skeleton().is_none() {
                                Loading { size: "md" }
                            } else if let Some(s) = skeleton() {
                                {
                                    let inheritance_count =
                                        s.classes.iter().filter(|c| c.parent_term_key.is_some()).count();
                                    let wildcard_count = s
                                        .class_relations
                                        .iter()
                                        .filter(|r| {
                                            r.subject_class_key == WILDCARD_CLASS
                                                || r.object_class_key == WILDCARD_CLASS
                                        })
                                        .count();
                                    rsx! {
                                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-3 mb-3",
                                            div { class: "rounded-xl border border-base-300 bg-base-100/40 p-3",
                                                div { class: "text-xs opacity-60", "实体类" }
                                                div { class: "text-2xl font-semibold", "{s.classes.len()}" }
                                            }
                                            div { class: "rounded-xl border border-base-300 bg-base-100/40 p-3",
                                                div { class: "text-xs opacity-60", "继承边" }
                                                div { class: "text-2xl font-semibold", "{inheritance_count}" }
                                            }
                                            div { class: "rounded-xl border border-base-300 bg-base-100/40 p-3",
                                                div { class: "text-xs opacity-60", "关系类型" }
                                                div { class: "text-2xl font-semibold", "{s.relation_types.len()}" }
                                            }
                                            div { class: "rounded-xl border border-base-300 bg-base-100/40 p-3",
                                                div { class: "text-xs opacity-60", "骨架三元组" }
                                                div { class: "text-2xl font-semibold", "{s.class_relations.len()}" }
                                                div { class: "text-[11px] opacity-50", "其中通配 {wildcard_count} 条" }
                                            }
                                        }
                                        div { class: "grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_320px] gap-3",
                                            div { class: "rounded-xl border border-base-300 bg-base-100/40 p-2 overflow-x-auto",
                                                if s.classes.is_empty() {
                                                    EmptyState {
                                                        icon: Some("🕸️".to_string()),
                                                        message: "暂无实体类，先去「实体类词表」新增或同步预置词表".to_string(),
                                                    }
                                                } else {
                                                    { render_skeleton_graph(
                                                        &s.classes,
                                                        &s.class_relations,
                                                        &s.relation_types,
                                                        skeleton_selected_class(),
                                                        EventHandler::new(move |key: String| skeleton_selected_class.set(Some(key))),
                                                    ) }
                                                }
                                            }
                                            div { class: "rounded-xl border border-base-300 bg-base-100/40 p-4 space-y-3",
                                                if let Some(sel) = skeleton_selected_class() {
                                                    {
                                                        let selected_class = s.classes.iter().find(|c| c.term_key == sel).cloned();
                                                        let children: Vec<OntologyClassItem> = s
                                                            .classes
                                                            .iter()
                                                            .filter(|c| c.parent_term_key.as_deref() == Some(sel.as_str()))
                                                            .cloned()
                                                            .collect();
                                                        let related: Vec<OntologyClassRelationItem> = s
                                                            .class_relations
                                                            .iter()
                                                            .filter(|r| r.subject_class_key == sel || r.object_class_key == sel)
                                                            .cloned()
                                                            .collect();
                                                        let sel_for_child = sel.clone();
                                                        let sel_for_rel = sel.clone();
                                                        rsx! {
                                                            div { class: "flex items-center justify-between gap-2",
                                                                div {
                                                                    div { class: "font-mono text-sm", "{sel}" }
                                                                    div { class: "text-xs opacity-60",
                                                                        "{selected_class.as_ref().map(|c| c.display_name.clone()).unwrap_or_default()}"
                                                                    }
                                                                }
                                                                button { class: "btn hud-btn btn-ghost btn-xs",
                                                                    onclick: move |_| skeleton_selected_class.set(None),
                                                                    "取消选中"
                                                                }
                                                            }
                                                            if let Some(c) = selected_class {
                                                                div { class: "text-xs space-y-1",
                                                                    div {
                                                                        span { class: "opacity-60", "父类：" }
                                                                        if let Some(parent) = c.parent_term_key.clone() {
                                                                            span { class: "font-mono", "{parent}" }
                                                                        } else {
                                                                            span { class: "opacity-50", "无（根类）" }
                                                                        }
                                                                    }
                                                                    div {
                                                                        span { class: "opacity-60", "必填属性：" }
                                                                        if c.required_fields.is_empty() {
                                                                            span { class: "opacity-50", "-" }
                                                                        } else {
                                                                            span { class: "font-mono", "{c.required_fields.join(\", \")}" }
                                                                        }
                                                                    }
                                                                    div { class: "opacity-60 line-clamp-3", "{c.description}" }
                                                                }
                                                            }
                                                            div { class: "text-xs",
                                                                div { class: "opacity-60 mb-1", "子类（{children.len()}）" }
                                                                if children.is_empty() {
                                                                    div { class: "opacity-50", "无" }
                                                                } else {
                                                                    div { class: "flex flex-wrap gap-1",
                                                                        for ch in children {
                                                                            span { key: "{ch.id}", class: "badge orz-tag badge-sm font-mono", "{ch.term_key}" }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            div { class: "text-xs",
                                                                div { class: "opacity-60 mb-1", "关联约束（{related.len()}）" }
                                                                if related.is_empty() {
                                                                    div { class: "opacity-50", "无" }
                                                                } else {
                                                                    div { class: "space-y-1",
                                                                        for r in related {
                                                                            div { key: "{r.id}", class: "font-mono text-[11px] truncate",
                                                                                "{r.subject_class_key} -[{r.relation_key}]-> {r.object_class_key}"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            div { class: "flex flex-wrap gap-2 pt-1",
                                                                button { class: "btn hud-btn btn-primary btn-xs",
                                                                    onclick: move |_| {
                                                                        class_edit_id.set(None);
                                                                        class_form_key.set(String::new());
                                                                        class_form_name.set(String::new());
                                                                        class_form_desc.set(String::new());
                                                                        class_form_fields.set(String::new());
                                                                        class_form_parent.set(sel_for_child.clone());
                                                                        show_class_modal.set(true);
                                                                    },
                                                                    "新建子类"
                                                                }
                                                                button { class: "btn hud-btn btn-ghost btn-xs",
                                                                    onclick: move |_| open_triple_create(sel_for_rel.clone()),
                                                                    "为它加关系"
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    EmptyState {
                                                        icon: Some("👆".to_string()),
                                                        message: "点击左侧节点查看继承链与约束".to_string(),
                                                    }
                                                }
                                            }
                                        }
                                        div { class: "mt-4",
                                            h4 { class: "font-semibold mb-2 text-sm", "类-关系-类 骨架三元组" }
                                            if s.class_relations.is_empty() {
                                                EmptyState {
                                                    icon: Some("🔗".to_string()),
                                                    message: "暂无骨架三元组：无三元组 = 该关系不被约束".to_string(),
                                                }
                                            } else {
                                                div { class: "overflow-x-auto",
                                                    table { class: "table hud-table table-fixed table-zebra table-pin-rows w-full",
                                                        colgroup {
                                                            col {}
                                                            col {}
                                                            col {}
                                                            col { class: "w-[88px]" }
                                                            col { class: "w-[64px]" }
                                                            col { class: "w-[120px]" }
                                                        }
                                                        thead {
                                                            tr {
                                                                th { "主语类" }
                                                                th { "关系词" }
                                                                th { "宾语类" }
                                                                th { "基数" }
                                                                th { "必填" }
                                                                th { "操作" }
                                                            }
                                                        }
                                                        tbody {
                                                            for r in s.class_relations.iter().cloned() {
                                                                {
                                                                    let edit_id = r.id.clone();
                                                                    let edit_subject = r.subject_class_key.clone();
                                                                    let edit_relation = r.relation_key.clone();
                                                                    let edit_object = r.object_class_key.clone();
                                                                    let edit_cardinality = r.cardinality.clone();
                                                                    let edit_required = r.required;
                                                                    let retire_id = r.id.clone();
                                                                    rsx! {
                                                                        tr { key: "{r.id}",
                                                                            td { class: "font-mono text-xs truncate", "{r.subject_class_key}" }
                                                                            td { class: "font-mono text-xs truncate", "{r.relation_key}" }
                                                                            td { class: "font-mono text-xs truncate", "{r.object_class_key}" }
                                                                            td {
                                                                                span { class: "badge badge-ghost badge-sm", "{r.cardinality}" }
                                                                            }
                                                                            td {
                                                                                if r.required {
                                                                                    span { class: "badge badge-warning badge-sm", "是" }
                                                                                } else {
                                                                                    span { class: "opacity-50", "否" }
                                                                                }
                                                                            }
                                                                            td {
                                                                                div { class: "flex gap-2 items-center",
                                                                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                                                                        onclick: move |_| {
                                                                                            triple_edit_id.set(Some(edit_id.clone()));
                                                                                            triple_form_subject.set(edit_subject.clone());
                                                                                            triple_form_relation.set(edit_relation.clone());
                                                                                            triple_form_object.set(edit_object.clone());
                                                                                            triple_form_cardinality.set(edit_cardinality.clone());
                                                                                            triple_form_required.set(edit_required);
                                                                                            show_triple_modal.set(true);
                                                                                        },
                                                                                        "编辑"
                                                                                    }
                                                                                    button { class: "btn hud-btn btn-error btn-sm",
                                                                                        onclick: move |_| triple_retire_id.set(Some(retire_id.clone())),
                                                                                        "退役"
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                EmptyState {
                                    icon: Some("🕸️".to_string()),
                                    message: "骨架加载失败，请点击右上角刷新重试".to_string(),
                                }
                            }
                        }
                    }

                    if active_tab() == OntologyTab::Classes {
                        // ==================== 实体类词表 ====================
                        HudSection {
                            eyebrow: Some("CLASSES".to_string()),
                            title: "实体类词表".to_string(),
                            actions: Some(rsx! {
                                button { class: "btn hud-btn btn-primary btn-sm",
                                    onclick: move |_| {
                                        class_edit_id.set(None);
                                        class_form_key.set(String::new());
                                        class_form_name.set(String::new());
                                        class_form_desc.set(String::new());
                                        class_form_fields.set(String::new());
                                        class_form_parent.set(String::new());
                                        show_class_modal.set(true);
                                    },
                                    "+ 新增实体类"
                                }
                            }),
                            div { class: "flex flex-wrap gap-4 items-end",
                                div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                                    label { class: "label", span { class: "label-text", "状态" } }
                                    select { class: "select select-bordered hud-input w-full",
                                        value: "{class_status_filter}",
                                        onchange: move |e| {
                                            class_status_filter.set(e.value());
                                            fetch_classes(true);
                                        },
                                        option { value: "", "全部" }
                                        option { value: "Active", "启用中" }
                                        option { value: "Retired", "已退役" }
                                    }
                                }
                                div { class: "flex flex-col gap-1 min-w-[140px] flex-[2]",
                                    label { class: "label", span { class: "label-text", "关键词" } }
                                    input { class: "input input-bordered hud-input w-full",
                                        value: "{class_keyword}",
                                        oninput: move |e| class_keyword.set(e.value()),
                                        onkeydown: move |evt| {
                                            if evt.key() == Key::Enter {
                                                fetch_classes(true);
                                            }
                                        },
                                        placeholder: "按规范词 / 展示名模糊匹配，回车查询"
                                    }
                                }
                                div { class: "flex flex-col gap-1",
                                    button { class: "btn hud-btn btn-primary",
                                        onclick: move |_| fetch_classes(true),
                                        "查询"
                                    }
                                }
                            }
                            if classes_loading() {
                                Loading { size: "md" }
                            } else if classes.is_empty() {
                                EmptyState { message: "暂无实体类词条，点击右上角新增或同步预置词表".to_string() }
                            } else {
                                div { class: "overflow-x-auto",
                                    table { class: "table hud-table table-fixed table-zebra table-pin-rows w-full",
                                        colgroup {
                                            col {}
                                            col {}
                                            col {}
                                            col { class: "w-[120px]" }
                                            col {}
                                            col { class: "w-[88px]" }
                                            col { class: "w-[140px]" }
                                            col { class: "w-[132px]" }
                                        }
                                        thead {
                                            tr {
                                                th { "规范词" }
                                                th { "展示名" }
                                                th { "描述" }
                                                th { "父类" }
                                                th { "必填属性" }
                                                th { "状态" }
                                                th { "更新时间" }
                                                th { "操作" }
                                            }
                                        }
                                        tbody {
                                            for c in classes().iter().cloned() {
                                                {
                                                    let edit_item = c.clone();
                                                    let retire_id = c.id.clone();
                                                    rsx! {
                                                        tr { key: "{c.id}",
                                                            td { span { class: "font-mono text-sm block truncate", title: "{c.term_key}", "{c.term_key}" } }
                                                            td { class: "truncate", "{c.display_name}" }
                                                            td {
                                                                div { class: "line-clamp-2", title: "{c.description}",
                                                                    "{c.description}"
                                                                }
                                                            }
                                                            td {
                                                                if c.required_fields.is_empty() {
                                                                    span { class: "opacity-50", "-" }
                                                                } else {
                                                                    span { class: "font-mono text-xs block truncate", title: "{c.required_fields.join(\", \")}", "{c.required_fields.join(\", \")}" }
                                                                }
                                                            }
                                                            td {
                                                                if let Some(parent) = c.parent_term_key.clone() {
                                                                    span { class: "font-mono text-xs block truncate", title: "{parent}", "{parent}" }
                                                                } else {
                                                                    span { class: "opacity-50", "根类" }
                                                                }
                                                            }
                                                            td { {status_badge(c.status)} }
                                                            td { class: "whitespace-nowrap", "{format_datetime(c.updated_at * 1000)}" }
                                                            td {
                                                                div { class: "flex gap-2 items-center",
                                                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                                                        onclick: move |_| {
                                                                            class_edit_id.set(Some(edit_item.id.clone()));
                                                                            class_form_key.set(edit_item.term_key.clone());
                                                                            class_form_name.set(edit_item.display_name.clone());
                                                                            class_form_desc.set(edit_item.description.clone());
                                                                            class_form_fields.set(edit_item.required_fields.join(", "));
                                                                            class_form_parent.set(edit_item.parent_term_key.clone().unwrap_or_default());
                                                                            show_class_modal.set(true);
                                                                        },
                                                                        "编辑"
                                                                    }
                                                                    button { class: "btn hud-btn btn-error btn-sm",
                                                                        disabled: c.status == OntologyStatus::Retired,
                                                                        onclick: move |_| class_retire_id.set(Some(retire_id.clone())),
                                                                        "退役"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if classes_total() > 0 {
                                div { class: "flex items-center justify-between mt-3",
                                    span { class: "text-sm opacity-70",
                                        "共 {classes_total()} 条"
                                    }
                                    div { class: "flex gap-2",
                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: class_offset() == 0,
                                            onclick: move |_| {
                                                class_offset.set(class_offset().saturating_sub(PAGE_SIZE));
                                                fetch_classes(false);
                                            },
                                            "上一页"
                                        }
                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: class_offset() + PAGE_SIZE >= classes_total(),
                                            onclick: move |_| {
                                                class_offset.set(class_offset() + PAGE_SIZE);
                                                fetch_classes(false);
                                            },
                                            "下一页"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if active_tab() == OntologyTab::Relations {
                        // ==================== 关系类型词表 ====================
                        HudSection {
                            eyebrow: Some("RELATIONS".to_string()),
                            title: "关系类型词表".to_string(),
                            actions: Some(rsx! {
                                button { class: "btn hud-btn btn-primary btn-sm",
                                    onclick: move |_| {
                                        relation_edit_id.set(None);
                                        relation_form_key.set(String::new());
                                        relation_form_name.set(String::new());
                                        relation_form_desc.set(String::new());
                                        relation_form_weight.set(String::new());
                                        relation_form_inverse.set(String::new());
                                        show_relation_modal.set(true);
                                    },
                                    "+ 新增关系类型"
                                }
                            }),
                            div { class: "flex flex-wrap gap-4 items-end",
                                div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                                    label { class: "label", span { class: "label-text", "状态" } }
                                    select { class: "select select-bordered hud-input w-full",
                                        value: "{relation_status_filter}",
                                        onchange: move |e| {
                                            relation_status_filter.set(e.value());
                                            fetch_relations(true);
                                        },
                                        option { value: "", "全部" }
                                        option { value: "Active", "启用中" }
                                        option { value: "Retired", "已退役" }
                                    }
                                }
                                div { class: "flex flex-col gap-1 min-w-[140px] flex-[2]",
                                    label { class: "label", span { class: "label-text", "关键词" } }
                                    input { class: "input input-bordered hud-input w-full",
                                        value: "{relation_keyword}",
                                        oninput: move |e| relation_keyword.set(e.value()),
                                        onkeydown: move |evt| {
                                            if evt.key() == Key::Enter {
                                                fetch_relations(true);
                                            }
                                        },
                                        placeholder: "按规范词 / 展示名模糊匹配，回车查询"
                                    }
                                }
                                div { class: "flex flex-col gap-1",
                                    button { class: "btn hud-btn btn-primary",
                                        onclick: move |_| fetch_relations(true),
                                        "查询"
                                    }
                                }
                            }
                            if relations_loading() {
                                Loading { size: "md" }
                            } else if relations.is_empty() {
                                EmptyState { message: "暂无关系类型词条，点击右上角新增或同步预置词表".to_string() }
                            } else {
                                div { class: "overflow-x-auto",
                                    table { class: "table hud-table table-fixed table-zebra table-pin-rows w-full",
                                        colgroup {
                                            col {}
                                            col {}
                                            col {}
                                            col { class: "w-[72px]" }
                                            col {}
                                            col { class: "w-[88px]" }
                                            col { class: "w-[140px]" }
                                            col { class: "w-[132px]" }
                                        }
                                        thead {
                                            tr {
                                                th { "规范词" }
                                                th { "展示名" }
                                                th { "描述" }
                                                th { "权重" }
                                                th { "反向词" }
                                                th { "状态" }
                                                th { "更新时间" }
                                                th { "操作" }
                                            }
                                        }
                                        tbody {
                                            for r in relations().iter().cloned() {
                                                {
                                                    let edit_item = r.clone();
                                                    let retire_id = r.id.clone();
                                                    rsx! {
                                                        tr { key: "{r.id}",
                                                            td { span { class: "font-mono text-sm block truncate", title: "{r.term_key}", "{r.term_key}" } }
                                                            td { class: "truncate", "{r.display_name}" }
                                                            td {
                                                                div { class: "line-clamp-2", title: "{r.description}",
                                                                    "{r.description}"
                                                                }
                                                            }
                                                            td { "{r.weight_base}" }
                                                            td {
                                                                if let Some(inv) = r.inverse_key {
                                                                    span { class: "font-mono text-xs block truncate", title: "{inv}", "{inv}" }
                                                                } else {
                                                                    span { class: "opacity-50", "-" }
                                                                }
                                                            }
                                                            td { {status_badge(r.status)} }
                                                            td { class: "whitespace-nowrap", "{format_datetime(r.updated_at * 1000)}" }
                                                            td {
                                                                div { class: "flex gap-2 items-center",
                                                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                                                        onclick: move |_| {
                                                                            relation_edit_id.set(Some(edit_item.id.clone()));
                                                                            relation_form_key.set(edit_item.term_key.clone());
                                                                            relation_form_name.set(edit_item.display_name.clone());
                                                                            relation_form_desc.set(edit_item.description.clone());
                                                                            relation_form_weight.set(format!("{}", edit_item.weight_base));
                                                                            relation_form_inverse.set(edit_item.inverse_key.clone().unwrap_or_default());
                                                                            show_relation_modal.set(true);
                                                                        },
                                                                        "编辑"
                                                                    }
                                                                    button { class: "btn hud-btn btn-error btn-sm",
                                                                        disabled: r.status == OntologyStatus::Retired,
                                                                        onclick: move |_| relation_retire_id.set(Some(retire_id.clone())),
                                                                        "退役"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if relations_total() > 0 {
                                div { class: "flex items-center justify-between mt-3",
                                    span { class: "text-sm opacity-70",
                                        "共 {relations_total()} 条"
                                    }
                                    div { class: "flex gap-2",
                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: relation_offset() == 0,
                                            onclick: move |_| {
                                                relation_offset.set(relation_offset().saturating_sub(PAGE_SIZE));
                                                fetch_relations(false);
                                            },
                                            "上一页"
                                        }
                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: relation_offset() + PAGE_SIZE >= relations_total(),
                                            onclick: move |_| {
                                                relation_offset.set(relation_offset() + PAGE_SIZE);
                                                fetch_relations(false);
                                            },
                                            "下一页"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if active_tab() == OntologyTab::Properties {
                        // ==================== 属性定义 ====================
                        HudSection {
                            eyebrow: Some("PROPERTIES".to_string()),
                            title: "属性定义".to_string(),
                            actions: Some(rsx! {
                                button { class: "btn hud-btn btn-primary btn-sm",
                                    onclick: move |_| {
                                        property_edit_id.set(None);
                                        property_form_key.set(String::new());
                                        property_form_name.set(String::new());
                                        property_form_desc.set(String::new());
                                        property_form_data_type.set("string".to_string());
                                        property_form_cardinality.set("single".to_string());
                                        property_form_applies_to.set(String::new());
                                        property_form_domain.set(String::new());
                                        property_form_min.set(String::new());
                                        property_form_max.set(String::new());
                                        property_form_enum.set(String::new());
                                        property_form_unit.set(String::new());
                                        property_form_required.set(false);
                                        show_property_modal.set(true);
                                    },
                                    "+ 新增属性"
                                }
                            }),
                            div { class: "flex flex-wrap gap-4 items-end",
                                div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                                    label { class: "label", span { class: "label-text", "状态" } }
                                    select { class: "select select-bordered hud-input w-full",
                                        value: "{property_status_filter}",
                                        onchange: move |e| {
                                            property_status_filter.set(e.value());
                                            fetch_properties(true);
                                        },
                                        option { value: "", "全部" }
                                        option { value: "Active", "启用中" }
                                        option { value: "Retired", "已退役" }
                                    }
                                }
                                div { class: "flex flex-col gap-1 min-w-[140px] flex-[2]",
                                    label { class: "label", span { class: "label-text", "关键词" } }
                                    input { class: "input input-bordered hud-input w-full",
                                        value: "{property_keyword}",
                                        oninput: move |e| property_keyword.set(e.value()),
                                        onkeydown: move |evt| {
                                            if evt.key() == Key::Enter {
                                                fetch_properties(true);
                                            }
                                        },
                                        placeholder: "按规范词 / 展示名模糊匹配，回车查询"
                                    }
                                }
                                div { class: "flex flex-col gap-1",
                                    button { class: "btn hud-btn btn-primary",
                                        onclick: move |_| fetch_properties(true),
                                        "查询"
                                    }
                                }
                            }
                            if properties_loading() {
                                Loading { size: "md" }
                            } else if properties.is_empty() {
                                EmptyState { message: "暂无属性定义，点击右上角新增或同步预置词表".to_string() }
                            } else {
                                div { class: "overflow-x-auto",
                                    table { class: "table hud-table table-fixed table-zebra table-pin-rows w-full",
                                        colgroup {
                                            col {}
                                            col {}
                                            col {}
                                            col { class: "w-[84px]" }
                                            col { class: "w-[104px]" }
                                            col { class: "w-[96px]" }
                                            col { class: "w-[160px]" }
                                            col { class: "w-[88px]" }
                                            col { class: "w-[140px]" }
                                            col { class: "w-[132px]" }
                                        }
                                        thead {
                                            tr {
                                                th { "规范词" }
                                                th { "展示名" }
                                                th { "描述" }
                                                th { "值类型" }
                                                th { "基数 / 必填" }
                                                th { "载体" }
                                                th { "取值约束" }
                                                th { "状态" }
                                                th { "更新时间" }
                                                th { "操作" }
                                            }
                                        }
                                        tbody {
                                            for p in properties().iter().cloned() {
                                                {
                                                    let edit_item = p.clone();
                                                    let retire_id = p.id.clone();
                                                    let hint = p.value_range.to_hint();
                                                    let applies = if p.applies_to.is_empty() {
                                                        "节点/边".to_string()
                                                    } else {
                                                        applies_to_text(&p.applies_to)
                                                    };
                                                    let cardinality_label = format!(
                                                        "{}{}",
                                                        p.cardinality.as_str(),
                                                        if p.required { " · 必填" } else { "" }
                                                    );
                                                    rsx! {
                                                        tr { key: "{p.id}",
                                                            td { span { class: "font-mono text-sm block truncate", title: "{p.term_key}", "{p.term_key}" } }
                                                            td { class: "truncate", "{p.display_name}" }
                                                            td {
                                                                div { class: "line-clamp-2", title: "{p.description}",
                                                                    "{p.description}"
                                                                }
                                                            }
                                                            td { span { class: "font-mono text-xs", "{p.data_type.as_str()}" } }
                                                            td { class: "text-xs", "{cardinality_label}" }
                                                            td { class: "text-xs truncate", title: "{applies}", "{applies}" }
                                                            td {
                                                                if hint.is_empty() {
                                                                    span { class: "opacity-50", "-" }
                                                                } else {
                                                                    span { class: "font-mono text-xs block truncate", title: "{hint}", "{hint}" }
                                                                }
                                                            }
                                                            td { {status_badge(p.status)} }
                                                            td { class: "whitespace-nowrap", "{format_datetime(p.updated_at * 1000)}" }
                                                            td {
                                                                div { class: "flex gap-2 items-center",
                                                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                                                        onclick: move |_| {
                                                                            property_edit_id.set(Some(edit_item.id.clone()));
                                                                            property_form_key.set(edit_item.term_key.clone());
                                                                            property_form_name.set(edit_item.display_name.clone());
                                                                            property_form_desc.set(edit_item.description.clone());
                                                                            property_form_data_type.set(edit_item.data_type.as_str().to_string());
                                                                            property_form_cardinality.set(edit_item.cardinality.as_str().to_string());
                                                                            property_form_applies_to.set(applies_to_text(&edit_item.applies_to));
                                                                            property_form_domain.set(edit_item.domain_classes.join(", "));
                                                                            property_form_min.set(edit_item.value_range.min.map(|v| v.to_string()).unwrap_or_default());
                                                                            property_form_max.set(edit_item.value_range.max.map(|v| v.to_string()).unwrap_or_default());
                                                                            property_form_enum.set(edit_item.value_range.enum_values.join(", "));
                                                                            property_form_unit.set(edit_item.unit.clone().unwrap_or_default());
                                                                            property_form_required.set(edit_item.required);
                                                                            show_property_modal.set(true);
                                                                        },
                                                                        "编辑"
                                                                    }
                                                                    button { class: "btn hud-btn btn-error btn-sm",
                                                                        disabled: p.status == OntologyStatus::Retired,
                                                                        onclick: move |_| property_retire_id.set(Some(retire_id.clone())),
                                                                        "退役"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if properties_total() > 0 {
                                div { class: "flex items-center justify-between mt-3",
                                    span { class: "text-sm opacity-70",
                                        "共 {properties_total()} 条"
                                    }
                                    div { class: "flex gap-2",
                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: property_offset() == 0,
                                            onclick: move |_| {
                                                property_offset.set(property_offset().saturating_sub(PAGE_SIZE));
                                                fetch_properties(false);
                                            },
                                            "上一页"
                                        }
                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: property_offset() + PAGE_SIZE >= properties_total(),
                                            onclick: move |_| {
                                                property_offset.set(property_offset() + PAGE_SIZE);
                                                fetch_properties(false);
                                            },
                                            "下一页"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if active_tab() == OntologyTab::Synonyms {
                        // ==================== 同义映射 ====================
                        HudSection {
                            eyebrow: Some("SYNONYMS".to_string()),
                            title: "同义映射".to_string(),
                            actions: Some(rsx! {
                                button { class: "btn hud-btn btn-primary btn-sm",
                                    onclick: move |_| {
                                        synonym_form_raw.set(String::new());
                                        synonym_form_kind.set("relation".to_string());
                                        synonym_form_target_key.set(String::new());
                                        show_synonym_modal.set(true);
                                    },
                                    "+ 新增同义映射"
                                }
                            }),
                            div { class: "flex flex-wrap gap-4 items-end",
                                div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                                    label { class: "label", span { class: "label-text", "目标种类" } }
                                    select { class: "select select-bordered hud-input w-full",
                                        value: "{synonym_kind_filter}",
                                        onchange: move |e| {
                                            synonym_kind_filter.set(e.value());
                                            fetch_synonyms(true);
                                        },
                                        option { value: "", "全部" }
                                        option { value: "relation", "关系类型" }
                                        option { value: "class", "实体类" }
                                    }
                                }
                            }
                            if synonyms_loading() {
                                Loading { size: "md" }
                            } else if synonyms.is_empty() {
                                EmptyState { message: "暂无同义映射，新增后漂移原文将自动归并到目标规范词".to_string() }
                            } else {
                                div { class: "overflow-x-auto",
                                    table { class: "table hud-table table-fixed table-zebra table-pin-rows w-full",
                                        colgroup {
                                            col {}
                                            col { class: "w-[32px]" }
                                            col { class: "w-[112px]" }
                                            col {}
                                            col { class: "w-[140px]" }
                                            col { class: "w-[88px]" }
                                        }
                                        thead {
                                            tr {
                                                th { "漂移原文" }
                                                th { "" }
                                                th { "目标种类" }
                                                th { "目标规范词" }
                                                th { "创建时间" }
                                                th { "操作" }
                                            }
                                        }
                                        tbody {
                                            for s in synonyms().iter().cloned() {
                                                {
                                                    let delete_id = s.id.clone();
                                                    rsx! {
                                                        tr { key: "{s.id}",
                                                            td { span { class: "font-mono text-sm block truncate", title: "{s.raw_term}", "{s.raw_term}" } }
                                                            td { class: "opacity-50", "→" }
                                                            td { {kind_badge(s.target_kind)} }
                                                            td { span { class: "font-mono text-sm block truncate", title: "{s.target_key}", "{s.target_key}" } }
                                                            td { class: "whitespace-nowrap", "{format_datetime(s.created_at * 1000)}" }
                                                            td {
                                                                button { class: "btn hud-btn btn-error btn-sm",
                                                                    onclick: move |_| synonym_delete_id.set(Some(delete_id.clone())),
                                                                    "删除"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if synonyms_total() > 0 {
                                div { class: "flex items-center justify-between mt-3",
                                    span { class: "text-sm opacity-70",
                                        "共 {synonyms_total()} 条"
                                    }
                                    div { class: "flex gap-2",
                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: synonym_offset() == 0,
                                            onclick: move |_| {
                                                synonym_offset.set(synonym_offset().saturating_sub(PAGE_SIZE));
                                                fetch_synonyms(false);
                                            },
                                            "上一页"
                                        }
                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: synonym_offset() + PAGE_SIZE >= synonyms_total(),
                                            onclick: move |_| {
                                                synonym_offset.set(synonym_offset() + PAGE_SIZE);
                                                fetch_synonyms(false);
                                            },
                                            "下一页"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ==================== 实体类 新增/编辑 Modal ====================
            Modal {
                title: if class_edit_id().is_some() { "编辑实体类".to_string() } else { "新增实体类".to_string() },
                show: show_class_modal(),
                on_close: move |_| show_class_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_class_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: class_saving(),
                        onclick: move |_| handle_save_class(),
                        if class_saving() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "规范词 key *" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{class_form_key}",
                            disabled: class_edit_id().is_some(),
                            oninput: move |e| class_form_key.set(e.value()),
                            placeholder: "小写 snake_case，如：customer_profile"
                        }
                        if class_edit_id().is_some() {
                            span { class: "label-text-alt opacity-60", "创建后不可修改，如需换词请退役后新建" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "展示名 *" } }
                        input { class: "input input-bordered hud-input w-full",
                            value: "{class_form_name}",
                            oninput: move |e| class_form_name.set(e.value()),
                            placeholder: "如：客户档案"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "语义描述" } }
                        textarea { class: "textarea textarea-bordered hud-input w-full",
                            rows: "3",
                            value: "{class_form_desc}",
                            oninput: move |e| class_form_desc.set(e.value()),
                            placeholder: "该实体类的语义描述（会随词表注入提示词，帮助模型理解）"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "必填属性清单" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{class_form_fields}",
                            oninput: move |e| class_form_fields.set(e.value()),
                            placeholder: "逗号分隔，如：name, contact；留空 = 无必填属性"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "父类（继承）" } }
                        select { class: "select select-bordered hud-select w-full",
                            value: "{class_form_parent}",
                            onchange: move |e| class_form_parent.set(e.value()),
                            option { value: "", "无（作为根类）" }
                            if let Some(s) = skeleton() {
                                for c in s.classes.iter() {
                                    option { value: "{c.term_key}", "{c.display_name}（{c.term_key}）" }
                                }
                            } else {
                                for c in classes().iter() {
                                    option { value: "{c.term_key}", "{c.display_name}（{c.term_key}）" }
                                }
                            }
                        }
                        span { class: "label-text-alt opacity-60", "父类必须存在且启用，禁止自指与成环；子类自动继承祖先的骨架约束" }
                    }
                }
            }

            // ==================== 关系类型 新增/编辑 Modal ====================
            Modal {
                title: if relation_edit_id().is_some() { "编辑关系类型".to_string() } else { "新增关系类型".to_string() },
                show: show_relation_modal(),
                on_close: move |_| show_relation_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_relation_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: relation_saving(),
                        onclick: move |_| handle_save_relation(),
                        if relation_saving() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "规范词 key *" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{relation_form_key}",
                            disabled: relation_edit_id().is_some(),
                            oninput: move |e| relation_form_key.set(e.value()),
                            placeholder: "小写 snake_case，如：belongs_to"
                        }
                        if relation_edit_id().is_some() {
                            span { class: "label-text-alt opacity-60", "创建后不可修改，如需换词请退役后新建" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "展示名 *" } }
                        input { class: "input input-bordered hud-input w-full",
                            value: "{relation_form_name}",
                            oninput: move |e| relation_form_name.set(e.value()),
                            placeholder: "如：归属"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "语义描述" } }
                        textarea { class: "textarea textarea-bordered hud-input w-full",
                            rows: "3",
                            value: "{relation_form_desc}",
                            oninput: move |e| relation_form_desc.set(e.value()),
                            placeholder: "该关系的语义描述（会随词表注入提示词，帮助模型理解）"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "边权重基数" } }
                        input { class: "input input-bordered hud-input w-full",
                            r#type: "number",
                            step: "0.1",
                            min: "0",
                            value: "{relation_form_weight}",
                            oninput: move |e| relation_form_weight.set(e.value()),
                            placeholder: "留空 = 默认 1.0"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "反向关系 key" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{relation_form_inverse}",
                            oninput: move |e| relation_form_inverse.set(e.value()),
                            placeholder: "如 belongs_to 的反向词 owns；留空 = 无对称反向词"
                        }
                    }
                }
            }

            // ==================== 骨架三元组 新增/编辑 Modal ====================
            Modal {
                title: if triple_edit_id().is_some() { "编辑骨架三元组".to_string() } else { "新增骨架三元组".to_string() },
                show: show_triple_modal(),
                on_close: move |_| show_triple_modal.set(false),
                width_class: Some("max-w-xl".to_string()),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_triple_modal.set(false), "取消" }
                    button {
                        class: "btn hud-btn btn-primary",
                        disabled: triple_saving(),
                        onclick: move |_| handle_save_triple(),
                        if triple_saving() { span { class: "loading loading-spinner loading-xs" } }
                        "保存"
                    }
                },
                div { class: "space-y-4",
                    p { class: "text-xs opacity-60",
                        "三元组表达「主语类 -关系词-> 宾语类」的骨架约束：某关系无任何三元组 = 不约束；单侧留空时用 * 通配；子类沿继承链自动匹配父类约束。"
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "主语类" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{triple_form_subject}",
                            list: "skeleton-class-options",
                            oninput: move |e| triple_form_subject.set(e.value()),
                            placeholder: "实体类 term_key，或 * 表示不限"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "关系词" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{triple_form_relation}",
                            list: "skeleton-relation-options",
                            oninput: move |e| triple_form_relation.set(e.value()),
                            placeholder: "关系类型 term_key"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "宾语类" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{triple_form_object}",
                            list: "skeleton-class-options",
                            oninput: move |e| triple_form_object.set(e.value()),
                            placeholder: "实体类 term_key，或 * 表示不限"
                        }
                    }
                    div { class: "grid grid-cols-2 gap-4",
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "基数" } }
                            select { class: "select select-bordered hud-select w-full",
                                value: "{triple_form_cardinality}",
                                onchange: move |e| triple_form_cardinality.set(e.value()),
                                option { value: "many", "many（多值）" }
                                option { value: "single", "single（单值）" }
                            }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "是否必填" } }
                            label { class: "label cursor-pointer justify-start gap-3 rounded-lg border border-base-300 px-3 py-2",
                                input { r#type: "checkbox", class: "checkbox checkbox-sm",
                                    checked: triple_form_required(),
                                    onchange: move |e| triple_form_required.set(e.checked()),
                                }
                                span { class: "text-sm", "该关系为必填约束" }
                            }
                        }
                    }
                    datalist { id: "skeleton-class-options",
                        option { value: "{WILDCARD_CLASS}" }
                        if let Some(s) = skeleton() {
                            for c in s.classes.iter() {
                                option { value: "{c.term_key}" }
                            }
                        } else {
                            for c in classes().iter() {
                                option { value: "{c.term_key}" }
                            }
                        }
                    }
                    datalist { id: "skeleton-relation-options",
                        if let Some(s) = skeleton() {
                            for r in s.relation_types.iter() {
                                option { value: "{r.term_key}" }
                            }
                        } else {
                            for r in relations().iter() {
                                option { value: "{r.term_key}" }
                            }
                        }
                    }
                }
            }

            // ==================== 退役骨架三元组 确认 Modal ====================
            Modal {
                title: "退役骨架三元组".to_string(),
                show: triple_retire_id().is_some(),
                on_close: move |_| triple_retire_id.set(None),
                width_class: Some("max-w-md".to_string()),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| triple_retire_id.set(None), "取消" }
                    button { class: "btn hud-btn btn-error", onclick: move |_| handle_retire_triple(), "确认退役" }
                },
                p { class: "text-sm", "退役后该三元组不再参与骨架约束（软删除，可重新创建）。确认继续？" }
            }

            // ==================== 属性定义 新增/编辑 Modal ====================
            Modal {
                title: if property_edit_id().is_some() { "编辑属性定义".to_string() } else { "新增属性定义".to_string() },
                show: show_property_modal(),
                on_close: move |_| show_property_modal.set(false),
                width_class: Some("max-w-2xl".to_string()),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_property_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: property_saving(),
                        onclick: move |_| handle_save_property(),
                        if property_saving() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "规范词 key *" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{property_form_key}",
                            disabled: property_edit_id().is_some(),
                            oninput: move |e| property_form_key.set(e.value()),
                            placeholder: "小写 snake_case，如：confidence"
                        }
                        if property_edit_id().is_some() {
                            span { class: "label-text-alt opacity-60", "创建后不可修改，如需换词请退役后新建" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "展示名 *" } }
                        input { class: "input input-bordered hud-input w-full",
                            value: "{property_form_name}",
                            oninput: move |e| property_form_name.set(e.value()),
                            placeholder: "如：置信度"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "语义描述" } }
                        textarea { class: "textarea textarea-bordered hud-input w-full",
                            rows: "2",
                            value: "{property_form_desc}",
                            oninput: move |e| property_form_desc.set(e.value()),
                            placeholder: "该属性指什么、何时使用（会随词表注入提示词，帮助模型判断）"
                        }
                    }
                    div { class: "grid grid-cols-2 gap-4",
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "值类型" } }
                            select { class: "select select-bordered hud-input w-full",
                                value: "{property_form_data_type}",
                                onchange: move |e| property_form_data_type.set(e.value()),
                                option { value: "string", "string（文本）" }
                                option { value: "number", "number（数值）" }
                                option { value: "boolean", "boolean（布尔）" }
                                option { value: "date", "date（日期）" }
                                option { value: "reference", "reference（引用）" }
                                option { value: "enum", "enum（枚举）" }
                            }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "基数" } }
                            select { class: "select select-bordered hud-input w-full",
                                value: "{property_form_cardinality}",
                                onchange: move |e| property_form_cardinality.set(e.value()),
                                option { value: "single", "single（单值）" }
                                option { value: "many", "many（多值）" }
                            }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "允许挂载的载体" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{property_form_applies_to}",
                            oninput: move |e| property_form_applies_to.set(e.value()),
                            placeholder: "node, relation；留空 = 节点与边皆可"
                        }
                        span { class: "label-text-alt opacity-60", "作用域仅限知识图谱域内（节点 / 边），不扩展到 agent / task / project" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "适用实体类" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{property_form_domain}",
                            oninput: move |e| property_form_domain.set(e.value()),
                            placeholder: "逗号分隔的实体类 key；留空 = 不限实体类"
                        }
                    }
                    div { class: "border border-base-300 rounded-box p-3 space-y-2",
                        div { class: "label", span { class: "label-text font-medium", "取值约束（可全留空 = 无约束）" } }
                        div { class: "grid grid-cols-3 gap-2",
                            div { class: "form-control w-full",
                                input { class: "input input-bordered hud-input w-full",
                                    r#type: "number",
                                    step: "any",
                                    value: "{property_form_min}",
                                    oninput: move |e| property_form_min.set(e.value()),
                                    placeholder: "最小"
                                }
                            }
                            div { class: "form-control w-full",
                                input { class: "input input-bordered hud-input w-full",
                                    r#type: "number",
                                    step: "any",
                                    value: "{property_form_max}",
                                    oninput: move |e| property_form_max.set(e.value()),
                                    placeholder: "最大"
                                }
                            }
                            div { class: "form-control w-full",
                                input { class: "input input-bordered hud-input w-full",
                                    value: "{property_form_unit}",
                                    oninput: move |e| property_form_unit.set(e.value()),
                                    placeholder: "单位（仅数值型）"
                                }
                            }
                        }
                        div { class: "form-control w-full",
                            input { class: "input input-bordered hud-input w-full font-mono",
                                value: "{property_form_enum}",
                                oninput: move |e| property_form_enum.set(e.value()),
                                placeholder: "枚举白名单，逗号分隔，如：high, medium, low"
                            }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input { class: "checkbox",
                                r#type: "checkbox",
                                checked: property_form_required(),
                                onchange: move |e| property_form_required.set(e.checked()),
                            }
                            span { class: "label-text font-medium", "必填属性" }
                        }
                    }
                }
            }

            // ==================== 同义映射 新增 Modal ====================
            Modal {
                title: "新增同义映射".to_string(),
                show: show_synonym_modal(),
                on_close: move |_| show_synonym_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_synonym_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: synonym_saving(),
                        onclick: move |_| handle_save_synonym(),
                        if synonym_saving() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "漂移原文 *" } }
                        input { class: "input input-bordered hud-input w-full",
                            value: "{synonym_form_raw}",
                            oninput: move |e| synonym_form_raw.set(e.value()),
                            placeholder: "图谱中出现的非规范说法，入库前自动 trim + 小写"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "目标词条种类" } }
                        select { class: "select select-bordered hud-input w-full",
                            value: "{synonym_form_kind}",
                            onchange: move |e| synonym_form_kind.set(e.value()),
                            option { value: "relation", "关系类型" }
                            option { value: "class", "实体类" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "目标规范词 key *" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{synonym_form_target_key}",
                            oninput: move |e| synonym_form_target_key.set(e.value()),
                            placeholder: "必须存在于对应词表，否则视为无效映射"
                        }
                    }
                }
            }

            // ==================== 预置词表同步 Modal ====================
            Modal {
                title: "同步预置词表".to_string(),
                show: show_sync_modal(),
                on_close: move |_| show_sync_modal.set(false),
                width_class: Some("max-w-3xl".to_string()),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_sync_modal.set(false), "关闭" }
                    button { class: "btn hud-btn btn-primary",
                        disabled: syncing() || sync_preview_loading() || sync_result().is_some(),
                        onclick: move |_| {
                            // 覆盖式同步为破坏性动作：首点置确认态，再点才执行；仅补缺直接执行
                            if sync_strategy() == PresetOntologySyncStrategy::Overwrite
                                && !sync_confirm_pending()
                            {
                                sync_confirm_pending.set(true);
                            } else {
                                handle_sync();
                            }
                        },
                        if syncing() {
                            "同步中..."
                        } else if sync_strategy() == PresetOntologySyncStrategy::Overwrite
                            && sync_confirm_pending()
                        {
                            "确认覆盖同步？"
                        } else {
                            "执行同步"
                        }
                    }
                },
                if sync_preview_loading() {
                    Loading { size: "md" }
                } else if let Some(p) = sync_preview() {
                    {
                        // 影响面文案在 rsx 外计算：嵌套 if 进 format! 会破坏 rsx 解析（沿 skills 同步 Modal 同款先例）
                        let overwrite = sync_strategy() == PresetOntologySyncStrategy::Overwrite;
                        let strategy_hint = if overwrite {
                            format!(
                                "预置共 {} 条 · 覆盖式：已存在词条将按 seed 覆写（展示名/描述/方向等），退役词条跳过，自拟词不受影响",
                                p.items.len()
                            )
                        } else {
                            format!("预置共 {} 条 · 仅补缺，不覆盖已有修改", p.items.len())
                        };
                        rsx! {
                            div { class: "space-y-4",
                                // 策略选择（二选一，仿 skills 同步 Modal 先例）
                                div { class: "border border-base-300 rounded-box bg-base-200 overflow-hidden",
                                    div { class: "grid gap-2 p-3",
                                        div {
                                            class: if sync_strategy() == PresetOntologySyncStrategy::OnlyMissing {
                                                "card cursor-pointer border-2 border-primary bg-base-200 transition-colors"
                                            } else {
                                                "card cursor-pointer border border-base-300 bg-base-200 transition-colors"
                                            },
                                            onclick: move |_| {
                                                sync_strategy.set(PresetOntologySyncStrategy::OnlyMissing);
                                                sync_confirm_pending.set(false);
                                            },
                                            div { class: "card-body p-3",
                                                div { class: "font-semibold", "1 · 仅补缺（安全）" }
                                                div { class: "text-xs text-base-content/70",
                                                    "已存在的词条跳过，不覆盖管理页修改；只把 seed 中缺失的词条加入词表"
                                                }
                                            }
                                        }
                                        div {
                                            class: if sync_strategy() == PresetOntologySyncStrategy::Overwrite {
                                                "card cursor-pointer border-2 border-primary bg-base-200 transition-colors"
                                            } else {
                                                "card cursor-pointer border border-base-300 bg-base-200 transition-colors"
                                            },
                                            onclick: move |_| {
                                                sync_strategy.set(PresetOntologySyncStrategy::Overwrite);
                                                sync_confirm_pending.set(false);
                                            },
                                            div { class: "card-body p-3",
                                                div { class: "font-semibold", "2 · 覆盖式同步" }
                                                div { class: "text-xs text-base-content/70",
                                                    "已存在词条将被 seed 值覆写（展示名/描述/方向等）；退役词条跳过，自拟词不受影响"
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "flex flex-wrap items-center gap-3 text-sm",
                                    span { class: "badge hud-badge badge-success", "将新建 {p.missing_count}" }
                                    span { class: "badge hud-badge badge-ghost", "已存在 {p.existing_count}" }
                                    if overwrite {
                                        span { class: "badge hud-badge badge-warning", "将覆写 {p.overwrite_count}" }
                                    }
                                    span { class: "badge hud-badge badge-ghost", "退役行 {p.retired_count}" }
                                    span { class: "opacity-70", "{strategy_hint}" }
                                }
                                if let Some(r) = sync_result() {
                                    div { class: "alert alert-success text-sm",
                                        "同步完成：新建 {r.created} 条，更新 {r.updated} 条，跳过 {r.skipped} 条（预置共 {r.total} 条）"
                                    }
                                }
                                div { class: "max-h-80 overflow-y-auto space-y-1",
                                    for item in p.items {
                                        div { class: "flex flex-wrap items-center gap-3 rounded-lg border border-base-300 px-3 py-2",
                                            {kind_badge(item.kind)}
                                            span { class: "font-mono text-sm", "{item.term_key}" }
                                            span { class: "text-sm flex-1 truncate", "{item.display_name}" }
                                            if item.retired {
                                                span { class: "badge badge-ghost badge-sm", "退役·跳过" }
                                            } else if !item.exists {
                                                span { class: "badge badge-success badge-sm", "将新建" }
                                            } else if overwrite && !item.diff_fields.is_empty() {
                                                span { class: "badge badge-warning badge-sm", "将覆写 {item.diff_fields.len()} 字段" }
                                            } else {
                                                span { class: "badge badge-ghost badge-sm", "已存在·跳过" }
                                            }
                                            div { class: "w-full text-xs opacity-60 truncate", "{item.description}" }
                                            if overwrite && item.exists && !item.retired && !item.diff_fields.is_empty() {
                                                div { class: "w-full text-xs text-warning", "覆写字段：{item.diff_fields.join(\"、\")}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ==================== 漂移词下钻 Modal ====================
            Modal {
                title: format!("词条「{}」下钻", drill_term().unwrap_or_default()),
                show: drill_term().is_some(),
                on_close: move |_| drill_term.set(None),
                width_class: Some("max-w-3xl".to_string()),
                if drill_loading() {
                    Loading { size: "md" }
                } else {
                    div { class: "space-y-6",
                        div {
                            h4 { class: "font-semibold mb-2", "关系（边）明细" }
                            if drill_relations().is_empty() {
                                EmptyState { icon: Some("🔗".to_string()), message: "无相关关系边".to_string() }
                            } else {
                                div { class: "overflow-x-auto max-h-64 overflow-y-auto",
                                    table { class: "table hud-table table-zebra",
                                        thead {
                                            tr {
                                                th { "源节点" }
                                                th { "" }
                                                th { "目标节点" }
                                                th { "Agent" }
                                                th { "时间" }
                                            }
                                        }
                                        tbody {
                                            for e in drill_relations().iter().cloned() {
                                                tr { key: "{e.id}",
                                                    td { "{e.source_name}" }
                                                    td { class: "opacity-50", "→" }
                                                    td { "{e.target_name}" }
                                                    td {
                                                        div { class: "font-mono text-xs truncate",
                                                            title: "{e.agent_id}", "{e.agent_id}"
                                                        }
                                                    }
                                                    td { class: "whitespace-nowrap", "{format_datetime(e.created_at * 1000)}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div {
                            h4 { class: "font-semibold mb-2", "节点明细" }
                            if drill_classes().is_empty() {
                                EmptyState { icon: Some("🧩".to_string()), message: "无相关节点".to_string() }
                            } else {
                                div { class: "overflow-x-auto max-h-64 overflow-y-auto",
                                    table { class: "table hud-table table-zebra",
                                        thead {
                                            tr {
                                                th { "节点名称" }
                                                th { "Agent" }
                                                th { "时间" }
                                            }
                                        }
                                        tbody {
                                            for c in drill_classes().iter().cloned() {
                                                tr { key: "{c.id}",
                                                    td { "{c.name}" }
                                                    td {
                                                        div { class: "font-mono text-xs truncate",
                                                            title: "{c.agent_id}", "{c.agent_id}"
                                                        }
                                                    }
                                                    td { class: "whitespace-nowrap", "{format_datetime(c.created_at * 1000)}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ==================== 确认框 ====================
            ConfirmDialog {
                show: class_retire_id().is_some(),
                title: "确认退役".to_string(),
                message: "退役后该实体类不再参与解析（历史存量引用仍可解释）。确定退役？".to_string(),
                confirm_text: Some("退役".to_string()),
                confirm_class: Some("btn hud-btn btn-error".to_string()),
                on_confirm: move |_| handle_retire_class(),
                on_cancel: move |_| class_retire_id.set(None),
            }
            ConfirmDialog {
                show: relation_retire_id().is_some(),
                title: "确认退役".to_string(),
                message: "退役后该关系类型不再参与解析（历史存量边仍可解释）。确定退役？".to_string(),
                confirm_text: Some("退役".to_string()),
                confirm_class: Some("btn hud-btn btn-error".to_string()),
                on_confirm: move |_| handle_retire_relation(),
                on_cancel: move |_| relation_retire_id.set(None),
            }
            ConfirmDialog {
                show: property_retire_id().is_some(),
                title: "确认退役".to_string(),
                message: "退役后该属性不再参与注入与校验（历史存量属性值仍可解释）。确定退役？".to_string(),
                confirm_text: Some("退役".to_string()),
                confirm_class: Some("btn hud-btn btn-error".to_string()),
                on_confirm: move |_| handle_retire_property(),
                on_cancel: move |_| property_retire_id.set(None),
            }
            ConfirmDialog {
                show: synonym_delete_id().is_some(),
                title: "确认删除".to_string(),
                message: "删除后相关漂移原文将不再归并，下一次解析即回落为漂移词。确定删除？".to_string(),
                confirm_text: Some("删除".to_string()),
                confirm_class: Some("btn hud-btn btn-error".to_string()),
                on_confirm: move |_| handle_delete_synonym(),
                on_cancel: move |_| synonym_delete_id.set(None),
            }
        }
    }
}
