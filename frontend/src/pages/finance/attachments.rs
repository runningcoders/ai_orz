//! 附件管理

use dioxus::prelude::*;

use crate::api::finance::{create_text_attachment, delete_attachment, list_attachments};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{AttachmentDetail, CreateTextAttachmentRequest};

fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn format_timestamp(ts: i64) -> String {
    let mut sec = ts / 1000;
    let mut min = sec / 60;
    sec %= 60;
    let mut hr = min / 60;
    min %= 60;
    let mut days = hr / 24;
    hr %= 24;

    let mut year = 1970i64;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let d = if leap { 366 } else { 365 };
        if days >= d {
            days -= d;
            year += 1;
        } else {
            break;
        }
    }

    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let md = [31u32, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u32;
    let mut day = (days + 1) as u32;
    for d in md {
        if day <= d {
            break;
        }
        day -= d;
        month += 1;
    }

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hr, min, sec)
}

#[component]
pub fn FinanceAttachments() -> Element {
    let mut attachments = use_signal(Vec::<AttachmentDetail>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_add_modal = use_signal(|| false);

    let mut new_file_name = use_signal(String::new);
    let mut new_content = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(|| String::new());

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_attachments().await {
                Ok(list) => attachments.set(list),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

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
                    match list_attachments().await {
                        Ok(list) => attachments.set(list),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let attachments_list = attachments.read().clone();

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    div { class: "flex justify-between items-center mb-4",
                        h2 { class: "card-title", "附件管理" }
                        button { class: "btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 创建文本附件" }
                    }
                    if loading() {
                        Loading {}
                    } else if attachments_list.is_empty() {
                        EmptyState { icon: "📎".to_string(), message: "暂无附件".to_string() }
                    } else {
                        div { class: "overflow-x-auto",
                            table { class: "table table-zebra table-pin-rows",
                                thead { tr { th { "文件名" }, th { "大小" }, th { "用途" }, th { "创建时间" }, th { "操作" } }}
                                tbody {
                                    for a in attachments_list.iter() {
                                        {
                                            let id = a.id.clone();
                                            let original_name = a.original_name.clone();
                                            let size = format_size(a.size);
                                            let purpose = a.purpose.clone();
                                            let created_at = format_timestamp(a.created_at);
                                            let id_delete = id.clone();
                                            rsx! {
                                                tr { key: "{id}",
                                                    td { class: "font-semibold", "{original_name}" }
                                                    td { "{size}" }
                                                    td { span { class: "badge badge-info", "{purpose}" } }
                                                    td { "{created_at}" }
                                                    td {
                                                        button { class: "btn btn-error btn-sm",
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

            Modal {
                title: "创建文本附件".to_string(),
                show: show_add_modal(),
                on_close: move |_| show_add_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                    button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
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
                            toast.error(&format!("删除失败: {}", e));
                        } else {
                            match list_attachments().await {
                                Ok(list) => attachments.set(list),
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
