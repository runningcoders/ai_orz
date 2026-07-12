use dioxus::prelude::*;

#[component]
pub fn HrAgentDetail(id: String) -> Element {
    rsx! { div { class: "card", "Agent 详情 {id} - 待实现" } }
}
