//! 附件管理

use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::background_task::get_task_progress;
use crate::api::finance::{
    create_text_attachment, delete_attachment, list_attachments, search_attachments,
};
use crate::api::system::rebuild_vectors;
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::auth::use_auth_state;
use crate::store::toast::use_toast;
use crate::utils::{format_datetime_full as format_timestamp, format_file_size as format_size};
use common::api::{
    AttachmentDetail, AttachmentSearchResult, CreateTextAttachmentRequest, SearchAttachmentsRequest,
};
use common::enums::UserRole;

#[component]
pub fn FinanceAttachments() -> Element {
    let mut attachments = use_signal(Vec::<AttachmentSearchResult>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_add_modal = use_signal(|| false);

    let mut new_file_name = use_signal(String::new);
    let mut new_content = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 搜索（FTS5 + 向量），300ms 防抖 + 请求代际丢弃 =====
    let mut search_keyword = use_signal(String::new);
    let mut search_request_id = use_signal(|| 0u32);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // ===== 重建附件向量索引（仅 SuperAdmin，专属入口：entities 固定只有 attachment）=====
    let mut show_rebuild_confirm = use_signal(|| false);
    let mut rebuilding = use_signal(|| false);
    let mut rebuild_progress = use_signal(String::new);
    let mut rebuild_mode = use_signal(|| common::enums::VectorRebuildMode::Missing);
    let auth = use_auth_state();
    let is_super_admin =
        UserRole::has_permission(UserRole::from_i32(auth().role), UserRole::SuperAdmin);

    let load_data = move || {
        spawn(async move {
            loading.set(true);
            let keyword = search_keyword().trim().to_string();
            let my_id = search_request_id() + 1;
            search_request_id.set(my_id);

            let result = if keyword.is_empty() {
                // 无关键词：走列表，包装成统一的搜索结果项
                list_attachments().await.map(|list| {
                    list.into_iter()
                        .map(|a: AttachmentDetail| AttachmentSearchResult {
                            attachment: a,
                            match_type: None,
                            fts_rank: None,
                            vector_distance: None,
                        })
                        .collect::<Vec<_>>()
                })
            } else {
                search_attachments(&SearchAttachmentsRequest {
                    keyword: Some(keyword),
                    file_type: None,
                    purpose: None,
                    ..Default::default()
                })
                .await
                .map(|page| page.items)
            };
            // 丢弃过期请求的结果（防竞态）。
            // ⚠️ 过期分支故意不复位 loading：loading 由「最新一代」请求独占管理，
            // 若这里也 set(false)，会在新请求仍在飞行时提前清除加载态（闪烁）。
            if search_request_id() != my_id {
                return;
            }
            match result {
                Ok(list) => attachments.set(list),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load_data();
    });

    // 重建附件向量：复用系统 vector-rebuild 接口，entities 固定 ["attachment"]。
    // 缺省 Missing（查漏补缺，只补向量缺失/摘要已变的），Force 需用户显式选；
    // 不给 Auto —— 口径一致时它一条不看，按钮会变成空操作。
    let confirm_rebuild = move |_| {
        let mode = rebuild_mode();
        rebuilding.set(true);
        rebuild_progress.set("正在提交重建任务...".to_string());
        spawn(async move {
            let req = common::api::RebuildVectorsRequest {
                entities: Some(vec!["attachment".to_string()]),
                mode,
            };
            let task_id = match rebuild_vectors(req).await {
                Ok(r) => r.task_id,
                Err(e) => {
                    toast.error(format!("提交重建任务失败: {}", e));
                    rebuilding.set(false);
                    return;
                }
            };
            loop {
                gloo_timers::future::TimeoutFuture::new(1000).await;
                match get_task_progress(&task_id).await {
                    Ok(p) => {
                        rebuild_progress.set(p.step_message.clone());
                        match p.status {
                            common::api::TaskStatus::Completed => {
                                let processed = p
                                    .result
                                    .as_ref()
                                    .and_then(|v| v.get("processed"))
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                let verb = if mode == common::enums::VectorRebuildMode::Missing {
                                    "查漏补缺完成：扫描"
                                } else {
                                    "强制重建完成：处理"
                                };
                                toast.success(format!("附件索引{} {} 条", verb, processed));
                                load_data();
                                break;
                            }
                            common::api::TaskStatus::Failed => {
                                toast.error(format!(
                                    "重建失败: {}",
                                    p.error.unwrap_or_else(|| "未知错误".to_string())
                                ));
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        toast.error(format!("查询重建进度失败: {}", e));
                        break;
                    }
                }
            }
            rebuilding.set(false);
            rebuild_progress.set(String::new());
            show_rebuild_confirm.set(false);
        });
    };

    let handle_create = move |_| {
        spawn(async move {
            if new_file_name().is_empty() {
                toast.error("文件名不能为空");
                return;
            }
            if new_content().is_empty() {
                toast.error("内容不能为空");
                return;
            }
            creating.set(true);
            let req = CreateTextAttachmentRequest {
                file_name: new_file_name(),
                content: new_content(),
                mime_type: None,
                purpose: None,
            };
            match create_text_attachment(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_file_name.set(String::new());
                    new_content.set(String::new());
                    toast.success("创建成功");
                    load_data();
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let attachments_list = attachments.read().clone();

    rsx! {
        AppLayout {
            HudPanel { signal: Some(true),
                div { class: "card-body",
                    PageHeader {
                        eyebrow: Some("FINANCE".to_string()),
                        title: "附件管理".to_string(),
                        actions: Some(rsx!{
                        if is_super_admin {
                            button {
                                class: "btn hud-btn btn-ghost",
                                disabled: rebuilding(),
                                onclick: move |_| show_rebuild_confirm.set(true),
                                if rebuilding() { "⟳ 重建中..." } else { "⟳ 同步重建向量" }
                            }
                        }
                        button { class: "btn hud-btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 创建文本附件" }
                        }),
                    },
                    // 搜索框：关键词走混合搜索（文件名 + AI 摘要 + 用途）
                    div { class: "flex flex-col gap-1 mb-4 max-w-md",
                        label { class: "form-label", "搜索" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "搜索文件名、摘要或用途…",
                            value: "{search_keyword}",
                            oninput: move |e| {
                                let value = e.value();
                                search_keyword.set(value);
                                let my_id = search_request_id() + 1;
                                search_request_id.set(my_id);
                                spawn(async move {
                                    gloo_timers::future::TimeoutFuture::new(300).await;
                                    if search_request_id() != my_id { return; }
                                    load_data();
                                });
                            }
                        }
                    },
                    if loading() {
                        Loading {}
                    } else if attachments_list.is_empty() {
                        EmptyState { icon: "📎".to_string(), message: "暂无附件".to_string() }
                    } else {
                        div { class: "overflow-x-auto",
                            table { class: "table hud-table table-zebra table-pin-rows",
                                thead { tr { th { "文件名" }, th { "大小" }, th { "用途" }, th { "摘要" }, th { "创建时间" }, th { "操作" } }}
                                tbody {
                                    for item in attachments_list.iter() {
                                        {
                                            let a = &item.attachment;
                                            let id = a.id.clone();
                                            let original_name = a.original_name.clone();
                                            let size = format_size(a.size);
                                            let purpose = a.purpose.clone();
                                            let description = a.description.clone();
                                            let match_type = item.match_type.clone().unwrap_or_default();
                                            let created_at = format_timestamp(a.created_at);
                                            let id_delete = id.clone();
                                            rsx! {
                                                tr { key: "{id}",
                                                    td { class: "font-semibold",
                                                        "{original_name}"
                                                        if !match_type.is_empty() {
                                                            span { class: "badge orz-tag badge-sm ml-2", "{match_type}" }
                                                        }
                                                    }
                                                    td { "{size}" }
                                                    td { span { class: "badge orz-tag badge-sm", "{purpose}" } }
                                                    td { class: "max-w-xs truncate", title: "{description}", "{description}" }
                                                    td { "{created_at}" }
                                                    td {
                                                        div { class: "flex gap-1",
                                                            Link {
                                                                class: "btn hud-btn btn-ghost btn-sm",
                                                                to: crate::pages::Route::FinanceAttachmentDetail { id: id.clone() },
                                                                "详情"
                                                            }
                                                            button { class: "btn hud-btn btn-error btn-sm",
                                                                onclick: move |_| {
                                                                    pending_delete_id.set(id_delete.clone());
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

            Modal {
                title: "创建文本附件".to_string(),
                show: show_add_modal(),
                on_close: move |_| show_add_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: creating(), onclick: handle_create,
                        if creating() { "创建中..." } else { "创建" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "文件名 *" }
                        }
                        input { class: "input input-bordered w-full", value: "{new_file_name}",
                            oninput: move |e| new_file_name.set(e.value()), placeholder: "如: notes.txt" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "内容 *" }
                        }
                        textarea { class: "textarea textarea-bordered w-full", rows: "10", value: "{new_content}",
                        oninput: move |e| new_content.set(e.value()), placeholder: "输入文本内容..." }
                    }
                }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此附件？此操作不可撤销。".to_string(),
                on_confirm: move |_| {
                    let id = pending_delete_id();
                    show_delete_confirm.set(false);
                    spawn(async move {
                        if let Err(e) = delete_attachment(&id).await {
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

            // 重建附件向量：与技能页同款两档选择，但 entities 固定只有 attachment。
            Modal {
                title: "同步重建附件向量".to_string(),
                show: show_rebuild_confirm(),
                width_class: Some("max-w-2xl".to_string()),
                on_close: move |_| {
                    // 执行中不允许关：否则进度和「还在跑」的反馈一起消失
                    if !rebuilding() {
                        show_rebuild_confirm.set(false);
                    }
                },
                footer: rsx! {
                    button {
                        class: "btn hud-btn btn-ghost",
                        disabled: rebuilding(),
                        onclick: move |_| show_rebuild_confirm.set(false),
                        "取消"
                    }
                    button {
                        class: "btn hud-btn btn-primary",
                        disabled: rebuilding(),
                        onclick: confirm_rebuild,
                        if rebuilding() { "执行中..." } else { "开始执行" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "border border-base-300 rounded-box bg-base-200 overflow-hidden",
                        div { class: "grid gap-2 p-3",
                            div {
                                class: if rebuild_mode() == common::enums::VectorRebuildMode::Missing {
                                    "card cursor-pointer border-2 border-primary bg-base-200 transition-colors"
                                } else {
                                    "card cursor-pointer border border-base-300 bg-base-200 transition-colors"
                                },
                                onclick: move |_| {
                                    if !rebuilding() {
                                        rebuild_mode.set(common::enums::VectorRebuildMode::Missing);
                                    }
                                },
                                div { class: "card-body p-3",
                                    div { class: "flex items-center gap-2",
                                        span { class: "font-semibold", "1 · 查漏补缺" }
                                        span { class: "badge orz-tag badge-sm", "推荐" }
                                    }
                                    div { class: "text-xs text-base-content/70",
                                        "不清空现有索引，逐条比对摘要口径，只重算「向量缺失」或「文件名 / 描述 / 用途已改」的附件。费用最低，适合日常维护。"
                                    }
                                }
                            }
                            div {
                                class: if rebuild_mode() == common::enums::VectorRebuildMode::Force {
                                    "card cursor-pointer border-2 border-primary bg-base-200 transition-colors"
                                } else {
                                    "card cursor-pointer border border-base-300 bg-base-200 transition-colors"
                                },
                                onclick: move |_| {
                                    if !rebuilding() {
                                        rebuild_mode.set(common::enums::VectorRebuildMode::Force);
                                    }
                                },
                                div { class: "card-body p-3",
                                    div { class: "font-semibold", "2 · 强制重建" }
                                    div { class: "text-xs text-base-content/70",
                                        "清空全部附件向量后逐条重新计算，每条都会真实调用 embedding，可能产生费用。怀疑附件索引整体损坏时用这档，最彻底。"
                                    }
                                }
                            }
                        }
                    }

                    if rebuilding() {
                        div { class: "flex items-center gap-2 text-sm text-base-content/70",
                            span { class: "loading loading-spinner loading-sm" }
                            span { "{rebuild_progress}" }
                        }
                    }

                    div { class: "text-xs text-base-content/50",
                        "仅重建附件集合（entities 固定为 attachment），Agent / 记忆 / 技能 / 任务 / 项目 / 消息 / 工具不受影响；后台执行期间附件语义搜索可能短暂不完整，同一时刻只允许一个重建任务运行。"
                    }
                }
            }
        }
    }
}
