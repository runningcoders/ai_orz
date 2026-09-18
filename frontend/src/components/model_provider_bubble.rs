//! 模型提供商信息气泡（ModelProviderBubble）
//!
//! 点击 Agent 页里的模型提供商标识，弹出「基础信息 + 最近调用统计」的精简卡片，
//! 卡片底部提供详情页入口——交互与 [`AvatarBubble`] 同一套范式：DaisyUI dropdown +
//! onfocus 展开 + `position: fixed` 实测锚点浮层 + 懒加载缓存（定位工具直接复用
//! avatar_bubble 开放的 pub(crate) 接口，勿再复制一套逻辑）。
//! 触发层是文本 chip（长条形），锚点按实测宽高计算，右对齐偏移不会算偏。
//!
//! ⚠️ 展开路径只有 focus 一条，不要改回 `onclick`（原因见 [`AvatarBubble`] 文件头）。

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::finance::get_model_provider;
use crate::components::avatar_bubble::{
    BubbleAlign, BubbleAnchor, focused_element, resolve_anchor, scroll_ancestor_rect, viewport_size,
};
use crate::components::state::Loading;
use crate::utils::now_ms;
use crate::utils::number::format_compact_count;
use common::api::{GetModelProviderRequest, GetModelProviderResponse};
use common::models::ModelCallStats;

/// 气泡统计窗口：最近 24 小时（毫秒）
const STATS_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// 构造带最近调用统计的 GetModelProviderRequest（口径「最近 24 小时」，hourly 打点）。
///
/// 字段命名为 `stats_start_time` / `stats_end_time`（与其它实体的 `stats_time_start` 不同），
/// 且后端用 zip 配对，start / end / interval 必须成对提供才生效。
fn provider_stats_request(id: &str) -> GetModelProviderRequest {
    let now = now_ms();
    GetModelProviderRequest {
        id: id.to_string(),
        with_model_call_stats: Some(true),
        stats_start_time: Some(now - STATS_WINDOW_MS),
        stats_end_time: Some(now),
        stats_interval: Some("hourly".to_string()),
    }
}

/// 点击弹出模型提供商信息气泡。
///
/// - `provider_id` → 详情接口入参与卡片底部跳转目标
/// - `provider_label` → 触发层展示文案（列表页传「名称 (模型名)」，详情页传原始 ID）
/// - `align` → 浮层水平对齐（同 [`BubbleAlign`]）
#[component]
pub fn ModelProviderBubble(
    provider_id: String,
    provider_label: String,
    #[props(default)] align: BubbleAlign,
) -> Element {
    // None=未加载 / Some(Err)=加载失败 / Some(Ok)=已加载；仅展开时拉取并缓存
    let mut provider_card = use_signal(|| None::<Result<GetModelProviderResponse, ()>>);
    // 浮层锚点：聚焦时按触发层实测矩形算出，配合 `position: fixed` 渲染
    let mut anchor = use_signal(|| None::<BubbleAnchor>);
    let align_class = align.dropdown_class();
    let open_provider_id = provider_id.clone();
    let card_style = anchor().map_or_else(String::new, |a| a.style());
    rsx! {
        div { class: "dropdown dropdown-top {align_class}",
            div {
                tabindex: 0,
                role: "button",
                class: "link link-primary font-mono text-sm cursor-pointer outline-none",
                title: "查看模型信息",
                // 展开路径只有 focus 一条：锚点定位与懒加载都挂在这里
                //（不要改回 onclick，DaisyUI 的 pointer-events 规则会吞掉点击，见 AvatarBubble）
                onfocus: move |_| {
                    if let Some(el) = focused_element()
                        && let Some((vw, vh)) = viewport_size()
                    {
                        let r = el.get_bounding_client_rect();
                        let bounds = scroll_ancestor_rect(&el);
                        anchor.set(Some(resolve_anchor(&r, vw, vh, align, bounds)));
                    }
                    // 已成功加载则跳过；未加载或上次失败都重新拉取（失败可重试）
                    if matches!(provider_card(), Some(Ok(_))) {
                        return;
                    }
                    let pid = open_provider_id.clone();
                    spawn(async move {
                        let result = match get_model_provider(provider_stats_request(&pid)).await {
                            Ok(p) => Some(Ok(p)),
                            Err(_) => Some(Err(())),
                        };
                        provider_card.set(result);
                    });
                },
                "{provider_label}"
            }
            // 浮层视觉走 `.orz-popover`（与 AvatarBubble 同一浮层档，ui_design_system §8）
            div {
                tabindex: 0,
                class: "dropdown-content orz-popover w-64 p-3",
                style: card_style,
                match provider_card() {
                    None => rsx! {
                        div { class: "flex items-center justify-center gap-2 py-2 text-xs text-base-content/60",
                            Loading { size: "xs" }
                            span { "加载中…" }
                        }
                    },
                    Some(Err(())) => rsx! {
                        div { class: "text-xs text-error py-1", "信息加载失败" }
                    },
                    Some(Ok(p)) => rsx! { { provider_bubble_card_content(&p) } },
                }
            }
        }
    }
}

