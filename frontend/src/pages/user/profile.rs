//! 个人信息

use dioxus::prelude::*;

use common::api::UpdateCurrentUserRequest;

use crate::api::organization::{get_current_user_info, update_current_user};
use crate::components::state::{ErrorAlert, Loading, SuccessAlert};

#[component]
pub fn UserProfile() -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut username = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut email = use_signal(|| String::new());
    let mut role_name = use_signal(String::new);
    let mut saving = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            match get_current_user_info().await {
                Ok(resp) => {
                    let user = resp.data;
                    username.set(user.username);
                    display_name.set(user.display_name.unwrap_or_default());
                    email.set(user.email.unwrap_or_default());
                    role_name.set(user.role_name);
                }
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }

            div { class: "card-header",
                h2 { class: "card-title", "个人信息" }
            }

            if loading() {
                Loading {}
            } else {
                div { class: "form-group",
                    label { class: "form-label", "用户名" }
                    input { class: "form-input", disabled: true, value: "{username}" }
                }
                div { class: "form-group",
                    label { class: "form-label", "角色" }
                    input { class: "form-input", disabled: true,
                        value: "{role_name}" }
                }
                div { class: "form-group",
                    label { class: "form-label", "显示名称" }
                    input { class: "form-input", value: "{display_name}",
                        oninput: move |e| display_name.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "邮箱" }
                    input { class: "form-input", r#type: "email", value: "{email}",
                        oninput: move |e| email.set(e.value()) }
                }
                button { class: "btn btn-accent", disabled: saving(),
                    onclick: move |_| {
                        success.set(String::new());
                        error.set(String::new());
                        saving.set(true);
                        let display_name_val = display_name();
                        let email_val = email();
                        spawn(async move {
                            let req = UpdateCurrentUserRequest {
                                display_name: Some(display_name_val),
                                email: Some(email_val),
                                password_hash: None,
                            };
                            match update_current_user(req).await {
                                Ok(resp) => {
                                    let user = resp.data;
                                    display_name.set(user.display_name.unwrap_or_default());
                                    email.set(user.email.unwrap_or_default());
                                    success.set("个人信息保存成功".to_string());
                                }
                                Err(e) => error.set(e),
                            }
                            saving.set(false);
                        });
                    },
                    if saving() { "保存中..." } else { "保存" }
                }
            }
        }
    }
}
