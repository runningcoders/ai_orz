//! 用户管理

use dioxus::prelude::*;

use crate::api::organization::{create_user, delete_user, list_users};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{CreateOrganizationUserRequest, ListUsersResponseItem};

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
                    // Reload
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
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "用户管理" }
                button { class: "btn btn-accent", onclick: move |_| show_modal.set(true), "+ 添加用户" }
            }

            if loading() {
                Loading {}
            } else if users_list.is_empty() {
                EmptyState { icon: "👥".to_string(), message: "暂无用户".to_string() }
            } else {
                table { class: "table",
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
                                let uid_delete = uid.clone();
                                rsx! {
                                    tr { key: "{uid}",
                                        td { class: "detail-table-value-bold", "{uname}" }
                                        td { class: "text-secondary", "{udisplay}" }
                                        td { class: "text-mono text-muted", "{uemail}" }
                                        td { span { class: "{role_badge(urole)}", "{role_text(urole)}" } }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let uid_delete = uid_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_user(&uid_delete).await {
                                                            toast.error(&format!("删除失败: {}", e));
                                                        } else {
                                                            match list_users().await {
                                                                Ok(list) => users.set(list.data),
                                                                Err(e) => toast.error(&e),
                                                            }
                                                        }
                                                    });
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

        Modal {
            title: "添加用户".to_string(),
            show: show_modal(),
            on_close: move |_| show_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "用户名 *" }
                    input { class: "form-input", value: "{new_username}",
                        oninput: move |e| new_username.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "密码 *" }
                    input { class: "form-input", r#type: "password", value: "{new_password}",
                        oninput: move |e| new_password.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "显示名称" }
                    input { class: "form-input", value: "{new_display_name}",
                        oninput: move |e| new_display_name.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "邮箱" }
                    input { class: "form-input", r#type: "email", value: "{new_email}",
                        oninput: move |e| new_email.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "角色" }
                    select { class: "form-select", value: "{new_role}",
                        onchange: move |e| new_role.set(e.value().parse().unwrap_or(1)),
                        option { value: "1", "成员" }
                        option { value: "2", "管理员" }
                        option { value: "3", "超级管理员" }
                    }
                }
            }
        }
    }
}
