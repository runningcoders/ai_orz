//! 组织信息管理

use dioxus::prelude::*;

use crate::api::organization::{get_current_organization, update_current_organization};
use crate::components::state::Loading;
use crate::store::toast::use_toast;
use common::api::UpdateCurrentOrganizationRequest;

#[component]
pub fn OrganizationInfo() -> Element {
    let mut loading = use_signal(|| true);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut org_id = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let toast = use_toast();

    use_effect(move || {
        spawn(async move {
            match get_current_organization().await {
                Ok(org) => {
                    name.set(org.data.name);
                    description.set(org.data.description.unwrap_or_default());
                    org_id.set(org.data.organization_id);
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
                description: if description().is_empty() { None } else { Some(description()) },
                base_url: None,
            };
            match update_current_organization(req).await {
                Ok(_) => toast.success("保存成功"),
                Err(e) => toast.error(&e),
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "组织信息" }
            }

            if loading() {
                Loading {}
            } else {
                div { class: "form-group",
                    label { class: "form-label", "组织 ID" }
                    input { class: "form-input", disabled: true, value: "{org_id}" }
                }
                div { class: "form-group",
                    label { class: "form-label", "组织名称" }
                    input { class: "form-input", value: "{name}",
                        oninput: move |e| name.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "组织描述" }
                    textarea { class: "form-textarea", value: "{description}",
                        oninput: move |e| description.set(e.value()) }
                }
                button { class: "btn btn-accent", disabled: saving(), onclick: handle_save,
                    if saving() { "保存中..." } else { "保存" }
                }
            }
        }
    }
}
