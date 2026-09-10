//! 个人信息

use crate::components::hud::HudPanel;
use dioxus::prelude::*;

use common::api::{GetCurrentUserRequest, UpdateCurrentUserRequest};

use crate::api::organization::{get_current_user_info_with, update_current_user};
use crate::components::markdown::MarkdownRenderer;
use crate::components::state::Loading;
use crate::components::stats::UserStatsPanel;
use crate::components::time_range_picker::{TimeRange, TimeRangePicker};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::models::ModelCallStats;

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
    // 模型调用统计（打点 user_id 匹配口径；一次请求随资料带出）
    let mut model_call_stats = use_signal(|| Option::<ModelCallStats>::None);
    // 统计时间窗口（时间筛选器产出，默认最近 7 天）：变化时 effect 重跑重拉
    let mut stats_range = use_signal(TimeRange::default);
    // 稳定回调（hook 必须在组件顶层无条件调用，不能写在 rsx 条件分支里）
    let on_stats_range = use_callback(move |r: TimeRange| stats_range.set(r));
    let toast = use_toast();

    use_effect(move || {
        // 读取区间建立订阅：切换预设/自定义即重新请求
        let range = stats_range();
        spawn(async move {
            match get_current_user_info_with(GetCurrentUserRequest {
                with_model_call_stats: Some(true),
                stats_time_start: Some(range.start_ms),
                stats_time_end: Some(range.end_ms),
                // 粒度随窗口跨度自适应（≤2 天按小时 / 否则按天）
                stats_interval: Some(range.suggested_interval().to_string()),
            })
            .await
            {
                Ok(resp) => {
                    let user = resp.data;
                    username.set(user.username);
                    display_name.set(user.display_name.unwrap_or_default());
                    email.set(user.email.unwrap_or_default());
                    role_name.set(user.role_name);
                    preferences.set(user.preferences.unwrap_or_default());
                    model_call_stats.set(resp.model_call_stats);
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
                                button { class: "btn hud-btn btn-ghost btn-sm mt-1 self-end",
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
                                button { class: "btn hud-btn btn-ghost btn-sm mt-1 self-end",
                                    onclick: move |_| editing_prefs.set(true),
                                    "编辑偏好"
                                }
                            }
                        }
                        button { class: "btn hud-btn btn-primary", disabled: saving(),
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

                    // 模型调用统计看板（打点 user_id 匹配口径）
                    div { class: "mt-4",
                        div { class: "mb-3",
                            TimeRangePicker { value: stats_range(), on_change: on_stats_range }
                        }
                        UserStatsPanel { model_call_stats: model_call_stats() }
                    }
                }
            }
        }
        }
    }
}
