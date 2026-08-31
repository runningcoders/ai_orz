//! 组织信息管理
//!
//! 组织基本信息（名称/描述）与组织级配置（如消息向量索引开关）统一在此维护，
//! 便于管理员在同一页面完成组织相关设置。仅组织管理员可编辑。

use dioxus::prelude::*;

use common::api::{OrganizationConfig, UpdateCurrentOrganizationRequest};

use crate::api::organization::{get_current_organization, update_current_organization};
use crate::components::hud::HudPanel;
use crate::components::state::Loading;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::status::config_dimension_badge;

#[component]
pub fn OrganizationInfo() -> Element {
    let mut loading = use_signal(|| true);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut org_id = use_signal(String::new);
    let mut org_config = use_signal(OrganizationConfig::default);
    let mut saving = use_signal(|| false);
    let toast = use_toast();

    use_effect(move || {
        spawn(async move {
            match get_current_organization().await {
                Ok(org) => {
                    name.set(org.data.name);
                    description.set(org.data.description.unwrap_or_default());
                    org_id.set(org.data.organization_id);
                    org_config.set(org.data.config);
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

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
                    org_config.set(resp.data.config);
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
                                oninput: move |e| name.set(e.value()) }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "组织描述" }
                            }
                            textarea { class: "textarea textarea-bordered w-full", value: "{description}",
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
                                input {
                                    r#type: "checkbox",
                                    class: "toggle toggle-primary",
                                    checked: org_config().enable_message_vector,
                                    onchange: move |_| {
                                        let mut c = org_config.write();
                                        c.enable_message_vector = !c.enable_message_vector;
                                    },
                                }
                            }
                        }

                        button { class: "btn hud-btn btn-primary", disabled: saving(), onclick: handle_save,
                            if saving() { "保存中..." } else { "保存" }
                        }
                    }
                }
            }
        }
        }
    }
}
