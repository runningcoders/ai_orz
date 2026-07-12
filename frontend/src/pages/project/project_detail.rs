use dioxus::prelude::*;

#[component]
pub fn ProjectDetail(id: String) -> Element {
    rsx! { div { class: "card", "项目详情 {id} - 待实现" } }
}
