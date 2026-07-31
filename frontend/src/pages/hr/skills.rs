//! 技能库管理

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::hr::{create_skill, delete_skill, list_skills, search_skills};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    CreateSkillRequest, ListSkillsRequest, ListSkillsResponseItem, SearchSkillsRequest,
};

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
    // 修复 HIGH #12：搜索防抖 + race condition 机制
    let mut search_request_id = use_signal(|| 0u32);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_skills(ListSkillsRequest::default()).await {
                Ok(page) => skills.set(page.items),
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
                        list_skills(ListSkillsRequest::default())
                            .await
                            .map(|p| p.items)
                    } else {
                        search_skills(&SearchSkillsRequest {
                            keyword: Some(keyword),
                            ..Default::default()
                        })
                        .await
                        .map(|p| p.items)
                    };
                    match result {
                        Ok(v) => skills.set(v),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let skills_list = skills.read().clone();

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    div { class: "flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4 mb-4",
                        h2 { class: "card-title", "技能库" }
                    div { class: "flex gap-2 flex-wrap",
                        input { class: "input input-bordered w-full sm:w-auto", value: "{search_keyword}",
                            oninput: move |e| {
                                search_keyword.set(e.value());
                                // 修复 HIGH #12：防抖 300ms + request_id 丢弃过期结果
                                let my_id = search_request_id() + 1;
                                search_request_id.set(my_id);
                                spawn(async move {
                                    gloo_timers::future::TimeoutFuture::new(300).await;
                                    if search_request_id() != my_id { return; }
                                    loading.set(true);
                                    let kw = search_keyword();
                                    let result = if kw.trim().is_empty() {
                                        list_skills(ListSkillsRequest::default())
                                            .await
                                            .map(|p| p.items)
                                    } else {
                                        search_skills(&SearchSkillsRequest {
                                            keyword: Some(kw),
                                            ..Default::default()
                                        })
                                        .await
                                        .map(|p| p.items)
                                    };
                                    if search_request_id() != my_id { return; }
                                    match result {
                                        Ok(v) => skills.set(v),
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
                                    let my_id = search_request_id() + 1;
                                    search_request_id.set(my_id);
                                    spawn(async move {
                                        if search_request_id() != my_id { return; }
                                        loading.set(true);
                                        match list_skills(ListSkillsRequest::default()).await {
                                            Ok(page) => skills.set(page.items),
                                            Err(e) => toast.error(&e),
                                        }
                                        loading.set(false);
                                    });
                                },
                                "重置"
                            }
                        }
                        button { class: "btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 创建技能" }
                    }
                }
                if loading() {
                    Loading {}
                } else if skills_list.is_empty() {
                    EmptyState { icon: "📚".to_string(), message: "暂无技能".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table table-zebra table-pin-rows",
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
                                                td { class: "font-semibold", "data-label": "名称", "{name}" }
                                                td { class: "text-base-content/70", "data-label": "描述", "{description}" }
                                                td { "data-label": "标签",
                                                    div { class: "flex flex-wrap gap-1",
                                                        for tag in &tags {
                                                            span { class: "badge badge-neutral", "{tag}" }
                                                        }
                                                    }
                                                }
                                                td { "data-label": "操作",
                                                    div { class: "flex gap-1",
                                                        Link {
                                                            class: "btn btn-ghost btn-sm",
                                                            to: crate::pages::Route::HrSkillDetail { id: id.clone() },
                                                            "详情"
                                                        }
                                                        button { class: "btn btn-error btn-sm",
                                                            onclick: move |_| {
                                                                pending_delete_id.set(id.clone());
                                                                show_delete_confirm.set(true);
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
                button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "技能名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入技能名称" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "技能描述 *" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "请输入技能描述" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "标签" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_tags}",
                        oninput: move |e| new_tags.set(e.value()), placeholder: "coding, backend" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "分类" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_category}",
                        oninput: move |e| new_category.set(e.value()), placeholder: "development" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "技能内容" }
                    }
                    textarea { class: "textarea textarea-bordered w-full h-48", value: "{new_content}",
                        oninput: move |e| new_content.set(e.value()),
                        placeholder: "技能的 Markdown 内容，将写入 skill.md" }
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除".to_string(),
            message: "确定删除此技能？此操作不可撤销。".to_string(),
            on_confirm: move |_| {
                let id = pending_delete_id();
                show_delete_confirm.set(false);
                spawn(async move {
                    if let Err(e) = delete_skill(&id).await {
                        toast.error(format!("删除失败: {}", e));
                    } else {
                        let keyword = search_keyword();
                        let result = if keyword.trim().is_empty() {
                            list_skills(ListSkillsRequest::default())
                                .await
                                .map(|p| p.items)
                        } else {
                            search_skills(&SearchSkillsRequest {
                                keyword: Some(keyword),
                                ..Default::default()
                            })
                            .await
                            .map(|p| p.items)
                        };
                        match result {
                            Ok(v) => skills.set(v),
                            Err(e) => toast.error(&e),
                        }
                    }
                });
            },
            on_cancel: move |_| {
                show_delete_confirm.set(false);
            }
        }
        }
    }
}
