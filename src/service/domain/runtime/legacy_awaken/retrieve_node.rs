//! 检索节点（机械）：按意图关键词做长期知识图谱召回 + 少量短期记忆补充 + 技能召回
//!
//! 检索动作本身不调模型：知识走 FTS/向量混合搜索，方法技能走 DAL 标签查询，
//! 图谱结构走一跳 SQL 扩展。可选的「小脑精选」是唯一会发模型调用的一环
//! （开关关闭时完全不发）。输入 = `bus[Intent]` + `deps.retrieval`（visited 差集），
//! 输出 = `bus[Knowledge]` 的 [`RetrievalPack`]。空词 / 零命中都是合法结果
//! （照常 Done）：检索是增强，不是闸门。
//!
//! ⚠️ **长期知识与短期记忆分两次查**：本节点的首要目标是长期知识图谱和技能。
//! DAL 的 `filters.limit` 是「实体总数」口径，混在 `MemoryType::All` 里查会让
//! 短期记忆和知识节点抢同一个名额池。分开后两路各有独立预算、独立失败降级。
//!
//! 两路都是「宽召回 → 小脑精选 → 分层渲染」，但 `keep` 各自独立：长期知识
//! 15 → 5，短期记忆 20 → 3。**精选命中的给全文，其余候选不再丢弃**，改成
//! 「元信息 + 摘要片段」的 `[索引]` 行：信息量比只留 Top-K 大，token 却比
//! 全量灌正文低得多。两类行都带 id ⇒ Agent 可 `query_memory(ids)` 反查详情，
//! 或用 `search_memory(seed_node_ids=[...])` 沿关系边展开某个知识节点。
//! 小脑未配置 / 开关关闭 / 决策降级时，按召回序取前 Top-K 作全文（老路径）。

use super::context::{AwakenDeps, RetrievalPack, read_bus};
use crate::models::memory::{Memory, MemoryIndexTier, MemoryPo};
use crate::models::skill::SkillPo;
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use crate::service::dal::memory::TraversalStrategy;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use crate::service::domain::runtime::RuntimeMemory;
use crate::service::domain::runtime::types::IntentAnalysis;
use async_trait::async_trait;
use common::enums::MemoryType;
use common::error::Result;
use std::sync::Arc;

/// 节点 id：蓝图声明、runner 注册、轨迹指纹三处共用的同一真相
pub(crate) const NODE_RETRIEVE: &str = "retrieve";

/// 渲染预算：进 prompt 的知识节点条数（机械 Top-K）
///
/// 这是**消费端**的宽度，与召回宽度解耦：多召回、少渲染，中间那道筛
/// 由精选（开关打开时）或排序（关闭时）完成。
pub(crate) const KNOWLEDGE_TOP_K: usize = 5;

/// 短期记忆**全文行**的条数上限（精选的 keep，也是全文渲染上限）
///
/// 本节点的首要职责是「长期知识图谱 + 方法技能」；近期短期记忆已经由
/// 【历史对话】索引块按预算铺在 base 里。这里只留少量「语义命中、但可能
/// 落在索引窗口之外」的补充，全文硬顶 3 条，既不与长期知识抢候选位，
/// 也避免把整段历史重新灌回 prompt。
/// ⚠️ 未入选的候选不丢弃，改成 `[索引]` 行（元信息 + 64 字片段），
/// 所以「进 prompt 的短期记忆总行数」会大于 3；这里限的是**带全文的行**。
pub(crate) const SHORT_TERM_TOP_K: usize = 3;

/// 短期记忆**宽召回**的候选池大小：查 20 条，再精选 / 取前 3 条
///
/// 与长期知识一样走「多召回、少渲染」：短期记忆条目短、FTS/向量查询便宜，
/// 但排序靠词面相关度，直接取前 3 条容易把真正相关的那条漏掉。多给一个
/// 候选池让小脑有取舍空间；开关未打开时这段只是「按召回序取前 3」，
/// 与老路径逐字节等价。
const SHORT_TERM_RECALL_WIDTH: usize = 20;

/// `[索引]` 行里摘要片段的字符数：只留开头一小段，够判断「值不值得反查」即可
///
/// 不另设索引行条数上限——两路候选池本身有界（15 / 20），最多 35 行，
/// 每行元信息 + 64 字，比给它们全灌正文便宜一个量级。
const RECALL_INDEX_SNIPPET_CHARS: usize = 64;

