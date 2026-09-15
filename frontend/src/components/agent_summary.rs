//! Agent 摘要展示分组（头像信息气泡 / 聊天侧栏 Agent Tab 共用）
//!
//! 抽取动机：这两处的「身份行 + 状态徽章行」此前逐字重复，只靠注释约定「同源」，
//! 任一侧改样式就会静默漂移。收成一份实现后，展示口径只有一处。
//!
//! 用普通函数而非 `#[component]`：Dioxus 的 Props derive 要求字段实现 `PartialEq`，
//! 而 common 的 `GetAgentResponse` 未实现（不为展示层需要污染共享数据契约）。

use common::api::GetAgentResponse;
use dioxus::prelude::*;

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
