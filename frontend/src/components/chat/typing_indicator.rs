//! 输入指示器组件（三点动画）
//!
//! 用于显示 Agent 正在输入的状态。

use dioxus::prelude::*;

/// Agent 正在输入的指示器
#[component]
pub fn TypingIndicator() -> Element {
    rsx! {
        div { class: "message-item agent",
            div { class: "message-avatar", "A" }
            div { class: "typing-indicator",
                div { class: "typing-dot" }
                div { class: "typing-dot" }
                div { class: "typing-dot" }
            }
        }
    }
}