/// 召回宽度相对渲染预算的倍数
///
/// 召回是「便宜但漏一次就没了」的一侧：一次 FTS + 一次向量查询的成本远低于
/// 漏召回的代价，所以候选池按渲染预算的整数倍放宽，再由精选/排序收回 5 条。
///
/// 3 倍（= 15 条）是当前取值：足够让精选有取舍空间，又不至于让一次 Pick 的
/// 选项列表长到模型只看前几个。调大这一个是单点操作，不需要动结构。
const RECALL_WIDTH_FACTOR: usize = 3;

/// 关系边命中条数上限
///
/// 与知识节点**分开**截断：边是结构信息（「谁和谁什么关系」），
/// 和节点抢同一个名额会让图谱在提示里被挤掉。
const RELATION_TOP_K: usize = 4;

/// 方法技能条数上限（渲染预算 + 精选 keep，两处共用的同一真相）
pub(crate) const METHOD_SKILLS_TOP_K: usize = 2;

/// 图谱扩展的种子节点数（从长期知识候选池里取前 N 个知识节点）
///
/// ⚠️ 与**召回宽度**对齐，而不是只取渲染预算 5 个：主召回拆成
/// `MemoryType::KnowledgeNode` 后不再走 DAL 的 FTS 关系检索，关系边全部由
/// 一跳图谱扩展提供。候选池里的每个节点都值得当一次种子（只是 SQL 的
/// `IN` 条件，批量一趟查完），漏掉一个就等于那部分图谱结构永远进不了上下文。
const GRAPH_SEED_TOP_K: usize = KNOWLEDGE_TOP_K * RECALL_WIDTH_FACTOR;

/// 图谱扩展深度：固定一跳
///
/// 再深一层会把整片图谱拖进来，词面命中这个「相关性的锚」就被稀释了。
const GRAPH_MAX_DEPTH: i32 = 1;

/// 图谱扩展宽度：每个种子最多展开多少邻居
const GRAPH_MAX_BREADTH: i32 = 6;

/// thinking_method 技能的 tag 约定值（与 seed 技能、沉淀登记共用同一真相）
pub(crate) const METHOD_SKILL_TAG: &str = "thinking_method";

/// visited 差集：候选词 trim、去重后剔除已检索过的词（保持原顺序）
///
/// 抽成纯函数是为了单测，以及 discover 节点对新词做同样的去重。
pub(crate) fn diff_terms(candidates: &[String], queried: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::with_capacity(candidates.len());
    for term in candidates {
        let term = term.trim();
        if term.is_empty()
            || queried.iter().any(|q| q.as_str() == term)
            || seen.iter().any(|s| s.as_str() == term)
        {
            continue;
        }
        seen.push(term.to_string());
    }
    seen
}

