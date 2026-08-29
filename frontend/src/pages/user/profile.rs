//! 个人信息

use crate::components::hud::HudPanel;
use dioxus::prelude::*;

use common::api::UpdateCurrentUserRequest;

use crate::api::organization::{get_current_user_info, update_current_user};
use crate::components::markdown::MarkdownRenderer;
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
    // 用户自述偏好（声明式画像，Markdown 自由文本）
    let mut preferences = use_signal(String::new);
    // 偏好编辑态开关（false = Markdown 展示态，true = textarea 编辑态）
    let mut editing_prefs = use_signal(|| false);
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
                    preferences.set(user.preferences.unwrap_or_default());
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    rsx! {
        AppLayout {
        HudPanel { signal: Some(true),
            title: Some("个人信息".to_string()),
            div { class: "card-body",

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
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "我的偏好" }
                                span { class: "label-text-alt text-base-content/60",
                                    "向 Agent 声明你的沟通习惯与偏好，支持 Markdown"
                                }
                            }
                            if editing_prefs() {
                                textarea {
                                    class: "textarea textarea-bordered w-full h-40 font-mono text-sm",
                                    placeholder: "例如：回复请用中文、代码注释用英文、汇报要简洁...",
                                    value: "{preferences}",
                                    oninput: move |e| preferences.set(e.value())
                                }
                                button { class: "btn btn-ghost btn-sm mt-1 self-end",
                                    onclick: move |_| editing_prefs.set(false),
                                    "完成编辑"
                                }
                            } else {
                                div { class: "border border-base-300 rounded-lg p-3 min-h-16",
                                    if preferences().is_empty() {
                                        span { class: "text-base-content/50 text-sm", "尚未设置偏好" }
                                    } else {
                                        MarkdownRenderer { content: preferences(), compact: true }
                                    }
                                }
                                button { class: "btn btn-ghost btn-sm mt-1 self-end",
                                    onclick: move |_| editing_prefs.set(true),
                                    "编辑偏好"
                                }
                            }
                        }
                        button { class: "btn btn-primary", disabled: saving(),
                            onclick: move |_| {
                                saving.set(true);
                                let display_name_val = display_name();
                                let email_val = email();
                                let preferences_val = preferences();
                                spawn(async move {
                                    let req = UpdateCurrentUserRequest {
                                        display_name: Some(display_name_val),
                                        email: Some(email_val),
                                        password: None,
                                        preferences: Some(preferences_val),
                                    };
                                    match update_current_user(req).await {
                                        Ok(resp) => {
                                            let user = resp.data;
                                            display_name.set(user.display_name.unwrap_or_default());
                                            email.set(user.email.unwrap_or_default());
                                            preferences.set(user.preferences.unwrap_or_default());
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
