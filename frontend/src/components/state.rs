//! 状态展示组件 - 加载中、空状态、错误提示

use dioxus::prelude::*;

#[component]
pub fn Loading(#[props(default = "md")] size: &'static str) -> Element {
    rsx! {
        span { class: "loading loading-spinner loading-{size}" }
    }
}

#[component]
pub fn EmptyState(icon: Option<String>, message: String) -> Element {
    let icon = icon.unwrap_or_else(|| "📭".to_string());
    rsx! {
        div { class: "text-center py-8 text-base-content/60",
            div { class: "text-4xl mb-3", "{icon}" }
            p { "{message}" }
        }
    }
}

#[component]
pub fn ErrorAlert(message: String) -> Element {
    if message.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "alert alert-error flex items-center gap-2",
            span { "⚠️" }
            span { "{message}" }
        }
    }
}

#[component]
pub fn SuccessAlert(message: String) -> Element {
    if message.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "alert alert-success flex items-center gap-2",
            span { "✅" }
            span { "{message}" }
        }
    }
}
