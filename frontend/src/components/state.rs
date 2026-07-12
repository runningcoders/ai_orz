//! 状态展示组件 - 加载中、空状态、错误提示

use dioxus::prelude::*;

#[component]
pub fn Loading() -> Element {
    rsx! { div { class: "state-loading", "加载中..." } }
}

#[component]
pub fn EmptyState(icon: Option<String>, message: String) -> Element {
    let icon = icon.unwrap_or_else(|| "📭".to_string());
    rsx! {
        div { class: "state-empty",
            div { class: "state-empty-icon", "{icon}" }
            p { "{message}" }
        }
    }
}

#[component]
pub fn ErrorAlert(message: String) -> Element {
    if message.is_empty() {
        return rsx! {};
    }
    rsx! { div { class: "alert alert-error", "{message}" } }
}

#[component]
pub fn SuccessAlert(message: String) -> Element {
    if message.is_empty() {
        return rsx! {};
    }
    rsx! { div { class: "alert alert-success", "{message}" } }
}
