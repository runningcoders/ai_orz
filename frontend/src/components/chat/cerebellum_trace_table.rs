//! 小脑决策留痕的展示组件（run 卡片内的「折叠条 + 三维平表」）
//!
//! ## 为什么是「卡片内折叠条」而不是一级 Tab（形态丙）
//!
//! 小脑判定的语义单位是 **run**（一次唤醒里 route / 强度 / 两路精选 / chain_reply
//! 各自判一次），不是「一条独立的观测流」。做成一级 Tab 会让它和「思考」「工具」
//! 平级，读者会误以为它是跨 run 的全局视图；而 99% 的排查场景是「这次 run 到底
//! 判了什么」—— 答案应当就在那张卡片上。
//!
//! 硬约束（A 单实勘）：不动 `tab_names` 索引、不新增一级 tab。
//!
//! ## 为什么是平表而不是聚合图
//!
//! 三个维度（scene / decision / degraded）**不可按字面聚合**：不同 scene 的
//! `decision` 取值域不同（`trivial_direct` 是路由选项、`70` 是强度分数、
//! `close` 是收敛结论），混在一起算「跳过率」会得到一个没有意义的数。平表逐条
//! 陈列，让读者自己按 scene 分列看，比任何聚合都诚实。
//!
//! `bootstrap` / `not_assembled`（第 0 案）会作为普通行出现 —— 这正是它的用途：
//! 把「没装配」从「没触发」里区分出来。

use dioxus::prelude::*;

use common::api::ThinkTraceCerebellumDto;

/// 小脑留痕 → 中文场景文案
///
/// 与 `think_traces_tab::scene_label`（思考场景）刻意分开：两套 scene 词表不同
/// （那边是 awaken/settle/compact，这边是 entry_gate/thinking_strength.* /
/// chain_reply），混用一个 match 会不断往里塞不相干的分支。
pub fn cerebellum_scene_label(scene: &str) -> &'static str {
    match scene {
        "bootstrap" => "装配检查",
        "entry_gate" => "入口门",
        "entry_pool" => "候选池",
        "chain_reply" => "链深收敛",
        "memory_relevance" => "记忆精选",
        "skill_relevance" => "技能精选",
        "thinking_strength.awaken" => "强度·唤醒",
        "thinking_strength.intent" => "强度·意图",
        "thinking_strength.summary" => "强度·总结",
        "thinking_strength.compact" => "强度·压缩",
        "thinking_strength.settle" => "强度·沉淀",
        _ => "其他",
    }
}

/// 平表里「降级」列的文案（`None` = 正常出结论）
pub fn degraded_label(degraded: Option<&str>) -> &'static str {
    match degraded {
        None => "-",
        Some("ceiling_forced") => "绝对天花板",
        Some("circuit_open") => "熔断开闸",
        Some("capacity_too_small") => "容量过小",
        Some("budget_exhausted") => "预算耗尽",
        Some("empty_input") => "空输入",
        Some("pool_fallback") => "池兜底",
        Some("timeout") => "超时",
        Some("transport") => "传输失败",
        // 后端 `DegradedReason::as_str()` 与强度场景的五种成因：不补齐词表，
        // 这些真实原因会全部塌成笼统的「降级」，排查时看不出是哪一环出的问题。
        Some("no_provider") => "无小脑",
        Some("disabled") => "已关闭",
        Some("no_user_message") => "无用户消息",
        Some("call_error") => "调用失败",
        Some("low_confidence") => "置信不足",
        Some("unsupported_answer") => "答案不符",
        Some("no_usable_answer") => "无可用答案",
        Some("no_answer") => "无答案",
        Some("bad_option") => "选项异常",
        Some("empty_requests") => "空请求",
        Some(_) => "降级",
    }
}

/// 一次 run 的小脑留痕汇总（折叠条用）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CerebellumStats {
    /// 总条数
    pub total: usize,
    /// 降级条数（`degraded.is_some()`）
    pub degraded: usize,
    /// 真正改变了下游的条数（`applied`）
    pub applied: usize,
    /// 平均时延（毫秒，仅统计有 latency_ms 的条目）；无样本为 None
    pub avg_latency_ms: Option<u64>,
    /// p95 时延（毫秒，仅统计有 latency_ms 的条目，样本 <2 时等于最大值）
    pub p95_latency_ms: Option<u64>,
}

