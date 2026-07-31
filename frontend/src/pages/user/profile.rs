//! 个人信息

use dioxus::prelude::*;

use common::api::UpdateCurrentUserRequest;

use crate::api::organization::{get_current_user_info, update_current_user};
use crate::components::state::Loading;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

#[component]
pub fn UserProfile() -> Element {
    let mut loading = use_signal(|| true);
    let mut username = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut role_name = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let toast = use_toast();

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
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    rsx! {
        AppLayout {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                h2 { class: "card-title mb-4", "个人信息" }

                if loading() {
                    Loading {}
                } else {
                    div { class: "space-y-4",
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "用户名" }
                            }
                            input { class: "input input-bordered w-full", disabled: true, value: "{username}" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "角色" }
                            }
                            input { class: "input input-bordered w-full", disabled: true,
                                value: "{role_name}" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "显示名称" }
                            }
                            input { class: "input input-bordered w-full", value: "{display_name}",
                                oninput: move |e| display_name.set(e.value()) }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "邮箱" }
                            }
                            input { class: "input input-bordered w-full", r#type: "email", value: "{email}",
                                oninput: move |e| email.set(e.value()) }
                        }
                        button { class: "btn btn-primary", disabled: saving(),
                            onclick: move |_| {
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
                                            toast.success("个人信息保存成功");
                                        }
                                        Err(e) => toast.error(&e),
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
        }
    }
}
