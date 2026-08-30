//! 系统设置
//!
//! 仅保留界面偏好（主题外观）与前端配置（后端 API 地址）。
//! 身份凭证管理已迁至 Finance → Identity 子页面（/finance/identity）。
//!
//! 配置维度说明：
//! - 服务级：保存在浏览器 localStorage，跟随当前访问环境（如后端 API 地址）。
//! - 组织级：保存在服务端 organizations 表，跟随当前组织、全局生效（如消息向量索引开关）。

use dioxus::prelude::*;

use common::api::{OrganizationConfig, UpdateCurrentOrganizationRequest};
use common::enums::UserRole;

use crate::api::organization::{get_current_organization, update_current_organization};
use crate::config::FrontendConfig;
use crate::hooks::{AVAILABLE_THEMES, use_theme};
use crate::layouts::app_layout::AppLayout;
use crate::store::auth::use_auth_state;
use crate::store::toast::use_toast;

use crate::components::hud::{HudCallout, HudPanel};
use crate::utils::status::config_dimension_badge;

#[component]
pub fn Settings() -> Element {
    let mut config = use_signal(FrontendConfig::load);
    let toast = use_toast();
    let mut theme_ctrl = use_theme();

    // ===== 组织级配置（服务端，跟随当前组织）=====
    let auth = use_auth_state();
    let is_super_admin =
        UserRole::has_permission(UserRole::from_i32(auth().role), UserRole::SuperAdmin);

    let mut org_config = use_signal(OrganizationConfig::default);
    let mut org_loading = use_signal(|| true);
    let mut org_saving = use_signal(|| false);
    let mut org_loaded = use_signal(|| false);

    // 进入页面拉取一次组织级配置（带 loaded 守卫，避免 use_effect 每次重渲染重复拉取覆盖本地改动）
    use_effect(move || {
        if org_loaded() {
            return;
        }
        org_loaded.set(true);
        org_loading.set(true);
        spawn(async move {
            match get_current_organization().await {
                Ok(resp) => org_config.set(resp.data.config),
                Err(e) => toast.error(format!("加载组织配置失败：{e}")),
            }
            org_loading.set(false);
        });
    });

    let handle_save = move |_| {
        let cfg = config.read().clone();
        match cfg.save() {
            Ok(_) => toast.success("配置已保存"),
            Err(e) => toast.error(&e),
        }
    };

    let handle_reset = move |_| {
        let mut cfg = config.write();
        cfg.reset_to_default();
        drop(cfg);
        // 持久化解除：删除 localStorage 键，回到 origin 动态探测（而非保存 origin 快照）
        match config.read().clear_saved() {
            Ok(_) => toast.success("已清除保存的配置，恢复自动探测后端地址"),
            Err(e) => toast.error(&e),
        }
    };

    // 仅超级管理员可保存组织级配置
    let handle_save_org = move |_| {
        if !is_super_admin {
            return;
        }
        org_saving.set(true);
        let cfg = org_config.read().clone();
        spawn(async move {
            match update_current_organization(UpdateCurrentOrganizationRequest {
                name: None,
                description: None,
                base_url: None,
                config: Some(cfg),
            })
            .await
            {
                Ok(resp) => {
                    org_config.set(resp.data.config);
                    toast.success("组织配置已保存");
                }
                Err(e) => toast.error(format!("保存失败：{e}")),
            }
            org_saving.set(false);
        });
    };

    let current = config.read().clone();

    rsx! {
        AppLayout {
            HudPanel {
                title: "系统设置".to_string(),
                eyebrow: Some("SETTINGS".to_string()),
                signal: Some(true),
                div { class: "card-body",

                    div { class: "hud-divider divider" }

                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "主题外观" }
                        }
                        div { class: "flex flex-wrap gap-2",
                            for (theme_id, theme_name) in AVAILABLE_THEMES.iter().copied() {
                                button {
                                    class: if theme_ctrl.current() == theme_id { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-outline" },
                                    "data-theme": theme_id,
                                    onclick: {
                                        let theme_id = theme_id.to_string();
                                        move |_| theme_ctrl.set(theme_id.clone())
                                    },
                                    "{theme_name}"
                                }
                            }
                        }
                        label { class: "label",
                            span { class: "label-text-alt", "选择喜欢的界面主题，设置自动保存到浏览器" }
                        }
                    }

                    div { class: "hud-divider divider" }

                    div { class: "form-control w-full",
                        label { class: "label",
                                span { class: "label-text font-medium flex items-center gap-2",
                                    "后端 API 地址"
                                    span { class: config_dimension_badge("service"), "服务级" }
                                }
                        }
                        input {
                            class: "input input-bordered hud-input w-full",
                            value: "{current.api_base_url}",
                            oninput: move |e| config.write().api_base_url = e.value(),
                            placeholder: "http://localhost:3000"
                        }
                        label { class: "label",
                            span { class: "label-text-alt", "配置保存在浏览器 localStorage 中，跟随当前访问环境" }
                        }
                    }

                    div { class: "flex gap-3 mt-4",
                        button { class: "btn hud-btn btn-primary", onclick: handle_save, "保存配置" }
                        button { class: "btn hud-btn btn-ghost", onclick: handle_reset, "重置为默认" }
                    }

                    div { class: "hud-divider divider" }

                    // ===== 组织级配置 =====
                    div { class: "form-control w-full",
                        label { class: "label",
                                span { class: "label-text font-medium flex items-center gap-2",
                                    "组织级配置"
                                    span { class: config_dimension_badge("org"), "组织级" }
                                }
                        }

                        if org_loading() {
                            div { class: "flex items-center gap-2 text-sm opacity-60",
                                span { class: "loading loading-spinner loading-sm" }
                                "加载组织配置…"
                            }
                        } else {
                            div { class: "flex items-center justify-between gap-4",
                                div { class: "flex-1",
                                    div { class: "font-medium", "消息向量索引" }
                                    label { class: "label",
                                        span { class: "label-text-alt",
                                            "开启后，普通消息会构建语义向量以支持语义检索；默认关闭以避免无意义的 Embedding 开销。配置仅对当前组织生效。"
                                        }
                                    }
                                }
                                input {
                                    r#type: "checkbox",
                                    class: "toggle toggle-primary",
                                    checked: org_config().enable_message_vector,
                                    disabled: !is_super_admin,
                                    onchange: move |_| {
                                        let mut c = org_config.write();
                                        c.enable_message_vector = !c.enable_message_vector;
                                    },
                                }
                            }

                            if !is_super_admin {
                                HudCallout { tone: Some("warning".to_string()), extra_class: Some("mt-3".to_string()),
                                    span { "仅超级管理员可修改组织级配置。" }
                                }
                            }

                            div { class: "flex gap-3 mt-4",
                                button {
                                    class: if is_super_admin { "btn hud-btn btn-primary" } else { "btn hud-btn btn-primary btn-disabled" },
                                    disabled: !is_super_admin || org_saving(),
                                    onclick: handle_save_org,
                                    if org_saving() {
                                        span { class: "loading loading-spinner loading-xs" }
                                    }
                                    "保存组织配置"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