/// 主召回：只查**长期知识节点**，独立占满 `width` 个候选位
///
/// ⚠️ 不能用 `MemoryType::All`：DAL 的 `filters.limit` 按实体总数截断，
/// 短期记忆一旦排在前面就会把知识节点的候选位吃光。关系边不在这路里，
/// 由 [`expand_graph`] 的一跳扩展补齐。
fn long_term_search(keyword: &str, width: usize) -> MemorySearch {
    MemorySearch {
        keyword: Some(keyword.to_string()),
        // `top_k` 只管向量路；FTS 路的条数由 `filters.limit` 决定（DAO 缺省 50）。
        // 两个字段必须同时给，否则「宽召回 N 条」的语义会脱钩成 N + 50。
        top_k: Some(width as i32),
        filters: MemoryQuery {
            memory_type: Some(MemoryType::KnowledgeNode),
            limit: Some(width),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// 补充召回：单独查一次**短期记忆**，宽召回 [`SHORT_TERM_RECALL_WIDTH`] 条
///
/// 召回宽度 ≠ 渲染条数：查回来的候选整批进小脑精选，由精选按相关性收回
/// [`SHORT_TERM_TOP_K`] 条。不参与长期知识的图谱扩展（种子只认知识节点）。
fn short_term_search(keyword: &str, width: usize) -> MemorySearch {
    MemorySearch {
        keyword: Some(keyword.to_string()),
        // 同长期知识：`top_k` 管向量路、`filters.limit` 管 FTS 路，必须同时给，
        // 否则候选池会脱钩成「width + DAO 缺省 50」。
        top_k: Some(width as i32),
        filters: MemoryQuery {
            memory_type: Some(MemoryType::ShortTerm),
            limit: Some(width),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// 单条召回行：`full=true` → 元信息 + 全文；否则 `[索引]` + 元信息 + 摘要片段
///
/// 短期记忆与长期知识节点**都走 Models 的统一打包**
/// （`to_prompt_summary` / `to_index_prompt`）：元信息、正文口径、分段策略全部
/// 由各自的 PO 负责，本节点只决定「给全文还是给摘要片段」以及行前缀。
/// 元信息格式因此与【历史对话】索引块同源，不会再出现第二份拼串实现。
///
/// ⚠️ 两档都带 id：短期记忆走 `query_memory(ids)` 反查，知识节点走
/// `search_memory(seed_node_ids=[...], traversal_depth=1)` 查边。
fn recall_line(m: &Memory, full: bool) -> Option<String> {
    let line = if full {
        m.to_prompt_summary()?
    } else {
        m.to_index_prompt(MemoryIndexTier::OnePart, RECALL_INDEX_SNIPPET_CHARS)?
    };
    Some(match (&m.po, full) {
        // to_prompt_summary 自带 [id]，补一个类型标签让两层视觉上可区分
        (MemoryPo::ShortTerm(_), true) => format!("[短期记忆] {line}"),
        (_, true) => line,
        _ => format!("[索引] {line}"),
    })
}

/// 命中集合 → prompt 行（精选全文 + 其余候选的索引行 + 关系边）
///
/// `selected` 是小脑精选 / 召回序留下的条目，给全文；`rest` 是宽召回里
/// **没被选中**的候选，只给元信息 + 摘要片段。以前 `rest` 被整批丢弃，
/// Agent 既看不到「还有这些相关条目」，也就无从判断要不要按 id 反查。
///
/// `neighbors` 只用于**边两端的取名**，不渲染正文：扩展到一跳邻居的意义是
/// 「看清谁和谁有关系」，把邻居描述也塞进来就又回到噪声问题了。
///
/// ⚠️ 主召回已限定 `MemoryType::KnowledgeNode`，DAL 的 FTS 关系检索不再触发，
/// 边**全部由 [`expand_graph`] 的一跳邻域提供**；种子覆盖 `selected + rest` 两层。
/// 结果批里万一夹带 `Relation`，也按 id 去重一并入池（容错），再按结构信号
/// 排序截断——这一步不调模型。边的两端是否「命中」同样按 `selected + rest`
/// 一起判：索引行里的节点能给边取名，不该被误判成远端缺席。
///
/// ⚠️ 返回的行**不带 `- ` 前缀**：由 `render_retrieved_knowledge_section`
/// 统一加，否则会出现 `- - xxx` 的双重项目符号。
fn build_knowledge_lines(
    selected: &[Memory],
    rest: &[Memory],
    neighbors: &[Memory],
    relations: &[Memory],
) -> Vec<String> {
    let mut node_names: std::collections::HashMap<&str, &str> = selected
        .iter()
        .chain(rest.iter())
        .filter_map(|m| match &m.po {
            MemoryPo::KnowledgeNode(k) => Some((k.id.as_str(), k.node_name.as_str())),
            _ => None,
        })
        .collect();
    for m in neighbors {
        if let MemoryPo::KnowledgeNode(k) = &m.po {
            node_names
                .entry(k.id.as_str())
                .or_insert(k.node_name.as_str());
        }
    }

    let mut node_lines: Vec<String> = Vec::new();
    let mut short_term_lines: Vec<String> = Vec::new();
    let mut relation_lines: Vec<String> = Vec::new();

    for m in selected {
        match &m.po {
            MemoryPo::KnowledgeNode(_) => {
                if let Some(line) = recall_line(m, true) {
                    node_lines.push(line);
                }
            }
            MemoryPo::ShortTerm(_) => {
                if let Some(line) = recall_line(m, true) {
                    short_term_lines.push(line);
                }
            }
            _ => {}
        }
    }
    // 边池 = 检索带出的边 + 图谱扩展带出的边（同一 id 只留一次）
    let hit_ids: std::collections::HashSet<&str> = selected
        .iter()
        .chain(rest.iter())
        .filter(|m| !matches!(m.po, MemoryPo::Relation(_)))
        .filter_map(memory_id)
        .collect();
    let mut relation_pool: Vec<&Memory> = Vec::new();
    let mut seen_relation: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for m in selected.iter().chain(rest.iter()).chain(relations.iter()) {
        if let MemoryPo::Relation(r) = &m.po
            && seen_relation.insert(r.id.as_str())
        {
            relation_pool.push(m);
        }
    }

    // 结构排序（零成本，不调模型）：两端都命中的边信息量最高——
    // 「A 依赖 B」只有 A、B 都在本轮上下文里才闭环；远端没命中时
    // 那条边只能靠邻居取名，读者拿到的是半条信息。其次是边自身的 weight。
    relation_pool.sort_by(|a, b| {
        let key = |m: &Memory| match &m.po {
            MemoryPo::Relation(r) => (
                hit_ids.contains(r.source_node_id.as_str())
                    && hit_ids.contains(r.target_node_id.as_str()),
                r.weight.unwrap_or(0.0),
            ),
            _ => (false, 0.0),
        };
        let (both_a, weight_a) = key(a);
        let (both_b, weight_b) = key(b);
        both_b.cmp(&both_a).then_with(|| {
            weight_b
                .partial_cmp(&weight_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    for m in relation_pool.into_iter().take(RELATION_TOP_K) {
        if let MemoryPo::Relation(r) = &m.po {
            let (Some(src), Some(dst)) = (
                node_names.get(r.source_node_id.as_str()),
                node_names.get(r.target_node_id.as_str()),
            ) else {
                continue;
            };
            // ⚠️ 边是结构信息，同样要带 id：边自身 id 可 query_memory(ids) 反查，
            // 两端节点 id 可直接作 seed_node_ids 继续沿图展开，也能让 Agent 把
            // 这里的名字精确对上全文 / 索引行里的 [知识节点 kX]（同名节点消歧）。
            relation_lines.push(format!(
                "[关系 {}] {src}[{}] --{}--> {dst}[{}]",
                r.id, r.source_node_id, r.relation_type, r.target_node_id
            ));
        }
    }

    // 三份预算各自截断：全文短期记忆最多 3 条，既不吃掉 5 个长期知识名额，
    // 也不会因为知识节点多而整组消失；关系边仍单独占 4 个名额。
    node_lines.truncate(KNOWLEDGE_TOP_K);
    short_term_lines.truncate(SHORT_TERM_TOP_K);
    let mut lines = node_lines;
    lines.extend(short_term_lines);

    // 其余候选只给元信息 + 摘要片段：不丢掉它们，是为了让 Agent 知道
    // 「还有这些相关条目」并能按 id 精确反查，又不把整批正文灌进 prompt。
    for m in rest {
        if let Some(line) = recall_line(m, false) {
            lines.push(line);
        }
    }
    lines.extend(relation_lines);
    lines
}

/// 召回宽度（候选池大小），与渲染预算 [`KNOWLEDGE_TOP_K`] 成固定倍数
///
/// 与精选开关**无关**：精选只决定「从候选池里留下哪几条」，不该反过来决定
/// 「先从库里捞多少条」。旧实现在关闭精选时把候选池收到 5，于是同一个库里
/// 同一句话，开不开精选会看到两种排序结果；统一口径后关掉精选只是少了那道
/// 语义筛选，召回面的宽度不变。
pub(crate) fn recall_width() -> usize {
    KNOWLEDGE_TOP_K * RECALL_WIDTH_FACTOR
}

/// 精选产物：入选的给全文，其余候选留作 `[索引]` 行
///
/// ⚠️ `rest` 不能丢：宽召回的意义就是让 Agent 看到「还有哪些相关条目」，
/// 只把 Top-K 送进 prompt 等于把多查的那十几条白查了。
#[derive(Default)]
struct RerankOutcome {
    selected: Vec<Memory>,
    rest: Vec<Memory>,
}

/// 降级口径：按召回序前 `keep` 条入选，其余原序留作索引
fn split_by_recall_order(entities: Vec<Memory>, keep: usize) -> RerankOutcome {
    let mut iter = entities.into_iter();
    let selected = iter.by_ref().take(keep).collect();
    let rest = iter.collect();
    RerankOutcome { selected, rest }
}

/// 小脑精选：关闭 / 无大脑 / 降级 → 按召回序前 Top-K 条入选（老路径）
///
/// 精选是增强不是闸门：任何失败都退回机械切分，绝不把召回清空。
///
/// ⚠️ 关系边不参与精选：边的判据文本只有关系类型（`depends_on`），模型无从
/// 判断相关性，却要占掉一个选项位；而且精选只该决定「哪些实体进 prompt」，
/// 边由渲染侧按结构信号排序。边也不进 `rest`（索引行只收实体）。
///
/// `keep` = 这一路的渲染预算（长期 [`KNOWLEDGE_TOP_K`] / 短期 [`SHORT_TERM_TOP_K`]）：
/// 既传给排序形态作保留上限，也用于降级截断。两路共用一个函数但**不共用预算**，
/// 否则短期记忆精选后能塞回 5 条。
async fn rerank_if_enabled(
    deps: &AwakenDeps,
    question: &str,
    memories: Vec<Memory>,
    keep: usize,
) -> RerankOutcome {
    let (edges, entities): (Vec<Memory>, Vec<Memory>) = memories
        .into_iter()
        .partition(|m| matches!(m.po, MemoryPo::Relation(_)));

    let (ranked, rerank_scene): (Option<Vec<Memory>>, Option<&'static str>) =
        match deps.agent.brain.as_ref() {
            Some(brain) => (
                super::memory_relevance::rank_with_default_dao(
                    &deps.ctx, brain, question, &entities, keep,
                )
                .await,
                Some(super::memory_relevance::SCENE),
            ),
            _ => (None, None),
        };
    // 小脑留痕：精选是增强，没出结论时也要能看见「这次是靠老路径兜的」
    if let Some(scene) = rerank_scene {
        crate::pkg::think_trace::record_cerebellum(
            &deps.trace_id,
            crate::pkg::think_trace::ThinkTraceCerebellum {
                scene: scene.to_string(),
                decision: if ranked.is_some() {
                    "select"
                } else {
                    "fallback"
                }
                .to_string(),
                confidence: None,
                degraded: ranked.is_none().then(|| "no_usable_answer".to_string()),
                applied: ranked.is_some(),
            },
        );
    }

    let mut outcome = match ranked {
        None => split_by_recall_order(entities, keep),
        Some(kept) => {
            let kept: Vec<Memory> = kept.into_iter().take(keep).collect();
            let kept_ids: std::collections::HashSet<&str> =
                kept.iter().filter_map(memory_id).collect();
            let rest = entities
                .into_iter()
                .filter(|m| memory_id(m).is_none_or(|id| !kept_ids.contains(id)))
                .collect();
            RerankOutcome {
                selected: kept,
                rest,
            }
        }
    };
    // 边不占精选名额、也不进索引行，但仍随 selected 交给关系池渲染
    outcome.selected.extend(edges);
    outcome
}

/// 一跳图谱扩展的产物
#[derive(Default)]
struct GraphExpansion {
    /// 邻接节点：**只用于边取名**，不进 prompt 正文
    neighbors: Vec<Memory>,
    /// 关系边：命中节点与邻居之间的结构信息
    relations: Vec<Memory>,
}

/// 图谱扩展的种子：**精选 + 索引两层**的知识节点都要当种子。
///
/// ⚠️ 不能只传 `selected`：主召回拆成 `MemoryType::KnowledgeNode` 后，关系边
/// 全部由这一跳扩展提供，`[索引]` 行里的节点同样是「本轮命中的知识」。只拿
/// 5 条精选节点做种子，等于把 rest 那部分图谱结构整段丢掉。抽成纯函数是为了
/// 让这条不变量能被护栏测试锁住，而不是靠调用方记得传对参数。
fn graph_seeds(selected: &[Memory], rest: &[Memory]) -> Vec<String> {
    selected
        .iter()
        .chain(rest.iter())
        .filter_map(|m| match &m.po {
            MemoryPo::KnowledgeNode(k) => Some(k.id.clone()),
            _ => None,
        })
        .take(GRAPH_SEED_TOP_K)
        .collect()
}

/// 以关键词命中的知识节点为种子，拉一跳邻域
///
/// 关键词路只按词面召回，命中节点的关联边默认进不了结果批——而「边本身就是
/// 信息」（A 依赖 B、C 取代了 D），全靠检索正好撞上纯属运气。这里把种子交给
/// `traverse_graph` 做一次批量 SQL 扩展（成本极低），只取边与邻居名字。
///
/// 失败降级为空扩展：检索是增强，不是闸门。
async fn expand_graph(deps: &AwakenDeps, selected: &[Memory], rest: &[Memory]) -> GraphExpansion {
    let seeds = graph_seeds(selected, rest);
    if seeds.is_empty() {
        return GraphExpansion::default();
    }

    let expanded = match deps
        .domain
        .traverse_graph(
            deps.ctx.clone(),
            &seeds,
            GRAPH_MAX_DEPTH,
            GRAPH_MAX_BREADTH,
            TraversalStrategy::BreadthFirst,
            // 唤醒链路**硬传 false**：检索是增强而非决定条件，不为它引入行为变更
            // （退役节点不因这条链路被带回上下文）。
            false,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            crate::log_warn!(
                &deps.ctx,
                "legacy_awaken",
                "graph expansion degraded to keyword hits: seeds={}, error={}",
                seeds.len(),
                e
            );
            return GraphExpansion::default();
        }
    };

    // 命中集合同样覆盖两层：rest 里的节点不该被当成「新邻居」再收一遍
    let hit_ids: std::collections::HashSet<&str> = selected
        .iter()
        .chain(rest.iter())
        .filter_map(memory_id)
        .collect();
    let mut relations: Vec<Memory> = Vec::new();
    let mut neighbors: Vec<Memory> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in expanded {
        let Some(id) = memory_id(&m) else {
            continue;
        };
        // 种子节点本身已在命中原批里，不再重复收集
        if hit_ids.contains(id) || !seen.insert(id.to_string()) {
            continue;
        }
        match &m.po {
            MemoryPo::Relation(_) => relations.push(m),
            MemoryPo::KnowledgeNode(_) => neighbors.push(m),
            _ => {}
        }
    }

    crate::log_info!(
        &deps.ctx,
        "legacy_awaken",
        "graph expansion done, seeds={}, relations={}, neighbors={}",
        seeds.len(),
        relations.len(),
        neighbors.len()
    );
    GraphExpansion {
        neighbors,
        relations,
    }
}

/// 取记忆实体 id（PO 变体各自的 id 字段同名，但没有统一 trait）
pub(super) fn memory_id(m: &Memory) -> Option<&str> {
    match &m.po {
        MemoryPo::Trace(t) => Some(t.id.as_str()),
        MemoryPo::ShortTerm(st) => Some(st.id.as_str()),
        MemoryPo::KnowledgeNode(k) => Some(k.id.as_str()),
        MemoryPo::Relation(r) => Some(r.id.as_str()),
    }
}

/// 截取前 `max` 个字符（按字符而不是字节，避免把中文切坏）
pub(super) fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// 写总线：pack 统一 Full 深度（prepare / deliberate 都按全量消费）
fn write_pack(bus: &mut ThinkBus, pack: &RetrievalPack) -> Result<()> {
    let value = serde_json::to_value(pack)
        .map_err(|e| common::error::err!(Internal, "encode retrieval pack failed: {e}"))?;
    bus.insert(ContextKey::Knowledge, LoadDepth::Full, value);
    Ok(())
}

#[derive(Debug)]
pub struct RetrieveNode {
    deps: Arc<AwakenDeps>,
}

impl RetrieveNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for RetrieveNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_RETRIEVE)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Triage
    }

    fn purpose(&self) -> &str {
        "知识与方法召回：按意图新词做混合检索（机械，不调模型）"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![],
            // 只声明 Intent：queried_terms 走 deps.retrieval 状态槽位，
            // 不在总线契约里（它是链路级累计状态，不是节点间数据流）
            reads: vec![ContextKey::Intent],
            writes: vec![ContextKey::Knowledge],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;

        // 1. 读意图产物：分区缺失 / 解码失败按空结构继续（intent 节点降级路径的下游）
        let ia: IntentAnalysis =
            read_bus::<IntentAnalysis>(bus, ContextKey::Intent).unwrap_or_default();

        // 2. 快照后立刻放锁：MutexGuard 不跨 await（防死锁，照 BusinessProvider 的先例）
        let queried_snapshot = deps
            .retrieval
            .lock()
            .map(|s| s.queried_terms.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().queried_terms.clone());

        // 3. 候选词 = intent 关键词 + discover 回环写入的新词（design §3.2：retrieve
        //    的输入是「IntentAnalysis.key_terms **或 discover 新词**」）。
        //    discover 先写「新词 + 思考摘要」的过渡 pack，由本次 retrieve 覆写成
        //    完整产物（见 RetrievalPack 契约）；旧 pack 里已被检索过的词会被
        //    差集自然剔除。visited 差集为空 → 无新息不查库：写空 pack 短路
        //    （保留旧 pack 的思考摘要）
        let mut candidates = read_bus::<RetrievalPack>(bus, ContextKey::Knowledge)
            .ok()
            .map(|p| p.query_terms)
            .unwrap_or_default();
        candidates.extend(ia.key_terms.iter().cloned());
        let new_terms = diff_terms(&candidates, &queried_snapshot);
        if new_terms.is_empty() {
            let prior = read_bus::<RetrievalPack>(bus, ContextKey::Knowledge)
                .ok()
                .and_then(|p| p.prior_thinking_summary);
            let pack = RetrievalPack {
                query_terms: Vec::new(),
                knowledge_hits: Vec::new(),
                method_skills: Vec::new(),
                prior_thinking_summary: prior,
            };
            crate::log_info!(
                &deps.ctx,
                "legacy_awaken",
                "retrieve node short-circuit (no new terms), agent_id={}, key_terms={}",
                deps.agent.po.id,
                ia.key_terms.len()
            );
            write_pack(bus, &pack)?;
            return Ok(NodeOutcome::new(NodeSignal::Done));
        }

        // 4. 分两路召回：长期知识图谱（主）+ 短期记忆（少量补充）
        //
        // ⚠️ 两路各自独立 limit / 独立失败降级：一路查挂了不影响另一路。
        // 检索是增强，不是闸门；任何失败都退回空命中照常 Done。
        let keyword = new_terms.join(" ");
        let width = recall_width();

        // 4a. 长期知识节点：宽召回 → 精选（入选全文 / 其余索引）→ 一跳图谱扩展
        //     （主召回不再带关系边，图谱结构全部由这次扩展补上）
        let long_term = match deps
            .domain
            .search(deps.ctx.clone(), long_term_search(&keyword, width))
            .await
        {
            Ok(nodes) => rerank_if_enabled(deps, &keyword, nodes, KNOWLEDGE_TOP_K).await,
            Err(e) => {
                crate::log_error!(
                    &deps.ctx,
                    "legacy_awaken",
                    "long-term knowledge search degraded to empty: agent_id={}, error={}",
                    deps.agent.po.id,
                    e
                );
                RerankOutcome::default()
            }
        };
        // 种子必须覆盖 selected + rest：索引行里的知识节点同样要带出自己的边
        let graph = expand_graph(deps, &long_term.selected, &long_term.rest).await;

        // 4b. 短期记忆：独立宽召回 20 条 → 同形态精选（keep=3）。
        //     先按 PO 收一道口（防止 DAL 未来放宽类型语义时把别的实体混进
        //     候选池占选项位）；无小脑 / 开关关闭 / 决策降级时按召回序
        //     前 3 条入选，其余全部留作 [索引] 行。
        let short_term = match deps
            .domain
            .search(
                deps.ctx.clone(),
                short_term_search(&keyword, SHORT_TERM_RECALL_WIDTH),
            )
            .await
        {
            Ok(memories) => {
                let candidates: Vec<Memory> = memories
                    .into_iter()
                    .filter(|m| matches!(&m.po, MemoryPo::ShortTerm(_)))
                    .collect();
                rerank_if_enabled(deps, &keyword, candidates, SHORT_TERM_TOP_K).await
            }
            Err(e) => {
                crate::log_error!(
                    &deps.ctx,
                    "legacy_awaken",
                    "short-term memory recall degraded to empty: agent_id={}, error={}",
                    deps.agent.po.id,
                    e
                );
                RerankOutcome::default()
            }
        };

        let long_term_len = long_term.selected.len();
        let short_term_len = short_term.selected.len();
        let index_len = long_term.rest.len() + short_term.rest.len();
        let mut selected = long_term.selected;
        selected.extend(short_term.selected);
        let mut rest = long_term.rest;
        rest.extend(short_term.rest);
        let knowledge_hits =
            build_knowledge_lines(&selected, &rest, &graph.neighbors, &graph.relations);

        // 5. 方法技能召回：同 tag 已发布技能 → 可选小脑精选（默认关）→ 机械排序取前 2
        //
        // 候选池不 take：精选要看到全部候选才有取舍空间。关闭开关 / 小脑降级 /
        // 无答案时退回「词面重叠优先 + 稳定排序」的老路径，逐字节等价。
        let method_skills = match crate::service::dal::skill::dal()
            .list_published_by_tag(deps.ctx.clone(), METHOD_SKILL_TAG)
            .await
        {
            Ok(skills) => {
                let mut candidates: Vec<SkillPo> = skills.into_iter().map(|s| s.po).collect();
                let (ranked, rerank_scene): (Option<Vec<SkillPo>>, Option<&'static str>) =
                    match deps.agent.brain.as_ref() {
                        Some(brain) => (
                            super::skill_relevance::rank_with_default_dao(
                                &deps.ctx,
                                brain,
                                &keyword,
                                &candidates,
                                METHOD_SKILLS_TOP_K,
                            )
                            .await,
                            Some(super::skill_relevance::SCENE),
                        ),
                        _ => (None, None),
                    };
                // 小脑留痕：与记忆精选同口径（有没有出结论都要能看见）
                if let Some(scene) = rerank_scene {
                    crate::pkg::think_trace::record_cerebellum(
                        &deps.trace_id,
                        crate::pkg::think_trace::ThinkTraceCerebellum {
                            scene: scene.to_string(),
                            decision: if ranked.is_some() {
                                "select"
                            } else {
                                "fallback"
                            }
                            .to_string(),
                            confidence: None,
                            degraded: ranked.is_none().then(|| "no_usable_answer".to_string()),
                            applied: ranked.is_some(),
                        },
                    );
                }
                match ranked {
                    Some(kept) => kept.into_iter().take(METHOD_SKILLS_TOP_K).collect(),
                    None => {
                        candidates.sort_by_key(|s| {
                            let desc = s.description.to_lowercase();
                            // 词面重叠者优先（重叠 = false 排前；稳定排序保证其余保序）
                            !new_terms.iter().any(|t| desc.contains(&t.to_lowercase()))
                        });
                        candidates.into_iter().take(METHOD_SKILLS_TOP_K).collect()
                    }
                }
            }
            Err(e) => {
                crate::log_error!(
                    &deps.ctx,
                    "legacy_awaken",
                    "method skill recall degraded to empty: agent_id={}, error={}",
                    deps.agent.po.id,
                    e
                );
                Vec::new()
            }
        };

        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "retrieve node done, agent_id={}, new_terms={}, long_term={}, short_term={}, index={}, knowledge_hits={}, method_skills={}",
            deps.agent.po.id,
            new_terms.len(),
            long_term_len,
            short_term_len,
            index_len,
            knowledge_hits.len(),
            method_skills.len()
        );

        // 6. 写总线 + 累积 visited（discover 回环下轮差集的输入）
        // 旧 pack 的思考摘要透传：discover 写入的摘要要保留到 prepare 消费
        let pack = RetrievalPack {
            query_terms: new_terms.clone(),
            knowledge_hits,
            method_skills,
            prior_thinking_summary: read_bus::<RetrievalPack>(bus, ContextKey::Knowledge)
                .ok()
                .and_then(|p| p.prior_thinking_summary),
        };
        write_pack(bus, &pack)?;

        // 锁中毒自愈：状态槽位只是记账，拿旧值继续比丢更新更接近正确
        let mut state = match deps.retrieval.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.queried_terms.extend(new_terms);

        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}
#[cfg(test)]
#[path = "retrieve_node_tests.rs"]
mod tests;
