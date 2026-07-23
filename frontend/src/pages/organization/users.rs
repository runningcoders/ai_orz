//! 用户管理

use dioxus::prelude::*;

use crate::api::organization::{create_user, delete_user, list_users};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{CreateOrganizationUserRequest, ListUsersResponseItem, UpdateUserRequest};

fn role_badge(role: i32) -> &'static str {
    match role {
        3 => "badge badge-info",
        2 => "badge badge-success",
        _ => "badge badge-neutral",
    }
}

fn role_text(role: i32) -> &'static str {
    match role {
        3 => "超级管理员",
        2 => "管理员",
        _ => "成员",
    }
}

#[component]
pub fn OrganizationUsers() -> Element {
    let mut users = use_signal(Vec::<ListUsersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut show_modal = use_signal(|| false);
    let mut new_username = use_signal(String::new);
    let mut new_display_name = use_signal(|| String::new());
    let mut new_email = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut new_role = use_signal(|| 1i32);
    let mut creating = use_signal(|| false);
    let toast = use_toast();

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(|| String::new());

    // ===== 编辑用户 Modal =====
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_user_id = use_signal(String::new);
    let mut edit_display_name = use_signal(String::new);
    let mut edit_email = use_signal(String::new);
    let mut edit_role = use_signal(|| "1".to_string());
    let mut edit_status = use_signal(|| "1".to_string());
    let mut edit_password = use_signal(String::new);
    let mut saving_user = use_signal(|| false);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_users().await {
                Ok(list) => users.set(list.data),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_username().is_empty() || new_password().is_empty() {
                toast.error("用户名和密码不能为空");
                return;
            }
            creating.set(true);
            let req = CreateOrganizationUserRequest {
                username: new_username(),
                password_hash: new_password(),
                display_name: if new_display_name().is_empty() { None } else { Some(new_display_name()) },
                email: if new_email().is_empty() { None } else { Some(new_email()) },
                role: new_role(),
            };
            match create_user(req).await {
                Ok(_) => {
                    show_modal.set(false);
                    new_username.set(String::new());
                    new_display_name.set(String::new());
                    new_email.set(String::new());
                    new_password.set(String::new());
                    new_role.set(1);
                    match list_users().await {
                        Ok(list) => users.set(list.data),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let users_list = users.read().clone();

    rsx! {
        AppLayout {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                div { class: "flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4 mb-4",
                    h2 { class: "card-title", "用户管理" }
                    button { class: "btn btn-primary", onclick: move |_| show_modal.set(true), "+ 添加用户" }
                }

                if loading() {
                    Loading {}
                } else if users_list.is_empty() {
                    EmptyState { icon: "👥".to_string(), message: "暂无用户".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table table-zebra table-pin-rows",
                            thead { tr {
                                th { "用户名" }
                                th { "显示名称" }
                                th { "邮箱" }
                                th { "角色" }
                                th { "操作" }
                            }}
                            tbody {
                                for u in users_list.iter() {
                                    {
                                        let uid = u.user_id.clone();
                                        let uname = u.username.clone();
                                        let udisplay = u.display_name.clone().unwrap_or_default();
                                        let uemail = u.email.clone().unwrap_or_default();
                                        let urole = u.role;
                                        let ustatus = u.status;
                                        let uid_delete = uid.clone();
                                        let uid_edit = uid.clone();
                                        let udisplay_edit = udisplay.clone();
                                        let uemail_edit = uemail.clone();
                                        rsx! {
                                            tr { key: "{uid}",
                                                td { class: "font-semibold", "data-label": "用户名", "{uname}" }
                                                td { class: "text-base-content/70", "data-label": "显示名称", "{udisplay}" }
                                                td { class: "font-mono text-sm text-base-content/70", "data-label": "邮箱", "{uemail}" }
                                                td { "data-label": "角色", span { class: "{role_badge(urole)}", "{role_text(urole)}" } }
                                                td { "data-label": "操作",
                                                    button { class: "btn btn-ghost btn-xs mr-1",
                                                        onclick: move |_| {
                                                            edit_user_id.set(uid_edit.clone());
                                                            edit_display_name.set(udisplay_edit.clone());
                                                            edit_email.set(uemail_edit.clone());
                                                            edit_role.set(urole.to_string());
                                                            edit_status.set(ustatus.to_string());
                                                            edit_password.set(String::new());
                                                            show_edit_modal.set(true);
                                                        },
                                                        "编辑"
                                                    }
                                                    button { class: "btn btn-error btn-sm",
                                                        onclick: move |_| {
                                                            pending_delete_id.set(uid_delete.clone());
                                                            show_delete_confirm.set(true);
                                                        },
                                                        "删除"
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
            title: "添加用户".to_string(),
            show: show_modal(),
            on_close: move |_| show_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "用户名 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_username}",
                        oninput: move |e| new_username.set(e.value()) }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "密码 *" }
                    }
                    input { class: "input input-bordered w-full", r#type: "password", value: "{new_password}",
                        oninput: move |e| new_password.set(e.value()) }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "显示名称" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_display_name}",
                        oninput: move |e| new_display_name.set(e.value()) }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "邮箱" }
                    }
                    input { class: "input input-bordered w-full", r#type: "email", value: "{new_email}",
                        oninput: move |e| new_email.set(e.value()) }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "角色" }
                    }
                    select { class: "select select-bordered w-full", value: "{new_role}",
                        onchange: move |e| new_role.set(e.value().parse().unwrap_or(1)),
                        option { value: "1", "成员" }
                        option { value: "2", "管理员" }
                        option { value: "3", "超级管理员" }
                    }
                }
            }
        }

        Modal {
            title: "编辑用户".to_string(),
            show: show_edit_modal(),
            on_close: move |_| show_edit_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                button {
                    class: "btn btn-primary",
                    disabled: saving_user(),
                    onclick: move |_| {
                        let display_name = if edit_display_name().trim().is_empty() { None } else { Some(edit_display_name()) };
                        let email = if edit_email().trim().is_empty() { None } else { Some(edit_email()) };
                        let role: i32 = edit_role().trim().parse().unwrap_or(1);
                        let status: i32 = edit_status().trim().parse().unwrap_or(1);
                        let password_hash = if edit_password().is_empty() { None } else { Some(edit_password()) };
                        let req = UpdateUserRequest {
                            user_id: edit_user_id(),
                            display_name,
                            email,
                            role: Some(role),
                            status: Some(status),
                            password_hash,
                        };
                        saving_user.set(true);
                        spawn(async move {
                            match crate::api::organization::update_user(req).await {
                                Ok(_) => {
                                    toast.success("用户已更新");
                                    show_edit_modal.set(false);
                                    match list_users().await {
                                        Ok(list) => users.set(list.data),
                                        Err(e) => toast.error(&format!("重新加载失败: {}", e)),
                                    }
                                }
                                Err(e) => toast.error(&format!("更新失败: {}", e)),
                            }
                            saving_user.set(false);
                        });
                    },
                    if saving_user() { "保存中..." } else { "保存" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "显示名称" }
                    }
                    input { class: "input input-bordered w-full", value: "{edit_display_name}",
                        oninput: move |e| edit_display_name.set(e.value()) }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "邮箱" }
                    }
                    input { class: "input input-bordered w-full", r#type: "email", value: "{edit_email}",
                        oninput: move |e| edit_email.set(e.value()) }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "角色" }
                    }
                    select {
                        class: "select select-bordered w-full",
                        value: "{edit_role}",
                        onchange: move |e| edit_role.set(e.value()),
                        option { value: "1", "成员" }
                        option { value: "2", "管理员" }
                        option { value: "3", "超级管理员" }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "状态" }
                    }
                    select {
                        class: "select select-bordered w-full",
                        value: "{edit_status}",
                        onchange: move |e| edit_status.set(e.value()),
                        option { value: "1", "正常" }
                        option { value: "0", "禁用" }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "新密码（留空不修改）" }
                    }
                    input { class: "input input-bordered w-full", r#type: "password", value: "{edit_password}",
                        oninput: move |e| edit_password.set(e.value()), placeholder: "输入新密码或留空" }
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除".to_string(),
            message: "确定删除此用户？此操作不可撤销。".to_string(),
            on_confirm: move |_| {
                let id = pending_delete_id();
                show_delete_confirm.set(false);
                spawn(async move {
                    if let Err(e) = delete_user(&id).await {
                        toast.error(&format!("删除失败: {}", e));
                    } else {
                        match list_users().await {
                            Ok(list) => users.set(list.data),
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
