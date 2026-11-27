//! Agent 摘要展示分组（头像信息气泡 / 聊天侧栏 Agent Tab 共用）
//!
//! 抽取动机：这两处的「身份行 + 状态徽章行」此前逐字重复，只靠注释约定「同源」，
//! 任一侧改样式就会静默漂移。收成一份实现后，展示口径只有一处。
//!
//! 用普通函数而非 `#[component]`：Dioxus 的 Props derive 要求字段实现 `PartialEq`，
//! 而 common 的 `GetAgentResponse` 未实现（不为展示层需要污染共享数据契约）。

use common::api::GetAgentResponse;
use dioxus::prelude::*;
use dioxus_router::Link;

use crate::utils::{
    agent_lifecycle_badge, agent_lifecycle_text, agent_runtime_badge, agent_runtime_text,
    avatar_initials, tag_chip,
};

/// Agent 身份行：头像 + 名称 + 类型。
///
/// `avatar_ring` 由调用方决定（一般传 `avatar_status_ring(agent.status)`，传空串则不叠环）。
/// ⚠️ 同一个展示单元内若已有 [`agent_badge_row`] 的生命周期徽章，
/// 头像环就是同一状态说第二遍（ui_design_system §4.4 状态语义唯一）→ 传空串。
pub fn agent_identity_row(agent: &GetAgentResponse, avatar_ring: &str) -> Element {
    rsx! {
        div { class: "flex items-center gap-2",
            div { class: "w-10 h-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold {avatar_ring}",
                "{avatar_initials(&agent.name)}"
            }
            div { class: "flex-1 min-w-0",
                div { class: "font-semibold truncate", "{agent.name}" }
                div { class: "text-xs text-base-content/60", "类型：{agent.kind}" }
            }
        }
    }
}

/// Agent 状态与角色徽章行：生命周期徽章 + 运行时徽章 + 角色属性标签。
///
/// 三者配色/形状均走 `utils/status.rs` 单一事实源（状态走 `hud-badge` 语义色，
/// 属性走 `orz-tag` 中性 pill），禁止在此散写 DaisyUI 原始徽章类。
pub fn agent_badge_row(agent: &GetAgentResponse) -> Element {
    rsx! {
        div { class: "flex flex-wrap gap-1",
            span { class: "{agent_lifecycle_badge(agent.status)}",
                "{agent_lifecycle_text(agent.status)}"
            }
            span { class: "{agent_runtime_badge(agent.runtime_state)}",
                "{agent_runtime_text(agent.runtime_state)}"
            }
            for role in agent.roles.iter() {
                span { key: "{role}", class: "{tag_chip()}", "{role}" }
            }
        }
    }
}

/// 身份行右上角的管理入口：在详情页打开。
///
/// 这是「查看完整档案」而不是快捷交互，故与下方 [`agent_quick_actions`] 分开摆放。
/// 图标 + 原生 `title` hover，不占卡片纵向空间。
pub fn agent_detail_link(agent_id: &str) -> Element {
    let aid = agent_id.to_string();
    rsx! {
        Link {
            class: "btn hud-btn btn-ghost btn-xs btn-circle flex-none",
            title: "在详情页打开",
            to: crate::pages::Route::HrAgentDetail { id: aid },
            svg {
                width: 15, height: 15, view_box: "0 0 24 24",
                fill: "none", stroke: "currentColor", stroke_width: 1.9,
                path { d: "M14 4h6v6M20 4l-9 9M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4" }
            }
        }
    }
}

/// 签名下方的横向快捷功能组，对齐飞书个人名片：
/// 头像 → 姓名 → 个性签名 → **消息 / 语音 / 视频 图标组** → 个人信息字段。
///
/// Agent 场景下目前只有「专注模式」；后续电话 / 视频等交互入口只在这里加一个
/// `Link` 即可，简介卡与信息页上部同时生效。文案收进原生 `title` hover。
pub fn agent_quick_actions(agent_id: &str) -> Element {
    let aid = agent_id.to_string();
    rsx! {
        div { class: "flex items-center gap-2",
            // 专注模式 = 工作台 AgentDetail 视图；只需要 Agent ID，无需额外状态
            Link {
                class: "btn hud-btn btn-ghost btn-sm min-w-14",
                title: "进入专注模式",
                to: crate::pages::Route::Workspace {
                    view: Some(format!("agent:{aid}")),
                },
                svg {
                    width: 18, height: 18, view_box: "0 0 24 24",
                    fill: "none", stroke: "currentColor", stroke_width: 1.9,
                    path { d: "M8 3H5a2 2 0 0 0-2 2v3M16 3h3a2 2 0 0 1 2 2v3M8 21H5a2 2 0 0 1-2-2v-3M16 21h3a2 2 0 0 0 2-2v-3" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// 两组入口分离：右上角详情圆钮 / 签名下方横向快捷组；均为图标 + hover 文案。
    #[test]
    fn agent_actions_are_icon_only_and_layered() {
        let src = include_str!("agent_summary.rs");
        let detail = src
            .find("pub fn agent_detail_link")
            .expect("agent_detail_link must exist");
        let quick = src
            .find("pub fn agent_quick_actions")
            .expect("agent_quick_actions must exist");
        let detail_body = &src[detail..quick];
        let quick_body = &src[quick..src.find("#[cfg(test)]").unwrap_or(src.len())];

        assert!(
            detail_body.contains("btn-circle"),
            "详情页入口必须是右上角圆钮"
        );
        assert!(detail_body.contains("title: \"在详情页打开\""));
        assert!(
            !detail_body.contains("进入专注模式"),
            "专注模式不属于右上角管理入口"
        );
        assert!(
            quick_body.contains("flex items-center gap-2"),
            "快捷组必须横向排列"
        );
        assert!(quick_body.contains("title: \"进入专注模式\""));
        for label in ["进入专注模式", "在详情页打开"] {
            assert!(
                !src.contains(&format!("\"{label} →\"")),
                "{label} 不允许出现冗长按钮文案"
            );
        }
    }
}
