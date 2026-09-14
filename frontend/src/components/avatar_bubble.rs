//! 通用头像信息气泡（AvatarBubble）
//!
//! 点击头像弹出基础信息卡，Agent / 用户两用：
//! - Agent 头像：点击时按需拉取 `get_agent`（组件内缓存，重复点击不重复请求），
//!   卡片内容与聊天侧栏 Agent Tab 同源（生命周期 / 运行时徽章 + 简介 + 能力）。
//! - 用户头像：静态展示显示名与短 ID。
//!
//! 交互采用 DaisyUI dropdown 范式（tabindex + focus-within），点外自动收起，零 JS。
//! `status` 传入时按 [`avatar_status_ring`] 渲染生命周期警示环（待离职=黄 / 已离职=红）。
//!
//! ⚠️ **宿主契约**：调用方只需给一个定位容器（聊天页是 `.chat-image`，负责 grid 定位），
//! **不要**再叠加 `.avatar`——`.avatar` 由本组件贴在触发层上，紧邻圆形，
//! 否则 DaisyUI 的 `.avatar > div`（aspect-ratio + overflow:hidden）会落到中间的
//! dropdown 包装层，导致头像变椭圆 + 浮层被裁（详见组件内结构约束注释）。

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::hr::get_agent;
use crate::utils::{
    agent_lifecycle_badge, agent_lifecycle_text, agent_runtime_badge, agent_runtime_text,
    avatar_initials, avatar_status_ring, tag_chip,
};
use common::api::{GetAgentRequest, GetAgentResponse};

/// 头像配色基调：Agent=secondary / User=primary（与聊天页原头像配色一致）
#[derive(Clone, Copy, PartialEq)]
pub enum AvatarTone {
    Agent,
    User,
}

impl AvatarTone {
    fn classes(self) -> &'static str {
        match self {
            Self::Agent => "bg-secondary text-secondary-content",
            Self::User => "bg-primary text-primary-content",
        }
    }
}

/// 点击头像弹出基础信息气泡。
///
/// - `agent_id` 有值 → Agent 卡（点击时懒加载 get_agent）
/// - `user_id` 有值 → 用户卡（展示短 ID）
/// - `status` 有值 → 按 [`avatar_status_ring`] 叠加生命周期警示环
/// - `align` → DaisyUI dropdown 方位类（`dropdown-start` 左对齐 / `dropdown-end` 右对齐）
#[component]
pub fn AvatarBubble(
    name: String,
    tone: AvatarTone,
    #[props(default)] agent_id: Option<String>,
    #[props(default)] status: Option<i32>,
    #[props(default)] user_id: Option<String>,
    #[props(default = "dropdown-start")] align: &'static str,
) -> Element {
    // None=未加载 / Some(Err)=加载失败 / Some(Ok)=已加载；仅点击时拉取并缓存
    let mut agent_card = use_signal(|| None::<Result<GetAgentResponse, ()>>);
    let ring = status.map(avatar_status_ring).unwrap_or("");
    let tone_classes = tone.classes();
    let has_agent_card = agent_id.is_some();
    let open_agent_id = agent_id.clone();
    rsx! {
        // ⚠️ 根节点与 `.avatar` 之间**不能再插包装层**：DaisyUI 的
        // `.avatar > div { aspect-ratio: 1; display: block; overflow: hidden }`
        // 会命中 `.avatar` 的直接子 div。此前把 `.dropdown` 放在中间，
        // 该规则落到包装层上，同时踩两个坑（2026-09-14 无头实测）：
        //   ① 圆只剩 `w-10`（无 h-10）、正方形由 `.avatar > div` 的 aspect-ratio 提供，
        //      规则被包装层吃掉后圆盒实测 40 × 24 ——「头像被挤压」；
        //   ② `overflow: hidden` 把 `bottom: 100%` 的浮层裁进 40 × 40 的包装盒里，
        //      卡片 computed style 是 `display:block; opacity:1`（focus-within 正常工作）
        //      却一个像素都看不见 ——「点击无反应」。
        // 故 `.avatar` 只贴在触发层、紧邻圆形；宿主（`.chat-image`）只负责 grid 定位。
        div { class: "dropdown dropdown-top {align}",
            div {
                tabindex: 0,
                role: "button",
                class: "avatar cursor-pointer outline-none",
                onclick: move |_| {
                    let Some(aid) = open_agent_id.clone() else {
                        return;
                    };
                    if agent_card().is_some() {
                        return;
                    }
                    spawn(async move {
                        let result = match get_agent(GetAgentRequest {
                            id: aid,
                            ..Default::default()
                        })
                        .await
                        {
                            Ok(a) => Some(Ok(a)),
                            Err(_) => Some(Err(())),
                        };
                        agent_card.set(result);
                    });
                },
                div { class: "w-10 h-10 rounded-full {tone_classes} flex items-center justify-center font-bold {ring}",
                    "{avatar_initials(&name)}"
                }
            }
            div {
                tabindex: 0,
                class: "dropdown-content z-[100] mb-2 w-64 rounded-box border border-base-300 bg-base-100 p-3 shadow-xl",
                if has_agent_card {
                    match agent_card() {
                        None => rsx! { div { class: "text-xs text-base-content/60", "加载中…" } },
                        Some(Err(())) => rsx! { div { class: "text-xs text-error", "信息加载失败" } },
                        Some(Ok(a)) => rsx! { { agent_bubble_card_content(&a) } },
                    }
                } else {
                    { user_bubble_card_content(&name, user_id.as_deref()) }
                }
            }
        }
    }
}

