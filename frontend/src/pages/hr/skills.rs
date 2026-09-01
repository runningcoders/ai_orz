//! 技能库管理

use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::hr::{create_skill, delete_skill, list_skills, query_skills, search_skills};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::skill_content_input_editor::SkillContentInputEditor;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::status::{short_id, skill_author_type_badge, skill_author_type_text};
use common::api::{
    CreateSkillRequest, ListSkillsRequest, ListSkillsResponseItem, SearchSkillsRequest,
    SkillContentInput, SkillQueryRequest,
};
use common::enums::SkillAuthorType;
use common::enums::SkillStatus;

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
    let mut new_content_input = use_signal(|| Option::<SkillContentInput>::None);
    let mut creating = use_signal(|| false);
    let mut search_keyword = use_signal(String::new);
    // 修复 HIGH #12：搜索防抖 + race condition 机制
    let mut search_request_id = use_signal(|| 0u32);

    // 过滤条件
    let mut filter_category = use_signal(String::new);
    let mut filter_status = use_signal(|| -1i32);
    // -1 = 全部，0 = 用户（SkillAuthorType::User），1 = Agent（SkillAuthorType::Agent）
    let mut filter_author_type = use_signal(|| -1i32);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // 加载数据（三场景切换：list / query / search）
    let load_data = move || {
        spawn(async move {
            loading.set(true);
            let keyword = search_keyword();
            let category = filter_category();
            let status = filter_status();
            let author_type = filter_author_type();
            let my_id = search_request_id() + 1;
            search_request_id.set(my_id);

            let category_opt = if category.trim().is_empty() {
                None
            } else {
                Some(category)
            };
            let author_type_opt = if author_type < 0 {
                None
            } else {
                Some(SkillAuthorType::from(author_type))
            };
            let has_filter = category_opt.is_some() || status >= 0 || author_type_opt.is_some();

            // 三场景切换：
            // 无关键词 + 无过滤 → list_skills
            // 无关键词 + 有过滤 → query_skills
            // 有关键词 → search_skills（可同时带过滤条件）
            let result = if keyword.trim().is_empty() && !has_filter {
                list_skills(ListSkillsRequest::default())
                    .await
                    .map(|p| p.items)
            } else if keyword.trim().is_empty() {
                query_skills(&SkillQueryRequest {
                    category: category_opt.clone(),
                    status: if status >= 0 {
                        Some(SkillStatus::from(status))
                    } else {
                        None
                    },
                    author_type: author_type_opt,
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            } else {
                search_skills(&SearchSkillsRequest {
                    keyword: Some(keyword),
                    category: category_opt,
                    status: if status >= 0 {
                        Some(SkillStatus::from(status))
                    } else {
                        None
                    },
                    author_type: author_type_opt,
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            };

            // 丢弃过期请求的结果
            if search_request_id() != my_id {
                return;
            }

            match result {
                Ok(v) => skills.set(v),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    // 初始加载
    use_effect(move || {
        load_data();
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
                content_input: new_content_input(),
            };
            match create_skill(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_description.set(String::new());
                    new_tags.set(String::new());
                    new_category.set(String::new());
                    new_content_input.set(None);
                    load_data();
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let skills_list = skills.read().clone();

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: Some("HR".to_string()),
                title: "技能库".to_string(),
                actions: Some(rsx!{
                div { class: "flex gap-2 flex-wrap",
                    if !search_keyword().is_empty() || !filter_category().is_empty() || filter_status() >= 0 || filter_author_type() >= 0 {
                        button { class: "btn hud-btn btn-ghost",
                            onclick: move |_| {
                                search_keyword.set(String::new());
                                filter_category.set(String::new());
                                filter_status.set(-1);
                                filter_author_type.set(-1);
                                load_data();
                            },
                            "重置"
                        }
                    }
                    button { class: "btn hud-btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 创建技能" }
                }
                }),
            },

            // 筛选栏（独立卡片）
            HudPanel { signal: Some(true), extra_class: Some("mb-4".to_string()),
                div { class: "card-body",
                    div { class: "flex flex-wrap gap-4 items-end",
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "分类" }
                            input {
                                class: "input input-bordered w-full",
                                placeholder: "分类名称",
                                value: "{filter_category}",
                                oninput: move |e| {
                                    filter_category.set(e.value());
                                    let my_id = search_request_id() + 1;
                                    search_request_id.set(my_id);
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(300).await;
                                        if search_request_id() != my_id {
                                            return;
                                        }
                                        load_data();
                                    });
                                }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "状态" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_status}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_status.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "1", "已发布" }
                                option { value: "2", "草稿" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "作者类型" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_author_type}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_author_type.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "0", "用户" }
                                option { value: "1", "Agent" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "搜索" }
                            input {
                                class: "input input-bordered w-full",
                                placeholder: "搜索技能...",
                                value: "{search_keyword}",
                                oninput: move |e| {
                                    search_keyword.set(e.value());
                                    let my_id = search_request_id() + 1;
                                    search_request_id.set(my_id);
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(300).await;
                                        if search_request_id() != my_id {
                                            return;
                                        }
                                        load_data();
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // 列表卡片
            HudPanel { signal: Some(true),
                div { class: "card-body",
                if loading() {
                    Loading {}
                } else if skills_list.is_empty() {
                    EmptyState { icon: "📚".to_string(), message: "暂无技能".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table hud-table table-zebra table-pin-rows",
                            thead { tr { th { "名称" }, th { "描述" }, th { "标签" }, th { "创建者" }, th { "操作" } }}
                            tbody {
                                for s in skills_list.iter() {
                                    {
                                        let id = s.id.clone();
                                        let name = s.name.clone();
                                        let description = s.description.clone();
                                        let tags = s.tags.clone();
                                        let author_type = s.author_type;
                                        let author_id_short = short_id(&s.author_id);
                                        rsx! {
                                            tr { key: "{id}",
                                                td { class: "font-semibold", "data-label": "名称", "{name}" }
                                                td { class: "text-base-content/70", "data-label": "描述", "{description}" }
                                                td { "data-label": "标签",
                                                    div { class: "flex flex-wrap gap-1",
                                                        for tag in &tags {
                                                            span { class: "badge orz-tag badge-sm", "{tag}" }
                                                        }
                                                    }
                                                }
                                                td { "data-label": "创建者",
                                                    div { class: "flex items-center gap-2",
                                                        span {
                                                            class: "{skill_author_type_badge(author_type)}",
                                                            "{skill_author_type_text(author_type)}"
                                                        }
                                                        span { class: "font-mono text-xs text-base-content/60 select-all", "{author_id_short}" }
                                                    }
                                                }
                                                td { "data-label": "操作",
                                                    div { class: "flex gap-1",
                                                        Link {
                                                            class: "btn hud-btn btn-ghost btn-sm",
                                                            to: crate::pages::Route::HrSkillDetail { id: id.clone() },
                                                            "详情"
                                                        }
                                                        button { class: "btn hud-btn btn-error btn-sm",
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
                new_content_input.set(None);
            },
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: creating(), onclick: handle_create,
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
                    SkillContentInputEditor {
                        value: None,
                        on_change: move |ci| new_content_input.set(ci),
                    }
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
                        load_data();
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