/// 模型提供商信息卡内容（紧凑预览版）。
///
/// 普通函数而非 `#[component]`：Dioxus 组件 Props derive 要求字段实现 PartialEq，
/// 而 common 的 `GetModelProviderResponse` 未实现（与 [`AvatarBubble`] 同因）。
fn provider_bubble_card_content(provider: &GetModelProviderResponse) -> Element {
    let pid = provider.id.clone();
    let desc = provider.description.clone().filter(|s| !s.is_empty());
    let ptype = provider.provider_type.to_string();
    let cap_label = if provider.capability.is_embedding() {
        "embedding"
    } else {
        "agent"
    };
    rsx! {
        div { class: "space-y-2",
            div { class: "space-y-1 min-w-0",
                div { class: "font-semibold truncate", "{provider.name}" }
                div { class: "flex flex-wrap items-center gap-1",
                    span { class: "badge orz-tag badge-sm", "{ptype}" }
                    span { class: "badge orz-tag badge-sm", "{cap_label}" }
                    span { class: "font-mono text-xs text-base-content/60 truncate", "{provider.model_name}" }
                }
            }
            if let Some(d) = desc {
                p { class: "text-xs text-base-content/70 line-clamp-2", "{d}" }
            }
            { provider_stats_grid(provider.stats.as_ref()) }
            Link {
                class: "btn hud-btn btn-ghost btn-xs",
                to: crate::pages::Route::FinanceModelProviderDetail { id: pid },
                "在详情页打开 →"
            }
        }
    }
}

/// 统计读数（2 列网格）：调用 / 平均 QPS / 输入 / 输出 Token，口径「最近 24 小时」。
fn provider_stats_grid(stats: Option<&ModelCallStats>) -> Element {
    let Some(s) = stats else {
        return rsx! {
            div { class: "text-xs text-base-content/60", "最近 24 小时暂无调用统计" }
        };
    };
    let (calls, avg_qps) = s
        .call_summary
        .as_ref()
        .map_or((0, None), |c| (c.total_calls, c.avg_qps));
    let avg_qps_text = avg_qps.map(|q| format!("{q:.2}"));
    let (tokens_in, tokens_out) = s
        .token_summary
        .as_ref()
        .map_or((0, 0), |t| (t.total_tokens_input, t.total_tokens_output));
    rsx! {
        div { class: "space-y-1",
            div { class: "text-[10px] uppercase tracking-wide text-base-content/50",
                "最近 24 小时"
            }
            div { class: "grid grid-cols-2 gap-x-3 gap-y-1 text-xs",
                div { class: "flex items-baseline justify-between gap-2 min-w-0",
                    span { class: "text-base-content/60", "调用" }
                    span { class: "font-mono", "{format_compact_count(calls)}" }
                }
                if let Some(qps) = avg_qps_text {
                    div { class: "flex items-baseline justify-between gap-2 min-w-0",
                        span { class: "text-base-content/60", "平均 QPS" }
                        span { class: "font-mono", "{qps}" }
                    }
                }
                div { class: "flex items-baseline justify-between gap-2 min-w-0",
                    span { class: "text-base-content/60", "输入 Token" }
                    span { class: "font-mono", "{format_compact_count(tokens_in)}" }
                }
                div { class: "flex items-baseline justify-between gap-2 min-w-0",
                    span { class: "text-base-content/60", "输出 Token" }
                    span { class: "font-mono", "{format_compact_count(tokens_out)}" }
                }
            }
        }
    }
}