/// Agent 信息卡内容：与聊天侧栏 AgentTab 展示同源（徽章 + 简介 + 能力 + 跳转）。
///
/// 普通函数而非 `#[component]`：Dioxus 组件 Props derive 要求字段实现 PartialEq，
/// 而 common 的 `GetAgentResponse` 未实现（避免为此污染数据契约）。
fn agent_bubble_card_content(agent: &GetAgentResponse) -> Element {
    let aid = agent.id.clone();
    let kind = agent.kind.clone();
    let desc = agent.description.clone().filter(|s| !s.is_empty());
    let capabilities = agent.capabilities.clone().unwrap_or_default();
    rsx! {
        div { class: "space-y-2",
            div { class: "flex items-center gap-2",
                div { class: "w-10 h-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold {avatar_status_ring(agent.status)}",
                    "{agent.name.chars().next().unwrap_or('A')}"
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-semibold truncate", "{agent.name}" }
                    div { class: "text-xs text-base-content/60", "类型：{kind}" }
                }
            }
            div { class: "flex flex-wrap gap-1",
                span { class: "{agent_lifecycle_badge(agent.status)}", "{agent_lifecycle_text(agent.status)}" }
                span { class: "{agent_runtime_badge(agent.runtime_state)}", "{agent_runtime_text(agent.runtime_state)}" }
                for role in agent.roles.iter() {
                    span { key: "{role}", class: "{tag_chip()}", "{role}" }
                }
            }
            if let Some(d) = desc {
                p { class: "text-xs text-base-content/70 line-clamp-3", "{d}" }
            }
            if !capabilities.is_empty() {
                div { class: "flex flex-wrap gap-1",
                    for c in capabilities.iter().take(6) {
                        span { key: "{c}", class: "{tag_chip()}", "{c}" }
                    }
                }
            }
            Link {
                class: "btn hud-btn btn-ghost btn-xs",
                to: crate::pages::Route::HrAgentDetail { id: aid },
                "在详情页打开 →"
            }
        }
    }
}

/// 用户信息卡内容：静态展示（显示名 + 短 ID），无额外请求
fn user_bubble_card_content(name: &str, user_id: Option<&str>) -> Element {
    // utils 里有两个同名 short_id（status/message 各一），glob 重导出二义，
    // 故此处用 status 模块完整路径调用
    let uid_text = user_id.map(crate::utils::status::short_id);
    rsx! {
        div { class: "space-y-2",
            div { class: "flex items-center gap-2",
                div { class: "w-10 h-10 rounded-full bg-primary text-primary-content flex items-center justify-center font-bold",
                    "{avatar_initials(&name)}"
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-semibold truncate", "{name}" }
                    div { class: "text-xs text-base-content/60", "当前用户" }
                }
            }
            if let Some(text) = uid_text {
                div { class: "text-xs text-base-content/60", "ID：{text}" }
            }
        }
    }
}
