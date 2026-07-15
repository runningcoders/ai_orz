//! 技能库管理

use dioxus::prelude::*;

use crate::api::hr::{create_skill, delete_skill, list_skills, search_skills};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{CreateSkillRequest, ListSkillsResponseItem};

#[component]
pub fn HrSkills() -> Element {
    let mut skills = use_signal(Vec::<ListSkillsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_add_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut new_tags = use_signal(String::new);
    let mut new_category = use_signal(String::new);
    let mut new_content = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut search_keyword = use_signal(String::new);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_skills().await {
                Ok(list) => skills.set(list.skills),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().trim().is_empty() || new_description().trim().is_empty() {
                toast.error("技能名称和描述不能为空");
                return;
            }
            creating.set(true);
            let tags: Vec<String> = new_tags()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let req = CreateSkillRequest {
                name: new_name().trim().to_string(),
                description: new_description().trim().to_string(),
                tags,
                category: if new_category().trim().is_empty() {
                    None
                } else {
                    Some(new_category().trim().to_string())
                },
                status: None,
                content: if new_content().is_empty() {
                    None
                } else {
                    Some(new_content())
                },
                initial_files: None,
            };
            match create_skill(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_description.set(String::new());
                    new_tags.set(String::new());
                    new_category.set(String::new());
                    new_content.set(String::new());
                    let keyword = search_keyword();
                    let result = if keyword.trim().is_empty() {
                        list_skills().await
                    } else {
                        search_skills(&keyword).await
                    };
                    match result {
                        Ok(list) => skills.set(list.skills),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let skills_list = skills.read().clone();

    rsx! {
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "技能库" }
                div { class: "flex gap-2",
                    input { class: "form-input", value: "{search_keyword}",
                        oninput: move |e| {
                            let keyword = e.value();
                            search_keyword.set(keyword.clone());
                            spawn(async move {
                                loading.set(true);
                                let result = if keyword.trim().is_empty() {
                                    list_skills().await
                                } else {
                                    search_skills(&keyword).await
                                };
                                match result {
                                    Ok(list) => skills.set(list.skills),
                                    Err(e) => toast.error(&e),
                                }
                                loading.set(false);
                            });
                        },
                        placeholder: "搜索技能..."
                    }
                    if !search_keyword().is_empty() {
                        button { class: "btn btn-ghost",
                            onclick: move |_| {
                                search_keyword.set(String::new());
                                spawn(async move {
                                    loading.set(true);
                                    match list_skills().await {
                                        Ok(list) => skills.set(list.skills),
                                        Err(e) => toast.error(&e),
                                    }
                                    loading.set(false);
                                });
                            },
                            "重置"
                        }
                    }
                    button { class: "btn btn-accent", onclick: move |_| show_add_modal.set(true), "+ 创建技能" }
                }
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
                                        td { class: "detail-table-value-bold", "{name}" }
                                        td { class: "text-secondary", "{description}" }
                                        td {
                                            for tag in &tags {
                                                span { class: "badge badge-neutral tag-item", "{tag}" }
                                            }
                                        }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_skill(&id).await {
                                                            toast.error(&format!("删除失败: {}", e));
                                                        } else {
                                                            let keyword = search_keyword();
                                                            let result = if keyword.trim().is_empty() {
                                                                list_skills().await
                                                            } else {
                                                                search_skills(&keyword).await
                                                            };
                                                            match result {
                                                                Ok(list) => skills.set(list.skills),
                                                                Err(e) => toast.error(&e),
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

        // 创建技能弹窗
        Modal {
            title: "创建新技能".to_string(),
            show: show_add_modal(),
            on_close: move |_| {
                show_add_modal.set(false);
                new_name.set(String::new());
                new_description.set(String::new());
                new_tags.set(String::new());
                new_category.set(String::new());
                new_content.set(String::new());
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "技能名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入技能名称" }
                }
                div { class: "form-group",
                    label { class: "form-label", "技能描述 *" }
                    textarea { class: "form-textarea", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "请输入技能描述" }
                }
                div { class: "form-group",
                    label { class: "form-label", "标签" }
                    input { class: "form-input", value: "{new_tags}",
                        oninput: move |e| new_tags.set(e.value()), placeholder: "coding, backend" }
                }
                div { class: "form-group",
                    label { class: "form-label", "分类" }
                    input { class: "form-input", value: "{new_category}",
                        oninput: move |e| new_category.set(e.value()), placeholder: "development" }
                }
                div { class: "form-group",
                    label { class: "form-label", "技能内容" }
                    textarea { class: "form-textarea", value: "{new_content}",
                        oninput: move |e| new_content.set(e.value()),
                        placeholder: "技能的 Markdown 内容，将写入 skill.md" }
                }
            }
        }
    }
}
