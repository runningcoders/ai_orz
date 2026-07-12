//! 技能库管理

use dioxus::prelude::*;

use crate::api::hr::{delete_skill, list_skills};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListSkillsResponseItem;

#[component]
pub fn HrSkills() -> Element {
    let mut skills = use_signal(Vec::<ListSkillsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_skills().await {
                Ok(list) => skills.set(list.skills),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    let skills_list = skills.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            div { class: "card-header",
                h2 { class: "card-title", "技能库" }
            }
            if loading() {
                Loading {}
            } else if skills_list.is_empty() {
                EmptyState { icon: "📚".to_string(), message: "暂无技能".to_string() }
            } else {
                table { class: "table",
                    thead { tr { th { "名称" }, th { "描述" }, th { "标签" }, th { "操作" } }}
                    tbody {
                        for s in skills_list.iter() {
                            {
                                let id = s.id.clone();
                                let name = s.name.clone();
                                let description = s.description.clone();
                                let tags = s.tags.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{name}" }
                                        td { class: "text-secondary", "{description}" }
                                        td {
                                            for tag in &tags {
                                                span { class: "badge badge-neutral", style: "margin-right: 4px;", "{tag}" }
                                            }
                                        }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_skill(&id).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            match list_skills().await {
                                                                Ok(list) => skills.set(list.skills),
                                                                Err(e) => error.set(e),
                                                            }
                                                        }
                                                    });
                                                }, "删除"
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
    }
}
