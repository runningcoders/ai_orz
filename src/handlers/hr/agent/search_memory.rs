//! Handler: 搜索记忆 - Neural Tool

use std::collections::HashSet;

use super::property_attach::{AttachOptions, attach_properties};
use crate::models::memory::{Memory, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::dal::memory::TraversalStrategy;
use crate::service::dao::memory::MemorySearch;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MemoryResult, PropertyFilterParam, SearchMemoryParams, SearchMemoryResponse};
use common::enums::MemoryType;
use common::error::{Result, bail_err, err};
use common::ontology_property::PropertyValue;

/// Search memory by keyword or semantic query
#[register_handler_tool(
    id = "search_memory",
    name = "Search Memory (Semantic)",
    description = "Search memories with a free-text query via hybrid keyword + vector semantic matching; results are ranked and annotated with match_type, vector_distance, and fts_rank. memory_type accepts short_term/knowledge_node/trace/relation/all; traversal_strategy accepts breadth_first/depth_first. Values outside those lists are rejected with invalid_request rather than silently defaulted. Knowledge nodes are hive-shared: every agent can read all of them, and agent_id is only an optional ownership filter on the seeds (omit it to search the whole hive). Pass seed_node_ids to expand through the knowledge graph instead of searching: the seed nodes themselves are always returned (the center node is never dropped), traversal_depth/breadth/strategy shape the expansion, and neighbors are never filtered by ownership. Pass property_filters for exact structured filtering on node properties (term_key + operator + typed value, AND semantics): it runs as an independent recall channel that resolves matching node ids first instead of re-filtering the semantic top_k, and combines with query as an intersection; it cannot be combined with seed_node_ids, and comparison operators other than eq require a number value. Returned knowledge nodes carry their current properties, so you can see what is already attached (and re-send an item to correct it via update_memory). `tags` matching uses one composed view: a node matches when any requested tag equals one of its WRITABLE user tags, OR when the requested tag is a system flag (`published`) carried by the read-only is_published marker (the marker is the source of truth - do not rely on a `published` string sitting in the tag list). Results echo the two faces separately: `tags` is the writable business-label list, `system_flags` is a read-only derived list (currently only `published`; always present on knowledge nodes, empty array when not published). Never write `published` into tags - use save_long_term_memory / update_memory's is_published parameter instead. For structured field filtering on tags/agent/status use query_memory. Retired (soft-deleted) knowledge nodes are EXCLUDED by default; pass include_retired: true to also return them - such a node then echoes status = retired, which is the SAME value as the request-side forgotten (status 0), not a fifth state. The response echoes the effective switch in include_retired. retired_excluded_count reports how many retired knowledge nodes the keyword channel filtered out; it is measured in a widened probe re-run, so it stays exact even when max_results truncates the main result set (it is a lower bound only if the probe itself hits its internal cap, and it is approximate when a vector channel participated). It is omitted (null) in four cases: include_retired is true (nothing was excluded), the seed / traversal channels (not measured), when property_filters is present (the property hit set is applied after recall, so the count would overstate), and when the probe re-run itself fails (degraded to unmeasured; the main request still succeeds). Result status is active / settled / retired for knowledge nodes. Relation results on the search channels are ALWAYS active: every relation read path hard-codes status = 1 (six SELECTs in dao/memory/sqlite/relations.rs), so superseded and deleted edges never reach a search result - the retired / deleted labels remain in the mapping only as a complete conversion for write-side / update paths that may hand a non-active edge to to_api_result. short_term and trace results omit the key entirely.",
    params = "common::api::SearchMemoryParams",
    neural,
    tags = "ontology"
)]
#[generate_http_handler]
pub async fn search_memory(
    ctx: RequestContext,
    params: SearchMemoryParams,
) -> Result<SearchMemoryResponse> {
    // 调用主体：人类用户（HTTP）或 Agent（唤醒 / 休息沉淀链路）都可以操作记忆。
    // ⚠️ 不能只认 user —— 休息沉淀的 ctx 由 `RequestContext::new_system()` 还原，
    // **天生没有 user_id**（只有 agent_id）：`agent_rest` cron → `agent.settle.requested`
    // → Settle 场景，而沉淀 prompt 明确要求 Agent 调用本工具（TEMPLATE_MEMORY_COGNITION）。
    // 只认 user 会让这类调用全部 400「当前请求缺少用户上下文」，实测 call_trace 已复现。
    // 记忆的归属维度是 Agent（短期私有）与蜂巢（知识节点共享），本就与 user 无关。
    if ctx.uid().is_empty() && ctx.agent_id().is_none() {
        bail_err!(InvalidRequest, "当前请求缺少用户/Agent 上下文");
    }

    // 归属筛选：**只认显式传入的 `params.agent_id`**（空串 = None = 不过滤）。
    // ⚠️ 刻意**不**回退 `ctx.agent_id()`：
    // - 知识节点是蜂巢共享资产，回退会让 Agent 的检索静默收窄成「只看自己沉淀的节点」，
    //   与「所有 Agent 都能看到所有知识节点」相反；
    // - 短期记忆（私有）需要的归属回退由 DAL 内部完成（`private_agent_scope`），
    //   那里才分得清「私有要作用域」和「共享不要门槛」。
    // 该参数只约束**起点**的选取，不约束沿图展开（见下方分支注释）。
    let agent_filter: Option<String> = params.agent_id.clone().filter(|s| !s.is_empty());

    // 归属作用域自述（回包 `agent_scope`，入口处判定**一次**，两条返回路径共用）：
    // 全库 / 显式作用域，**没有第三种取值** —— 因为这里刻意不回退 `ctx.agent_id()`，
    // 不存在「上下文兜底」这种中间态。调用方据此确认「这次查的是谁的范围」。
    let agent_scope: String = match &agent_filter {
        Some(id) => format!("explicit:{id}"),
        None => "hive".to_string(),
    };
    // `agent_filter` 在下方召回分支里会被 move 进查询参数，这里先固化成 bool，
    // 供回包 `reason` 的优先级链复用（否则 move 之后无法再问「有没有显式作用域」）。
    let has_explicit_agent = agent_filter.is_some();

    // 退役节点受控出口的**生效值**（入口处固化一次，两条返回路径共用）：
    // `None`（未传）= 默认排除退役，与拆分前的既有行为一致。
    // 回包用 `include_retired` 回显它，让调用方不必回看自己传了什么。
    let include_retired: bool = params.include_retired.unwrap_or(false);

    // 属性过滤入参先落地：后面据此收敛记忆类型与命中集（见「精确召回通道」注释）
    let property_filters: Vec<PropertyFilterParam> =
        params.property_filters.clone().unwrap_or_default();
    let has_property_filters = !property_filters.is_empty();

    // 类型/遍历策略统一走枚举自带的 SSOT 解析，非法值直接 400（不做 `_ =>` 兜底）。
    // ⚠️ 静默降级会让调用方拼错一个词就拿到全量结果 / 另一种形状的图，而响应看起来成功。
    let memory_type = match params.memory_type.as_deref() {
        None => MemoryType::All,
        Some(raw) => MemoryType::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 memory_type: `{}`，合法取值：{}",
                raw,
                MemoryType::ACCEPTED_VALUES
            )
        })?,
    };
    // 属性过滤是针对**知识节点**的结构化条件（工具面只开放节点载体）：
    // 命中集里只可能是节点 id，混查其他记忆类型只会稀释结果。
    let effective_memory_type = if has_property_filters {
        MemoryType::KnowledgeNode
    } else {
        memory_type
    };

    let traversal_depth = params.traversal_depth.unwrap_or(0);
    let traversal_breadth = params.traversal_breadth.unwrap_or(0);
    let traversal_strategy = match params.traversal_strategy.as_deref() {
        None => TraversalStrategy::DEFAULT,
        Some(raw) => TraversalStrategy::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 traversal_strategy: `{}`，合法取值：{}",
                raw,
                TraversalStrategy::ACCEPTED_VALUES
            )
        })?,
    };
    let seed_node_ids = params.seed_node_ids.clone().unwrap_or_default();

    let has_seeds = !seed_node_ids.is_empty();
    let do_traversal = traversal_depth > 0;

    // 🎯 属性过滤：结构化**精确召回**通道（与语义检索互补）
    //
    // 先按属性反查命中节点 id，再把 id 注入 `MemoryQuery.ids` 让 DAO 直取节点——
    // **不是**在语义 top_k 里再筛一遍（那样命中率会被语义排序吃掉）。
    let property_hit_ids: Option<Vec<String>> = match property_filters.as_slice() {
        [] => None,
        filters => {
            // 与点名种子互斥：种子恒返回（遍历不变量），叠加过滤会造成
            // 「传了过滤却仍返回未命中节点」的静默失真，直接报 400 而非悄悄忽略。
            if has_seeds {
                bail_err!(
                    InvalidRequest,
                    "property_filters 与 seed_node_ids 不可同时使用（点名种子展开时种子恒返回，属性过滤无法生效）"
                );
            }
            // 数值运算符必须配 number 值：DAL 侧同样会拒，但那里是
            // 「查询无结果」，这里是「调用方写错参数」，报错要早且要说清。
            for f in filters {
                if let Some(op) = f.op
                    && op.requires_number()
                    && !matches!(f.value, PropertyValue::Number(_))
                {
                    bail_err!(
                        InvalidRequest,
                        "属性 {} 的比较运算 {:?} 仅支持 number 值，当前给了 {}",
                        f.term_key,
                        op,
                        f.value.data_type().as_str()
                    );
                }
            }
            let filters: Vec<common::ontology_property::PropertyFilter> =
                filters.iter().map(|f| f.clone().into_filter()).collect();
            Some(
                runtime_domain()
                    .memory()
                    .query_entities_by_properties(ctx.clone(), &filters)
                    .await?,
            )
        }
    };

    // 命中集为空 = 确实没有符合条件的节点，**短路返回空**而不是回退成全量：
    // ids 为空集若被当成「不过滤」，调用方会拿到全量结果并误以为筛过了。
    if let Some(ids) = &property_hit_ids
        && ids.is_empty()
    {
        // 空结果原因：本分支 = 「传了 property_filters 且精确通道命中集为空」，
        // 五值里**只有** `filter_no_match` 说得通，且它**最高优先** —— 即使同时
        // 传了显式 `agent_id` 也不是 `agent_scope_mismatch`：命中集压根没有元素，
        // 归属筛选还没来得及参与，把「本来就空」归因成「被筛空」会把排查方向带偏。
        // `semantic_used` 给 `None`：此路径没进任何召回通道，无法判定向量跑没跑，
        // 给 `false` 是伪报。`returned` 恒 0（命中集空 ⇒ 一条都不可能返回）。
        return Ok(SearchMemoryResponse {
            results: Vec::new(),
            matched_total: Some(0),
            returned: Some(0),
            agent_scope: Some(agent_scope),
            semantic_used: None,
            reason: Some("filter_no_match".to_string()),
            // 本路径没进任何召回通道，无从计量被排除的退役节点（未计量 ≠ 0 条）。
            retired_excluded_count: None,
            include_retired: Some(include_retired),
        });
    }

    let mut all_memories: Vec<Memory> = Vec::new();

    // 被排除的退役节点条数（受控出口的**可观测性**）：只在纯检索分支给出真实值，
    // 种子 / 遍历分支恒 `None`（未计量 —— 那些分支的召回范围由图结构决定，
    // 「被排除多少条」没有稳定口径）。
    let mut retired_excluded_count: Option<u64> = None;

    if has_seeds {
        // 🎯 点名种子（前端点击节点展开 / 调用方指定起点）：直接沿图展开，**不搜索**。
        // - 种子**必返回**（中心节点缺席的话整张展开图就没有意义了）；
        // - 展开不受归属筛选：知识节点蜂巢共享，邻居不该因为属于别人而被丢掉；
        // - `traversal_depth = 0` 表示「只看种子、不展开」，同样走这条路而不是回退到搜索。
        let traversed = runtime_domain()
            .memory()
            .traverse_graph(
                ctx.clone(),
                &seed_node_ids,
                traversal_depth,
                traversal_breadth,
                traversal_strategy,
                include_retired,
            )
            .await?;
        all_memories.extend(traversed);
    } else if do_traversal {
        // 🔍 关键词搜索选起点（**这里**才受归属筛选：`agent_id` 决定从谁的节点起步）
        // → 再沿图展开（蜂巢全域，见 `has_seeds` 分支的说明）。
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(MemoryType::KnowledgeNode),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                agent_id: agent_filter.clone(),
                ids: property_hit_ids.clone(),
                include_retired,
                ..Default::default()
            },
            ..Default::default()
        };

        let search_results = runtime_domain()
            .memory()
            .search(ctx.clone(), search)
            .await?;

        let seed_ids: Vec<String> = search_results
            .iter()
            .filter_map(|m| match &m.po {
                MemoryPo::KnowledgeNode(kn) => Some(kn.id.clone()),
                _ => None,
            })
            .collect();

        all_memories.extend(search_results);

        if !seed_ids.is_empty() {
            let traversed = runtime_domain()
                .memory()
                .traverse_graph(
                    ctx.clone(),
                    &seed_ids,
                    traversal_depth,
                    traversal_breadth,
                    traversal_strategy,
                    include_retired,
                )
                .await?;
            all_memories.extend(traversed);
        }
    } else {
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(effective_memory_type),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                task_id: params.task_id.clone(),
                agent_id: agent_filter.clone(),
                ids: property_hit_ids.clone(),
                include_retired,
                ..Default::default()
            },
            ..Default::default()
        };

        let search_results = runtime_domain()
            .memory()
            .search(ctx.clone(), search)
            .await?;
        all_memories.extend(search_results);

        // ===== 退役节点计量（仅本分支：纯关键词 / 语义检索）=====
        // 口径：在**放开退役**的同条件二次检索里直接数
        // `MemoryPo::KnowledgeNode` 且 `status == Forgotten` 的条数
        // （等价于回包里的 `memory_type == "knowledge_node" && status == "retired"`）。
        //
        // 为什么不用「二次条数 − 本次条数」：本次若被 `max_results` 截断，差值会把
        // 「本来就没返回的活跃节点」也算成退役 ⇒ **高估**。直接数不受截断影响。
        // 为什么二次检索要放开 limit：沿用 `max_results` 会让两次都撞同一个上限，
        // 退役节点可能整批落在窗口外 ⇒ 恒为 0 的假值。
        // 安全侧：二次检索的结果**绝不进 `results`**，失败只让本字段降级为 `None`，
        // 主请求照常成功（计量是附加观测，不是主功能）。
        // 三种 `None`：① `include_retired == true`（没有排除，也就无从计量，省一次检索）；
        // ② 传了 `property_filters`（属性命中集是召回**之后**的内存 `retain`，这里不重放
        //    该收口 ⇒ 会数进不带该属性的退役节点，既高估又与同回包的 `matched_total` 矛盾）；
        // ③ 二次检索失败。
        if !include_retired && !has_property_filters {
            let probe = MemorySearch {
                keyword: Some(params.query.clone()),
                top_k: Some(RETIRED_COUNT_PROBE_LIMIT),
                filters: crate::service::dao::memory::MemoryQuery {
                    memory_type: Some(effective_memory_type),
                    limit: Some(RETIRED_COUNT_PROBE_LIMIT as usize),
                    tags: params.tags.clone(),
                    task_id: params.task_id.clone(),
                    agent_id: agent_filter.clone(),
                    include_retired: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            if let Ok(probe_results) = runtime_domain().memory().search(ctx.clone(), probe).await {
                // 按 **id 去重**后计数：二次检索走的是 DAL 合并通道（关键词/向量/图
                // 多条来源可能给出同一节点的多行），而主结果的 `results` 是去重过的。
                // 不去重就会把「一条退役节点被两条来源命中」数成两条 ⇒ 高估，
                // 与同一回包里 `results` 的节点数口径也对不上。
                // 计数用的是**节点数**（退役节点有多少个被排除），不是命中行数。
                let retired_ids: std::collections::HashSet<&str> = probe_results
                    .iter()
                    .filter_map(|m| match &m.po {
                        MemoryPo::KnowledgeNode(kn)
                            if kn.status == common::enums::MemoryStatus::Forgotten =>
                        {
                            Some(kn.id.as_str())
                        }
                        _ => None,
                    })
                    .collect();
                retired_excluded_count = Some(retired_ids.len() as u64);
            }
        }
    }

    let mut seen = HashSet::new();
    let mut unique_memories = Vec::new();
    for memory in all_memories {
        let id = memory_id(&memory);
        if seen.insert(id) {
            unique_memories.push(memory);
        }
    }

    // 属性条件是**硬条件**：任何召回通道（FTS / 向量 / 遍历）都不能越过命中集。
    // DAO 的 FTS 通道已经把候选集下推到 SQL，这里再兜一道——混合搜索会合并
    // 多个通道的结果，只改一条通道不足以保证「命中集之外一个都不出现」。
    if let Some(hit_ids) = &property_hit_ids {
        unique_memories.retain(|m| hit_ids.contains(&memory_id(m)));
    }

    let mut results = memories_to_results(unique_memories);

    // 属性回显：一趟批量查完再回填（防 N+1）。
    // 装配逻辑**只有一份**（`property_attach::attach_properties`），query_memory 复用同一个
    // 函数、同一个 `AttachOptions`——否则 (d)「唯一装配源」当场作废。
    // 默认只回显生效值；`include_history=true` 才带出 status=0 历史行（status=2 永不返回）。
    // 本文件的 has_seeds / do_traversal / else 三分支都汇到这一处调用，天然同口径（A1-j）。
    attach_properties(
        &ctx,
        &mut results,
        AttachOptions::from_param(params.include_history),
    )
    .await?;

    // ===== 回包计量（④）：四个字段的取值口径逐条落在注释里，改之前先看这里 =====
    //
    // `semantic_used` 三态：有结果且见 vector_distance ⇒ Some(true)；有结果但全无
    // ⇒ Some(false)（确定未参与）；空结果 ⇒ None（无法判定，不伪报 false）。
    let semantic_used = semantic_used_of(&results);
    let returned_count = results.len() as u64;

    // `reason` 五值封闭的**优先级链**（顺序即优先级，勿调换）：
    //   ① 有 property_filters 且命中集为空 → 已在上方早返回（`filter_no_match`），
    //      走不到这里；这条分支最高优先，即使同时有显式 agent_id。
    //   ② 有 property_filters + 命中集非空 + 显式 agent → `agent_scope_mismatch`
    //      （命中集里的节点都不在该 agent 名下）。
    //   ③ 有 property_filters + 命中集非空 + 无显式 agent → `property_filter_not_applied`
    //      （命中集非空却零结果 ⇒ 召回通道没把命中集注入，属实现异常自述）。
    //   ④ 无 property_filters + 显式 agent → `agent_scope_explicit_empty`
    //      （该作用域下确实没有记忆；用别的值会让人怀疑回退了他域结果）。
    //   ⑤ 无 property_filters + 无显式 agent → `no_recall_hit`（全库未召回）。
    // 非空结果一律 `None`（有结果就不存在「为什么空」的问题）。
    let reason: Option<String> = if results.is_empty() {
        let code = if has_property_filters {
            if property_hit_ids.as_ref().is_some_and(|ids| !ids.is_empty()) {
                if has_explicit_agent {
                    "agent_scope_mismatch"
                } else {
                    "property_filter_not_applied"
                }
            } else {
                // 理论上不可达（filters 非空 ⇒ 命中集恒为 Some，空集已早返回）；
                // 真走到这里说明命中集被清空却漏了短路，仍归到最贴近的语义。
                "filter_no_match"
            }
        } else if has_explicit_agent {
            "agent_scope_explicit_empty"
        } else {
            "no_recall_hit"
        };
        Some(code.to_string())
    } else {
        None
    };

    Ok(SearchMemoryResponse {
        results,
        // 仅当传了 property_filters 才给（`property_hit_ids` 为 None ⇔ 没传 filters）
        matched_total: property_hit_ids.as_ref().map(|ids| ids.len() as u64),
        returned: has_property_filters.then_some(returned_count),
        agent_scope: Some(agent_scope),
        semantic_used,
        reason,
        // 计量口径见 `else` 分支内的注释（三通道里只有纯检索分支能给出稳定值）。
        retired_excluded_count,
        include_retired: Some(include_retired),
    })
}

