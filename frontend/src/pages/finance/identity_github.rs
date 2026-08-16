//! 身份凭证 GitHub 区块（Finance → Identity 子组件）
//!
//! 展示当前用户 GitHub PAT 凭证（token 永不回显，仅尾号）、gh 登录态实测、
//! 默认凭证设置与增删改；Agent 经 gh_cli 工具以该凭证身份操作 GitHub。
//!
//! 数据来源 = `GET /api/v1/finance/identity/github/status` 聚合端点。

use dioxus::prelude::*;

use crate::api::github_integration::{
    create_github_credential, delete_github_credential, get_github_integration_status,
    set_default_github_credential, update_github_credential,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{
    CreateGithubCredentialRequest, GithubIntegrationStatusResponse, UpdateGithubCredentialRequest,
};

/// GitHub 凭证子区块（嵌入 FinanceIdentity 页面）
#[component]
pub fn IdentityGithubSection() -> Element {
    let toast = use_toast();

    // ===== 集成状态 =====
    let mut status = use_signal(|| Option::<GithubIntegrationStatusResponse>::None);
    let mut loading = use_signal(|| true);

    // ===== 录入凭证 =====
    let mut show_create_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_token = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 编辑凭证 =====
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_id = use_signal(String::new);
    let mut edit_name = use_signal(String::new);
    let mut edit_token = use_signal(String::new);
    let mut saving = use_signal(|| false);

    // ===== 删除凭证 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    let refresh = move || {
        spawn(async move {
            loading.set(true);
            match get_github_integration_status().await {
                Ok(s) => status.set(Some(s)),
                Err(e) => toast.error(format!("加载 GitHub 凭证状态失败: {}", e)),
            }
            loading.set(false);
        });
    };
    use_effect(refresh);

    // ===== 录入提交 =====
    let handle_create = move |_| {
        spawn(async move {
            let name = new_name();
            let token = new_token();
            if name.trim().is_empty() || token.trim().is_empty() {
                toast.error("名称 / Token 均为必填");
                return;
            }
            creating.set(true);
            let req = CreateGithubCredentialRequest { name, token };
            match create_github_credential(req).await {
                Ok(_) => {
                    show_create_modal.set(false);
                    new_name.set(String::new());
                    new_token.set(String::new());
                    toast.success("GitHub 凭证绑定成功，Agent 可使用 gh_cli 工具");
                    if let Ok(s) = get_github_integration_status().await {
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
            let req = UpdateGithubCredentialRequest {
                id: id.clone(),
                name: opt(edit_name()),
                token: opt(edit_token()),
            };
            match update_github_credential(req).await {
                Ok(_) => {
                    show_edit_modal.set(false);
                    edit_token.set(String::new());
                    toast.success("凭证已更新，Token 轮换后自动重新登录");
                    if let Ok(s) = get_github_integration_status().await {
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
            match set_default_github_credential(&credential_id).await {
                Ok(_) => {
                    toast.success("默认凭证已更新");
                    if let Ok(s) = get_github_integration_status().await {
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
            match delete_github_credential(&id).await {
                Ok(_) => {
                    toast.success("凭证已删除");
                    if let Ok(s) = get_github_integration_status().await {
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
    let auth = snapshot.as_ref().map(|s| s.auth.clone());
    let auth_name_suffix = auth
        .as_ref()
        .and_then(|a| a.user_name.as_deref())
        .map(|n| format!("（{}）", n))
        .unwrap_or_default();
    let auth_hint = auth.as_ref().and_then(|a| a.hint.clone());

    rsx! {
        // ==================== GitHub 凭证子区块 ====================
        div { class: "border border-base-300 rounded-lg p-4 mt-4",
            div { class: "flex items-center gap-2",
                h3 { class: "font-semibold text-lg", "GitHub" }
                span { class: "badge badge-outline badge-sm", "GithubToken" }
            }
            p { class: "text-xs text-base-content/50 mt-1",
                "Personal Access Token 凭证；Agent 经 gh_cli 工具以此身份操作 GitHub（repo/issue/pr 等）。"
            }

            if loading() && snapshot.is_none() {
                div { class: "text-base-content/50 text-sm py-4", "加载中..." }
            } else {
                // ===== 凭证卡 =====
                div { class: "border border-base-300 rounded-lg p-4 mt-3",
                    div { class: "flex items-center justify-between flex-wrap gap-2",
                        h4 { class: "font-semibold", "访问令牌" }
                        button { class: "btn btn-sm btn-primary", onclick: move |_| show_create_modal.set(true), "+ 绑定令牌" }
                    }
                    if credentials.is_empty() {
                        div { class: "text-sm text-base-content/50 py-3", "尚未绑定 GitHub 令牌" }
                    } else {
                        div { class: "space-y-3 mt-3",
                            for cred in credentials.iter() {
                                {
                                    let credential_id = cred.credential_id.clone();
                                    let cred_name = cred.name.clone();
                                    let token_tail = cred.token_tail.clone();
                                    let is_default = cred.is_default;
                                    let id_for_edit = credential_id.clone();
                                    let id_for_delete = credential_id.clone();
                                    let id_for_default = credential_id.clone();
                                    rsx! {
                                        div { key: "{credential_id}", class: "border border-base-200 rounded p-3",
                                            div { class: "flex items-center justify-between flex-wrap gap-2",
                                                div { class: "flex items-center gap-2 flex-wrap",
                                                    span { class: "font-medium", "{cred_name}" }
                                                    if !token_tail.is_empty() {
                                                        span { class: "badge badge-outline font-mono badge-sm", "****{token_tail}" }
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
                                                            edit_token.set(String::new());
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

                // ===== 登录态卡（gh auth 实测） =====
                div { class: "border border-base-300 rounded-lg p-4 mt-4",
                    div { class: "flex items-center justify-between flex-wrap gap-2",
                        h4 { class: "font-semibold", "登录状态" }
                        if let Some(auth) = &auth {
                            if auth.logged_in {
                                span { class: "badge badge-success", "已登录{auth_name_suffix}" }
                            } else {
                                span { class: "badge badge-ghost", "未登录" }
                            }
                        }
                    }
                    if let Some(hint) = auth_hint {
                        div { class: "alert alert-warning mt-3", span { "{hint}" } }
                    }
                    p { class: "text-xs text-base-content/50 mt-2",
                        "绑定令牌后 Agent 首次调用 gh_cli 时自动登录；登录态隔离在你专属的用户目录下。"
                    }
                }
            }
        }

        // ===== 绑定令牌 Modal =====
        Modal {
            title: "绑定 GitHub 令牌".to_string(),
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
                        oninput: move |e| new_name.set(e.value()), placeholder: "如：工作号" }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "Personal Access Token *" } }
                    input { class: "input input-bordered w-full font-mono", r#type: "password", value: "{new_token}",
                        oninput: move |e| new_token.set(e.value()),
                        placeholder: "ghp_xxx / github_pat_xxx，加密存储，永不回显" }
                    p { class: "text-xs text-base-content/50 mt-1",
                        "在 GitHub → Settings → Developer settings → Personal access tokens 生成；建议仅授予最小必需 scope。"
                    }
                }
            }
        }

        // ===== 编辑凭证 Modal =====
        Modal {
            title: "编辑 GitHub 凭证".to_string(),
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
                    label { class: "label", span { class: "label-text font-medium", "Personal Access Token" } }
                    input { class: "input input-bordered w-full font-mono", r#type: "password", value: "{edit_token}",
                        oninput: move |e| edit_token.set(e.value()), placeholder: "留空保留原值，填写则轮换" }
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除凭证".to_string(),
            message: "删除后 Agent 将无法以此身份操作 GitHub，确定删除？".to_string(),
            on_confirm: handle_delete,
            on_cancel: move |_| show_delete_confirm.set(false),
        }
    }
}
