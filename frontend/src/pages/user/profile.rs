//! 个人信息

use dioxus::prelude::*;

use crate::api::organization::get_current_user_info;
use crate::components::state::{ErrorAlert, Loading, SuccessAlert};

#[component]
pub fn UserProfile() -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut username = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut email = use_signal(String::new());
    let mut role = use_signal(1i32);
    let mut saving = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            match get_current_user_info().await {
                Ok(user) => {
                    username.set(user.username);
                    display_name.set(user.display_name.unwrap_or_default());
                    email.set(user.email.unwrap_or_default());
                    role.set(user.role);
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
                        value: "{match role() { 3 => \"超级管理员\", 2 => \"管理员\", _ => \"成员\" }}" }
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
                // 注意：更新用户信息需要后端 UpdateUserRequest，此处简化
                button { class: "btn btn-accent", disabled: saving(),
                    onclick: move |_| success.set("功能开发中".to_string()),
                    if saving() { "保存中..." } else { "保存" }
                }
            }
        }
    }
}