/// 二次检索（计量退役节点）放开的上限
///
/// 必须**远大于**常见 `max_results`（工具默认/常用范围 5~50，上限 100）：
/// 沿用请求的 `max_results` 会让「本次」与「二次」撞同一个窗口，退役节点可能整批
/// 落在窗口外 ⇒ 计量恒为 0 的假值。500 足够覆盖单次检索的实际候选规模；
/// 真撞到这个上限时结果只是**下界**（字段文档已声明）。
const RETIRED_COUNT_PROBE_LIMIT: i32 = 500;

/// 向量通道是否真实参与了本次召回（三态，见 `SearchMemoryResponse::semantic_used`）。
///
/// **单一判据**：只看 `search_match.vector_distance` 有没有值，不要同时再用
/// `score.is_some()`（两者同源换算，判两处迟早漂移成两种说法）。
fn semantic_used_of(results: &[MemoryResult]) -> Option<bool> {
    if results.is_empty() {
        // 空结果下「有没有跑向量」无从观测 ⇒ 诚实缺省，不给 false
        return None;
    }
    Some(results.iter().any(|r| {
        r.search_match
            .as_ref()
            .and_then(|m| m.vector_distance)
            .is_some()
    }))
}

fn memory_id(memory: &Memory) -> String {
    match &memory.po {
        MemoryPo::Trace(t) => t.id.clone(),
        MemoryPo::ShortTerm(st) => st.id.clone(),
        MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
        MemoryPo::Relation(rel) => rel.id.clone(),
    }
}

fn memories_to_results(memories: Vec<Memory>) -> Vec<MemoryResult> {
    // 字段映射收敛在 `Memory::to_api_result`（与 query_memory 共用一份实现），
    // 这里只负责批量转换
    memories.iter().map(Memory::to_api_result).collect()
}

#[cfg(test)]
#[path = "search_memory_tests.rs"]
mod tests;