/// 从留痕列表现算汇总（纯函数：可单测，不依赖渲染）
pub fn cerebellum_stats(items: &[ThinkTraceCerebellumDto]) -> CerebellumStats {
    let total = items.len();
    let degraded = items.iter().filter(|c| c.degraded.is_some()).count();
    let applied = items.iter().filter(|c| c.applied).count();

    let mut latencies: Vec<u64> = items.iter().filter_map(|c| c.latency_ms).collect();
    if latencies.is_empty() {
        return CerebellumStats {
            total,
            degraded,
            applied,
            avg_latency_ms: None,
            p95_latency_ms: None,
        };
    }
    // 升序后按最近秩取 p95：小样本（<20）下这是最能反映「最慢那几次」的取法，
    // 插值法会把 1 个样本的 p95 算成同一个值，反而不直观
    latencies.sort_unstable();
    let sum: u64 = latencies.iter().sum();
    let avg = sum / latencies.len() as u64;
    let rank = ((latencies.len() as f64) * 0.95).ceil() as usize;
    let idx = rank.saturating_sub(1).min(latencies.len() - 1);
    CerebellumStats {
        total,
        degraded,
        applied,
        avg_latency_ms: Some(avg),
        p95_latency_ms: Some(latencies[idx]),
    }
}

/// 折叠条：run 卡片收窄态那一行「小脑 n 次 · 降级 k · 均 x ms」
///
/// 没有留痕时**不渲染**（由调用方 `if !items.is_empty()` 控制），避免给
/// 每一个正常 run 都加一行空噪声。
#[component]
pub fn CerebellumSummaryLine(items: Vec<ThinkTraceCerebellumDto>) -> Element {
    if items.is_empty() {
        return rsx! {};
    }
    let stats = cerebellum_stats(&items);
    let latency = match (stats.avg_latency_ms, stats.p95_latency_ms) {
        (Some(avg), Some(p95)) => format!("均 {avg}ms · p95 {p95}ms"),
        _ => "无调用".to_string(),
    };
    rsx! {
        div { class: "text-[11px] text-base-content/60 mt-1 flex items-center gap-2 flex-wrap",
            span { class: "badge orz-tag badge-xs", "小脑" }
            span { "{stats.total} 次" }
            if stats.degraded > 0 {
                span { class: "text-warning", "降级 {stats.degraded}" }
            }
            if stats.applied > 0 {
                span { "生效 {stats.applied}" }
            }
            span { "{latency}" }
        }
    }
}

