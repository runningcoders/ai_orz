//! 组织信息管理
//!
//! 组织基本信息（名称/描述）与组织级配置（如消息向量索引开关）统一在此维护，
//! 便于管理员在同一页面完成组织相关设置。
//!
//! 交互遵循后台管理页惯例：默认查看态（字段只读），点击「编辑」进入编辑态，
//! 编辑态下字段可改、组织级配置开关可切换，并显示「保存 / 取消」。仅组织管理员
//! 可见「编辑」入口；其余成员始终只读。

use dioxus::prelude::*;

use common::api::{OrganizationConfig, UpdateCurrentOrganizationRequest};
use common::enums::UserRole;

use crate::api::organization::{get_current_organization, update_current_organization};
use crate::components::hud::{HudCallout, HudPanel};
use crate::components::state::Loading;
use crate::layouts::app_layout::AppLayout;
use crate::store::auth::use_auth_state;
use crate::store::toast::use_toast;
use crate::utils::status::config_dimension_badge;

#[component]
pub fn OrganizationInfo() -> Element {
    let mut loading = use_signal(|| true);
    // 工作副本（编辑态可改）
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut org_id = use_signal(String::new);
    let mut org_config = use_signal(OrganizationConfig::default);
    // 已落库副本（取消编辑时回滚用）
    let mut saved_name = use_signal(String::new);
    let mut saved_description = use_signal(String::new);
    let mut saved_config = use_signal(OrganizationConfig::default);
    // 编辑态开关
    let mut editing = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let toast = use_toast();
    // 组织级配置（含消息向量索引）仅组织管理员可改；其余成员只读。
    // 依赖 AuthState 正确回填（见 use_require_auth 的 identity 回填修复）。
    let auth = use_auth_state();
    // 组织级配置权限判定走 common 的层级权限方法：满足 Admin 最低权限即可
    // （SuperAdmin 或 Admin），与后端 update_current_organization 门控保持一致。
    let can_edit = UserRole::has_permission(UserRole::from_i32(auth().role), UserRole::Admin);

    use_effect(move || {
        spawn(async move {
            match get_current_organization().await {
                Ok(org) => {
                    let org_name = org.data.name;
                    let org_desc = org.data.description.unwrap_or_default();
                    let org_cfg = org.data.config;
                    name.set(org_name.clone());
                    saved_name.set(org_name);
                    description.set(org_desc.clone());
                    saved_description.set(org_desc);
                    org_id.set(org.data.organization_id);
                    org_config.set(org_cfg.clone());
                    saved_config.set(org_cfg);
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_edit = move |_| {
        // 进入编辑态前先把工作副本同步为已落库值，避免残留上一次半截编辑
        name.set(saved_name());
        description.set(saved_description());
        org_config.set(saved_config());
        editing.set(true);
    };

    let handle_cancel = move |_| {
        // 回滚到已落库值并退出编辑态
        name.set(saved_name());
        description.set(saved_description());
        org_config.set(saved_config());
        editing.set(false);
    };

    let handle_save = move |_| {
        spawn(async move {
            saving.set(true);
            let req = UpdateCurrentOrganizationRequest {
                name: Some(name()),
                description: if description().is_empty() {
                    None
                } else {
                    Some(description())
                },
                base_url: None,
                config: Some(org_config()),
            };
            match update_current_organization(req).await {
                Ok(resp) => {
                    let cfg = resp.data.config;
                    saved_config.set(cfg.clone());
                    org_config.set(cfg);
                    saved_name.set(name());
                    saved_description.set(description());
                    editing.set(false);
                    toast.success("保存成功");
                }
                Err(e) => toast.error(&e),
            }
            saving.set(false);
        });
    };

    rsx! {
        AppLayout {
        HudPanel { signal: Some(true),
            title: Some("组织信息".to_string()),
            actions: Some(rsx! {
                if can_edit {
                    if editing() {
                        button { class: "btn hud-btn btn-ghost btn-sm", disabled: saving(), onclick: handle_cancel,
                            "取消"
                        }
                        button { class: "btn hud-btn btn-primary btn-sm", disabled: saving(), onclick: handle_save,
                            if saving() { "保存中..." } else { "保存" }
                        }
                    } else {
                        button { class: "btn hud-btn btn-primary btn-sm", onclick: handle_edit,
                            "编辑"
                        }
                    }
                }
            }),
            div { class: "card-body",

                if loading() {
                    Loading {}
                } else {
                    div { class: "space-y-4",
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "组织 ID" }
                            }
                            input { class: "input input-bordered w-full", disabled: true, value: "{org_id}" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "组织名称" }
                            }
                            input { class: "input input-bordered w-full", value: "{name}",
                                disabled: !editing(),
                                oninput: move |e| name.set(e.value()) }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "组织描述" }
                            }
                            textarea { class: "textarea textarea-bordered w-full", value: "{description}",
                                disabled: !editing(),
                                oninput: move |e| description.set(e.value()) }
                        }

                        div { class: "hud-divider divider" }

                        // ===== 组织级配置（服务端，跟随当前组织）=====
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium flex items-center gap-2",
                                    "组织级配置"
                                    span { class: config_dimension_badge("org"), "组织级" }
                                }
                            }
                            div { class: "flex items-center justify-between gap-4",
                                div { class: "flex-1",
                                    div { class: "font-medium", "消息向量索引" }
                                    label { class: "label",
                                        span { class: "label-text-alt",
                                            "开启后，普通消息会构建语义向量以支持语义检索；默认关闭以避免无意义的 Embedding 开销。配置仅对当前组织生效。"
                                        }
                                    }
                                }
                                // 统一 HUD 开关：.hud-switch（状态由 .is-on 控制，禁用态降透明度）
                                div { class: "flex shrink-0 items-center gap-3",
                                    span { class: if org_config().enable_message_vector { "text-sm font-medium text-primary" } else { "text-sm font-medium text-base-content/50" },
                                        if org_config().enable_message_vector { "开启" } else { "关闭" }
                                    }
                                    button {
                                        r#type: "button",
                                        class: "hud-switch",
                                        class: if org_config().enable_message_vector { "is-on" } else { "" },
                                        disabled: !can_edit || !editing(),
                                        onclick: move |_| {
                                            let mut c = org_config.write();
                                            c.enable_message_vector = !c.enable_message_vector;
                                        },
                                        span { class: "hud-switch-thumb" }
                                    }
                                }
                            }
                            div { class: "hud-divider divider" }

                            // ===== Agent 入职配置：本组织要求每个 Agent 会哪些包 =====
                            div { class: "space-y-3",
                                div { class: "font-medium", "Agent 入职包（组织要求）" }
                                label { class: "label pt-0",
                                    span { class: "label-text-alt",
                                        "Agent 入职时自动安装这里配置的包：工具包负责工具授权，技能包负责把技能副本放进 Agent 池子。"
                                        "「同名双重身份」的包（如 project_management）两侧都要配 —— 少配一侧会导致工具被拒或技能不进 Prompt。"
                                    }
                                }
                                PackTagEditor {
                                    title: "工具包 tags",
                                    tags: org_config().agent_onboard.required_tool_packs,
                                    disabled: !can_edit || !editing(),
                                    on_add: move |tag: String| {
                                        let mut c = org_config.write();
                                        if !c.agent_onboard.required_tool_packs.contains(&tag) {
                                            c.agent_onboard.required_tool_packs.push(tag);
                                        }
                                    },
                                    on_remove: move |tag: String| {
                                        let mut c = org_config.write();
                                        c.agent_onboard.required_tool_packs.retain(|t| t != &tag);
                                    },
                                }
                                PackTagEditor {
                                    title: "技能包 tags",
                                    tags: org_config().agent_onboard.required_skill_packs,
                                    disabled: !can_edit || !editing(),
                                    on_add: move |tag: String| {
                                        let mut c = org_config.write();
                                        if !c.agent_onboard.required_skill_packs.contains(&tag) {
                                            c.agent_onboard.required_skill_packs.push(tag);
                                        }
                                    },
                                    on_remove: move |tag: String| {
                                        let mut c = org_config.write();
                                        c.agent_onboard.required_skill_packs.retain(|t| t != &tag);
                                    },
                                }
                            }

                            if !can_edit {
                                div { class: "mt-2",
                                    HudCallout { tone: Some("warning".to_string()), extra_class: Some("text-sm".to_string()),
                                        "仅组织管理员可修改组织级配置。"
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

/// 包 tag 编辑器：chips + 回车新增
///
/// 只回调「加/删哪个 tag」，不回调整个列表 —— 避免在闭包里捕获 props 的 `tags`
/// （闭包要 'static，而 `tags` 同时被渲染循环借用）。
#[component]
fn PackTagEditor(
    title: String,
    tags: Vec<String>,
    disabled: bool,
    on_add: Callback<String>,
    on_remove: Callback<String>,
) -> Element {
    let mut input = use_signal(String::new);
    rsx! {
        div { class: "form-control w-full",
            label { class: "label",
                span { class: "label-text", "{title}" }
            }
            div { class: "flex flex-wrap gap-2",
                if tags.is_empty() {
                    span { class: "text-sm text-base-content/50", "未配置" }
                }
                for tag in tags.iter() {
                    span { key: "{tag}", class: "badge orz-tag badge-sm gap-1",
                        "{tag}"
                        if !disabled {
                            button {
                                class: "cursor-pointer",
                                onclick: {
                                    let tag = tag.clone();
                                    move |_| on_remove.call(tag.clone())
                                },
                                "✕"
                            }
                        }
                    }
                }
            }
            input {
                class: "input hud-input w-full",
                disabled: disabled,
                placeholder: "输入包 tag 后回车添加",
                value: "{input}",
                oninput: move |e| input.set(e.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        e.prevent_default();
                        let tag = input().trim().to_string();
                        if tag.is_empty() {
                            return;
                        }
                        on_add.call(tag);
                        input.set(String::new());
                    }
                },
            }
        }
    }
}
