//! 身份凭证 Tavily 区块（Finance → Identity 子组件）
//!
//! 展示当前用户 Tavily API key 凭证（key 永不回显，仅尾号）、默认凭证设置
//! 与增删改，以及实例共享 key 配置状态（双轨授权可见性）；Agent 经
//! tavily_search 工具以该凭证身份检索网络信息。
//!
//! 数据来源 = `GET /api/v1/finance/identity/tavily/status` 聚合端点。

use dioxus::prelude::*;

use crate::api::tavily_integration::{
    create_tavily_credential, delete_tavily_credential, get_tavily_integration_status,
    set_default_tavily_credential, update_tavily_credential,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{
    CreateTavilyCredentialRequest, TavilyIntegrationStatusResponse, UpdateTavilyCredentialRequest,
};

/// Tavily 凭证子区块（嵌入 FinanceIdentity 页面）
#[component]
pub fn IdentityTavilySection() -> Element {
    let toast = use_toast();

    // ===== 集成状态 =====
    let mut status = use_signal(|| Option::<TavilyIntegrationStatusResponse>::None);
    let mut loading = use_signal(|| true);

    // ===== 录入凭证 =====
    let mut show_create_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_api_key = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 编辑凭证 =====
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_id = use_signal(String::new);
    let mut edit_name = use_signal(String::new);
    let mut edit_api_key = use_signal(String::new);
    let mut saving = use_signal(|| false);

    // ===== 删除凭证 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    let refresh = move || {
        spawn(async move {
            loading.set(true);
            match get_tavily_integration_status().await {
                Ok(s) => status.set(Some(s)),
                Err(e) => toast.error(format!("加载 Tavily 凭证状态失败: {}", e)),
            }
            loading.set(false);
        });
    };
    use_effect(refresh);

    // ===== 录入提交 =====
    let handle_create = move |_| {
        spawn(async move {
            let name = new_name();
            let api_key = new_api_key();
            if name.trim().is_empty() || api_key.trim().is_empty() {
                toast.error("名称 / API Key 均为必填");
                return;
            }
            creating.set(true);
            let req = CreateTavilyCredentialRequest { name, api_key };
            match create_tavily_credential(req).await {
                Ok(_) => {
                    show_create_modal.set(false);
                    new_name.set(String::new());
                    new_api_key.set(String::new());
                    toast.success("Tavily 凭证绑定成功，Agent 可使用 tavily_search 工具");
                    if let Ok(s) = get_tavily_integration_status().await {
                        status.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("绑定失败: {}", e)),
            }
            creating.set(false);
        });
    };

    // ===== 编辑提交（留空保留原值） =====
    let handle_save = move |_| {
        spawn(async move {
            let id = edit_id();
            saving.set(true);
            let opt = |v: String| if v.trim().is_empty() { None } else { Some(v) };
            let req = UpdateTavilyCredentialRequest {
                id: id.clone(),
                name: opt(edit_name()),
                api_key: opt(edit_api_key()),
            };
            match update_tavily_credential(req).await {
                Ok(_) => {
                    show_edit_modal.set(false);
                    edit_api_key.set(String::new());
                    toast.success("凭证已更新，下次搜索即生效");
                    if let Ok(s) = get_tavily_integration_status().await {
                        status.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("更新失败: {}", e)),
            }
            saving.set(false);
        });
    };

    // ===== 设为默认 =====
    let handle_set_default = move |credential_id: String| {
        spawn(async move {
            match set_default_tavily_credential(&credential_id).await {
                Ok(_) => {
                    toast.success("默认凭证已更新");
                    if let Ok(s) = get_tavily_integration_status().await {
                        status.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("设置默认凭证失败: {}", e)),
            }
        });
    };

    // ===== 删除 =====
    let handle_delete = move |_| {
        let id = pending_delete_id();
        show_delete_confirm.set(false);
        spawn(async move {
            match delete_tavily_credential(&id).await {
                Ok(_) => {
                    toast.success("凭证已删除");
                    if let Ok(s) = get_tavily_integration_status().await {
                        status.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("删除失败: {}", e)),
            }
        });
    };

    let snapshot = status.read().clone();
    let credentials = snapshot
        .as_ref()
        .map(|s| s.credentials.clone())
        .unwrap_or_default();
    let shared_key_configured = snapshot
        .as_ref()
        .map(|s| s.shared_key_configured)
        .unwrap_or(false);

    rsx! {
        // ==================== Tavily 凭证子区块 ====================
        div { class: "border border-base-300 rounded-lg p-4 mt-4",
            div { class: "flex items-center gap-2",
                h3 { class: "font-semibold text-lg", "Tavily" }
                span { class: "badge badge-outline badge-sm", "TavilyKey" }
            }
            p { class: "text-xs text-base-content/50 mt-1",
                "网络搜索（tavily_search）个人 API key 凭证；未绑定个人 key 时回退实例共享 key。"
            }

            if loading() && snapshot.is_none() {
                div { class: "text-base-content/50 text-sm py-4", "加载中..." }
            } else {
                // ===== 凭证卡 =====
                div { class: "border border-base-300 rounded-lg p-4 mt-3",
                    div { class: "flex items-center justify-between flex-wrap gap-2",
                        h4 { class: "font-semibold", "API Key" }
                        button { class: "btn btn-sm btn-primary", onclick: move |_| show_create_modal.set(true), "+ 绑定 Key" }
                    }
                    if credentials.is_empty() {
                        div { class: "text-sm text-base-content/50 py-3", "尚未绑定个人 Tavily key" }
                    } else {
                        div { class: "space-y-3 mt-3",
                            for cred in credentials.iter() {
                                {
                                    let credential_id = cred.credential_id.clone();
                                    let cred_name = cred.name.clone();
                                    let api_key_tail = cred.api_key_tail.clone();
                                    let is_default = cred.is_default;
                                    let id_for_edit = credential_id.clone();
                                    let id_for_delete = credential_id.clone();
                                    let id_for_default = credential_id.clone();
                                    rsx! {
                                        div { key: "{credential_id}", class: "border border-base-200 rounded p-3",
                                            div { class: "flex items-center justify-between flex-wrap gap-2",
                                                div { class: "flex items-center gap-2 flex-wrap",
                                                    span { class: "font-medium", "{cred_name}" }
                                                    if !api_key_tail.is_empty() {
                                                        span { class: "badge badge-outline font-mono badge-sm", "****{api_key_tail}" }
                                                    }
                                                    if is_default {
                                                        span { class: "badge badge-success badge-sm", "默认" }
                                                    }
                                                }
                                                div { class: "flex gap-2",
                                                    if !is_default {
                                                        button { class: "btn btn-ghost btn-xs",
                                                            onclick: move |_| handle_set_default(id_for_default.clone()),
                                                            "设为默认"
                                                        }
                                                    }
                                                    button { class: "btn btn-ghost btn-xs",
                                                        onclick: move |_| {
                                                            edit_id.set(id_for_edit.clone());
                                                            edit_name.set(String::new());
                                                            edit_api_key.set(String::new());
                                                            show_edit_modal.set(true);
                                                        }, "编辑"
                                                    }
                                                    button { class: "btn btn-ghost btn-xs text-error",
                                                        onclick: move |_| {
                                                            pending_delete_id.set(id_for_delete.clone());
                                                            show_delete_confirm.set(true);
                                                        }, "删除"
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

                // ===== 授权来源卡（双轨可见性） =====
                div { class: "border border-base-300 rounded-lg p-4 mt-4",
                    div { class: "flex items-center justify-between flex-wrap gap-2",
                        h4 { class: "font-semibold", "授权来源" }
                        if shared_key_configured {
                            span { class: "badge badge-success", "个人 key 优先 + 共享 key 兜底" }
                        } else {
                            span { class: "badge badge-ghost", "仅个人 key" }
                        }
                    }
                    if !shared_key_configured && credentials.is_empty() {
                        div { class: "alert alert-warning mt-3",
                            span { "个人 key 与实例共享 key 均未配置，tavily_search 调用将返回引导提示" }
                        }
                    }
                    p { class: "text-xs text-base-content/50 mt-2",
                        "个人 key 优先；未绑定时由管理员在服务端 ai_orz.toml 的 [tavily].api_key 配置共享 key 兜底。"
                    }
                }
            }
        }

        // ===== 绑定 Key Modal =====
        Modal {
            title: "绑定 Tavily Key".to_string(),
            show: show_create_modal(),
            on_close: move |_| show_create_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_create_modal.set(false), "取消" }
                button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "绑定中..." } else { "绑定" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称 *" } }
                    input { class: "input input-bordered w-full", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "如：个人号" }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "API Key *" } }
                    input { class: "input input-bordered w-full font-mono", r#type: "password", value: "{new_api_key}",
                        oninput: move |e| new_api_key.set(e.value()),
                        placeholder: "tvly-xxx，加密存储，永不回显" }
                    p { class: "text-xs text-base-content/50 mt-1",
                        "在 app.tavily.com 注册后于 API Keys 页面生成；免费档每月有搜索配额。"
                    }
                }
            }
        }

        // ===== 编辑凭证 Modal =====
        Modal {
            title: "编辑 Tavily 凭证".to_string(),
            show: show_edit_modal(),
            on_close: move |_| show_edit_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                button { class: "btn btn-primary", disabled: saving(), onclick: handle_save,
                    if saving() { "保存中..." } else { "保存" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称" } }
                    input { class: "input input-bordered w-full", value: "{edit_name}",
                        oninput: move |e| edit_name.set(e.value()), placeholder: "留空保持不变" }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "API Key" } }
                    input { class: "input input-bordered w-full font-mono", r#type: "password", value: "{edit_api_key}",
                        oninput: move |e| edit_api_key.set(e.value()), placeholder: "留空保留原值，填写则轮换" }
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除凭证".to_string(),
            message: "删除后 Agent 将无法以此身份搜索网络，确定删除？".to_string(),
            on_confirm: handle_delete,
            on_cancel: move |_| show_delete_confirm.set(false),
        }
    }
}