/// 三维平表：scene × decision × degraded（run 卡片展开态内嵌）
#[component]
pub fn CerebellumTable(items: Vec<ThinkTraceCerebellumDto>) -> Element {
    if items.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "space-y-1",
            div { class: "text-xs text-base-content/70 font-medium", "小脑决策（{items.len()} 条）" }
            div { class: "overflow-x-auto",
                table { class: "table table-xs",
                    thead {
                        tr {
                            th { "场景" }
                            th { "结论" }
                            th { "置信" }
                            th { "降级" }
                            th { "生效" }
                            th { "时延" }
                        }
                    }
                    tbody {
                        for (idx, item) in items.iter().enumerate() {
                            tr { key: "{idx}",
                                td { title: "{item.scene}", "{cerebellum_scene_label(&item.scene)}" }
                                td { class: "font-mono text-[11px]", "{item.decision}" }
                                td {
                                    {
                                        item.confidence
                                            .map(|c| format!("{:.2}", c))
                                            .unwrap_or_else(|| "-".to_string())
                                    }
                                }
                                td {
                                    class: if item.degraded.is_some() { "text-warning" } else { "" },
                                    {
                                        degraded_label(item.degraded.as_deref())
                                    }
                                }
                                td {
                                    if item.applied { "✓" } else { "-" }
                                }
                                td {
                                    {
                                        item.latency_ms
                                            .map(|ms| format!("{ms}ms"))
                                            .unwrap_or_else(|| "-".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        scene: &str,
        decision: &str,
        degraded: Option<&str>,
        applied: bool,
        ms: Option<u64>,
    ) -> ThinkTraceCerebellumDto {
        ThinkTraceCerebellumDto {
            scene: scene.to_string(),
            decision: decision.to_string(),
            confidence: None,
            degraded: degraded.map(str::to_string),
            applied,
            latency_ms: ms,
        }
    }

    #[test]
    fn stats_counts_degraded_and_applied_separately() {
        // 降级与生效是两个维度，不可互相推导：降级的条目 applied 恒 false，
        // 但 applied=false 不一定是降级（例如强度算了但 Provider 未启用该能力）。
        let items = vec![
            item("entry_gate", "trivial_direct", None, true, Some(100)),
            item("entry_gate", "fallback", Some("timeout"), false, Some(1000)),
            item("thinking_strength.awaken", "70", None, false, None),
        ];
        let stats = cerebellum_stats(&items);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.degraded, 1, "只有显式带 degraded 的算降级");
        assert_eq!(
            stats.applied, 1,
            "applied=false 不等于降级，两个计数各自独立"
        );
    }

    #[test]
    fn stats_latency_ignores_none_and_computes_p95() {
        // 未发生调用的条目（latency_ms=None）不得进分母：否则 p95 被系统性低估
        let items = vec![
            item("a", "x", None, false, Some(10)),
            item("a", "x", None, false, Some(20)),
            item("a", "x", None, false, Some(30)),
            item("a", "x", None, false, None),
        ];
        let stats = cerebellum_stats(&items);
        assert_eq!(stats.avg_latency_ms, Some(20), "均值只按 3 个有值样本算");
        assert_eq!(stats.p95_latency_ms, Some(30));
    }

    #[test]
    fn stats_without_any_call_has_no_latency() {
        let items = vec![item("bootstrap", "not_assembled", None, false, None)];
        let stats = cerebellum_stats(&items);
        assert_eq!(stats.total, 1);
        assert_eq!(stats.avg_latency_ms, None, "装配缺失没有调用，不能报时延");
        assert_eq!(stats.p95_latency_ms, None);
    }

    #[test]
    fn scene_labels_cover_contract_values_used_by_backend() {
        // 后端会写这些 scene 字面量；漏一个就会在平表里显示成「其他」
        assert_eq!(cerebellum_scene_label("bootstrap"), "装配检查");
        assert_eq!(cerebellum_scene_label("entry_gate"), "入口门");
        assert_eq!(cerebellum_scene_label("entry_pool"), "候选池");
        assert_eq!(cerebellum_scene_label("chain_reply"), "链深收敛");
        assert_eq!(
            cerebellum_scene_label("thinking_strength.intent"),
            "强度·意图"
        );
        assert_eq!(cerebellum_scene_label("unknown_scene"), "其他");
    }

    #[test]
    fn degraded_label_passes_through_unknown_reason() {
        assert_eq!(degraded_label(None), "-");
        assert_eq!(degraded_label(Some("circuit_open")), "熔断开闸");
        assert_eq!(
            degraded_label(Some("something_new")),
            "降级",
            "未知原因也要显式标降级，不能静默显示成正常"
        );
    }

    #[test]
    fn degraded_label_names_real_backend_reasons() {
        // 后端 `DegradedReason::as_str()` 与强度场景五种成因的实际取值：
        // 漏一个就会在平表里塌成笼统的「降级」，排查时无法归因。
        assert_eq!(degraded_label(Some("call_error")), "调用失败");
        assert_eq!(degraded_label(Some("timeout")), "超时");
        assert_eq!(degraded_label(Some("no_usable_answer")), "无可用答案");
        assert_eq!(degraded_label(Some("no_answer")), "无答案");
        assert_eq!(degraded_label(Some("no_provider")), "无小脑");
        assert_eq!(degraded_label(Some("budget_exhausted")), "预算耗尽");
    }
}
