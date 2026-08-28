//! 身份凭证（Finance → Identity）
//!
//! finance domain 下的身份凭证资产主页面：按凭证类型分子区块管理当前用户的凭据。
//! 当前含飞书（应用绑定卡 + 用户身份卡）、GitHub（PAT 凭证 + 登录态）与
//! 通用 API Token（单字段 API Key 类平台按 platform 分 Tab，如 Tavily、豆包搜索）三个子区块；
//! 未来新增微信/Slack 等类型直接加区块。
//!
//! 飞书区块数据来源 = `GET /api/v1/finance/identity/lark/status` 聚合端点（不缓存 localStorage）。
//! GitHub 区块数据来源 = `GET /api/v1/finance/identity/github/status` 聚合端点。
//! 通用 Token 区块数据来源 = `GET /api/v1/finance/identity/generic-token/status?platform=xxx` 聚合端点。

use crate::utils::status::*;
use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::lark_integration::{
    BindPollOutcome, create_lark_credential, delete_lark_credential, get_lark_integration_status,
    judge_bind_status, lark_auth_complete, lark_auth_logout, lark_auth_start, lark_bind_cancel,
    lark_bind_start, lark_bind_status, set_default_lark_credential, update_lark_credential,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::layouts::app_layout::AppLayout;
use crate::pages::finance::identity_generic_token::IdentityGenericTokenSection;
use crate::pages::finance::identity_github::IdentityGithubSection;
use crate::store::toast::use_toast;
use common::api::{
    CreateLarkCredentialRequest, LarkAuthCompleteRequest, LarkAuthStartRequest,
    LarkIntegrationStatusResponse, UpdateLarkCredentialRequest,
};

#[component]
pub fn FinanceIdentity() -> Element {
    let toast = use_toast();

    // ===== 飞书集成状态 =====
    let mut integration = use_signal(|| Option::<LarkIntegrationStatusResponse>::None);
    let mut integration_loading = use_signal(|| true);

    // ===== 手动录入凭证 =====
    let mut show_create_cred_modal = use_signal(|| false);
    let mut new_cred_name = use_signal(String::new);
    let mut new_cred_app_id = use_signal(String::new);
    let mut new_cred_app_secret = use_signal(String::new);
    let mut creating_cred = use_signal(|| false);

    // ===== 编辑凭证 =====
    let mut show_edit_cred_modal = use_signal(|| false);
    let mut edit_cred_id = use_signal(String::new);
    let mut edit_cred_name = use_signal(String::new);
    let mut edit_cred_app_id = use_signal(String::new);
    let mut edit_cred_app_secret = use_signal(String::new);
    let mut saving_cred = use_signal(|| false);

    // ===== 删除凭证 =====
    let mut show_delete_cred_confirm = use_signal(|| false);
    let mut pending_delete_cred_id = use_signal(String::new);

    // ===== 自动绑定（config init --new） =====
    let mut bind_session_id = use_signal(String::new);
    let mut bind_url = use_signal(String::new);
    let mut bind_polling = use_signal(|| false);
    let mut starting_bind = use_signal(|| false);

    // 飞书绑定轮询循环的卸载守卫：组件卸载时置 false，避免 spawn 的 loop 在
    // 绑定进行中离开页面后永久运行（每 3s 打 lark_bind_status + 持有已卸载信号）。
    let bind_poll_running =
        use_signal(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)));
    use_drop(move || {
        bind_poll_running
            .read()
            .store(false, std::sync::atomic::Ordering::SeqCst);
    });

    // ===== 用户授权（device flow） =====
    let mut show_auth_modal = use_signal(|| false);
    let mut auth_device_code = use_signal(String::new);
    let mut auth_url = use_signal(String::new);
    let mut auth_starting = use_signal(|| false);
    let mut auth_completing = use_signal(|| false);

    let refresh_integration = move || {
        spawn(async move {
            integration_loading.set(true);
            match get_lark_integration_status().await {
                Ok(s) => integration.set(Some(s)),
                Err(e) => toast.error(format!("加载飞书凭证状态失败: {}", e)),
            }
            integration_loading.set(false);
        });
    };
    use_effect(refresh_integration);

    // ===== 手动录入凭证提交 =====
    let handle_create_credential = move |_| {
        spawn(async move {
            let name = new_cred_name();
            let app_id = new_cred_app_id();
            let app_secret = new_cred_app_secret();
            if name.trim().is_empty() || app_id.trim().is_empty() || app_secret.trim().is_empty() {
                toast.error("名称 / App ID / App Secret 均为必填");
                return;
            }
            creating_cred.set(true);
            let req = CreateLarkCredentialRequest {
                name,
                app_id,
                app_secret,
                encrypt_key: None,
                verification_token: None,
            };
            match create_lark_credential(req).await {
                Ok(_) => {
                    show_create_cred_modal.set(false);
                    new_cred_name.set(String::new());
                    new_cred_app_id.set(String::new());
                    new_cred_app_secret.set(String::new());
                    toast.success("凭证绑定成功");
                    if let Ok(s) = get_lark_integration_status().await {
                        integration.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("绑定失败: {}", e)),
            }
            creating_cred.set(false);
        });
    };

    // ===== 编辑凭证提交（留空保留原值） =====
    let handle_save_credential = move |_| {
        spawn(async move {
            let id = edit_cred_id();
            saving_cred.set(true);
            let opt = |v: String| {
                if v.trim().is_empty() { None } else { Some(v) }
            };
            let req = UpdateLarkCredentialRequest {
                id: id.clone(),
                name: opt(edit_cred_name()),
                app_id: opt(edit_cred_app_id()),
                app_secret: opt(edit_cred_app_secret()),
                encrypt_key: None,
                verification_token: None,
            };
            match update_lark_credential(req).await {
                Ok(_) => {
                    show_edit_cred_modal.set(false);
                    edit_cred_app_secret.set(String::new());
                    toast.success("凭证已更新，关联渠道将自动重建联");
                    if let Ok(s) = get_lark_integration_status().await {
                        integration.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("更新失败: {}", e)),
            }
            saving_cred.set(false);
        });
    };

    // ===== 设为默认凭证（lark_cli 工具身份优先） =====
    let handle_set_default = move |credential_id: String| {
        spawn(async move {
            match set_default_lark_credential(&credential_id).await {
                Ok(_) => {
                    toast.success("默认凭证已更新");
                    if let Ok(s) = get_lark_integration_status().await {
                        integration.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("设置默认凭证失败: {}", e)),
            }
        });
    };

    // ===== 删除凭证 =====
    let handle_delete_credential = move |_| {
        let id = pending_delete_cred_id();
        show_delete_cred_confirm.set(false);
        spawn(async move {
            match delete_lark_credential(&id).await {
                Ok(_) => {
                    toast.success("凭证已删除");
                    if let Ok(s) = get_lark_integration_status().await {
                        integration.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("删除失败: {}", e)),
            }
        });
    };

    // ===== 自动绑定：发起 + 3s 轮询 =====
    let handle_bind_start = move |_| {
        spawn(async move {
            starting_bind.set(true);
            match lark_bind_start().await {
                Ok(resp) => {
                    bind_session_id.set(resp.session_id.clone());
                    bind_url.set(resp.verification_url.clone());
                    bind_polling.set(true);
                    if resp.verification_url.is_empty() {
                        toast.info("绑定流程已启动，等待验证链接生成（轮询中）");
                    }
                }
                Err(e) => toast.error(format!("发起绑定失败: {}", e)),
            }
            starting_bind.set(false);
        });
        // 轮询任务：3s 一次直到终态
        let running = bind_poll_running.read().clone();
        spawn(async move {
            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                gloo_timers::future::TimeoutFuture::new(3000).await;
                if !bind_polling() {
                    break;
                }
                let session_id = bind_session_id();
                if session_id.is_empty() {
                    break;
                }
                let Ok(resp) = lark_bind_status(&session_id).await else {
                    continue;
                };
                // 补取验证 URL（启动窗口内未抓到时由轮询带回）
                if bind_url().is_empty()
                    && let Some(u) = resp.verification_url.clone().filter(|u| !u.is_empty())
                {
                    bind_url.set(u);
                }
                match judge_bind_status(&resp.status, resp.error.as_deref()) {
                    BindPollOutcome::Continue => {}
                    BindPollOutcome::Done => {
                        bind_polling.set(false);
                        bind_session_id.set(String::new());
                        bind_url.set(String::new());
                        // 分支 B：secret 存 keychain 不可读，引导手动补填凭证
                        toast.success("飞书应用配置完成，请手动补填 App Secret 完成凭证绑定");
                        show_create_cred_modal.set(true);
                        if let Ok(s) = get_lark_integration_status().await {
                            integration.set(Some(s));
                        }
                        break;
                    }
                    BindPollOutcome::Failed(msg) => {
                        bind_polling.set(false);
                        bind_session_id.set(String::new());
                        bind_url.set(String::new());
                        toast.error(format!("绑定失败: {}", msg));
                        break;
                    }
                }
            }
        });
    };

    let handle_bind_cancel = move |_| {
        spawn(async move {
            let session_id = bind_session_id();
            bind_polling.set(false);
            bind_session_id.set(String::new());
            bind_url.set(String::new());
            if !session_id.is_empty() {
                let _ = lark_bind_cancel(&session_id).await;
            }
            toast.info("已取消绑定");
        });
    };

    // ===== 用户授权：device flow =====
    let handle_auth_start = move |_| {
        spawn(async move {
            auth_starting.set(true);
            match lark_auth_start(LarkAuthStartRequest::default()).await {
                Ok(resp) => {
                    auth_device_code.set(resp.device_code);
                    auth_url.set(resp.verification_url.clone());
                    show_auth_modal.set(true);
                }
                Err(e) => toast.error(format!("发起授权失败: {}", e)),
            }
            auth_starting.set(false);
        });
    };

    let handle_auth_complete = move |_| {
        spawn(async move {
            let device_code = auth_device_code();
            auth_completing.set(true);
            match lark_auth_complete(LarkAuthCompleteRequest { device_code }).await {
                Ok(resp) => {
                    show_auth_modal.set(false);
                    if resp.success {
                        toast.success("用户身份授权成功");
                    } else {
                        toast.error(format!(
                            "授权未完成: {}",
                            resp.hint
                                .unwrap_or_else(|| "请确认已在浏览器完成授权".into())
                        ));
                    }
                    if let Ok(s) = get_lark_integration_status().await {
                        integration.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("完成授权失败: {}", e)),
            }
            auth_completing.set(false);
        });
    };

    let handle_auth_logout = move |_| {
        spawn(async move {
            match lark_auth_logout().await {
                Ok(resp) => {
                    if resp.success {
                        toast.success("已取消用户授权");
                    } else {
                        toast.error(format!("取消授权失败: {}", resp.hint.unwrap_or_default()));
                    }
                    if let Ok(s) = get_lark_integration_status().await {
                        integration.set(Some(s));
                    }
                }
                Err(e) => toast.error(format!("取消授权失败: {}", e)),
            }
        });
    };

    let integration_snapshot = integration.read().clone();
    let credentials = integration_snapshot
        .as_ref()
        .map(|s| s.credentials.clone())
        .unwrap_or_default();
    let user_auth = integration_snapshot.as_ref().map(|s| s.user_auth.clone());
    let bind_active = bind_polling();
    let bind_url_value = bind_url();
    let auth_url_value = auth_url();
    let auth_name_suffix = user_auth
        .as_ref()
        .and_then(|a| a.user_name.as_deref())
        .map(|n| format!("（{}）", n))
        .unwrap_or_default();
    let degraded_hint = user_auth
        .as_ref()
        .and_then(|a| a.hint.clone())
        .unwrap_or_else(|| "钥匙串不可用，可继续使用应用身份".to_string());

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    h2 { class: "card-title text-xl mb-2", "身份凭证" }
                    p { class: "text-sm text-base-content/60",
                        "管理你的身份凭证资产；消息渠道通过引用凭证建联推送，凭证是驱动下游环节的关键资产。"
                    }

                    // ==================== 飞书凭证子区块 ====================
                    div { class: "border border-base-300 rounded-lg p-4 mt-4",
                        div { class: "flex items-center gap-2",
                            h3 { class: "font-semibold text-lg", "飞书" }
                            span { class: "badge badge-outline badge-sm", "LarkApp" }
                        }
                        p { class: "text-xs text-base-content/50 mt-1",
                            "飞书自建应用凭证与用户身份授权；消息渠道通过引用凭证接入。"
                        }

                        if integration_loading() && integration_snapshot.is_none() {
                            div { class: "text-base-content/50 text-sm py-4", "加载中..." }
                        } else {
                            // ===== 应用绑定卡 =====
                            div { class: "border border-base-300 rounded-lg p-4 mt-3",
                                div { class: "flex items-center justify-between flex-wrap gap-2",
                                    h4 { class: "font-semibold", "应用绑定" }
                                    div { class: "flex gap-2",
                                        button { class: "btn btn-sm btn-outline", disabled: starting_bind(), onclick: handle_bind_start,
                                            if starting_bind() { "启动中..." } else { "✨ 自动建应用" }
                                        }
                                        button { class: "btn btn-sm btn-primary", onclick: move |_| show_create_cred_modal.set(true), "+ 手动录入凭证" }
                                    }
                                }
                                if bind_active {
                                    div { class: "alert alert-info mt-3",
                                        div { class: "flex flex-col gap-1 w-full",
                                            span { "自动绑定进行中：请在浏览器完成飞书应用配置" }
                                            if !bind_url_value.is_empty() {
                                                a { class: "btn btn-sm btn-primary w-fit", href: "{bind_url_value}", target: "_blank", "打开验证链接" }
                                            } else {
                                                span { class: "text-sm", "验证链接生成中（轮询补取）..." }
                                            }
                                        }
                                        button { class: "btn btn-sm btn-ghost", onclick: handle_bind_cancel, "取消" }
                                    }
                                }
                                if credentials.is_empty() {
                                    div { class: "text-sm text-base-content/50 py-3", "尚未绑定飞书应用凭证" }
                                } else {
                                    div { class: "space-y-3 mt-3",
                                        for cred in credentials.iter() {
                                            {
                                                let credential_id = cred.credential_id.clone();
                                                let cred_name = cred.name.clone();
                                                let app_id = cred.app_id.clone();
                                                let channels = cred.channels.clone();
                                                let is_default = cred.is_default;
                                                let id_for_edit = credential_id.clone();
                                                let id_for_delete = credential_id.clone();
                                                let id_for_default = credential_id.clone();
                                                rsx! {
                                                    div { key: "{credential_id}", class: "border border-base-200 rounded p-3",
                                                        div { class: "flex items-center justify-between flex-wrap gap-2",
                                                            div { class: "flex items-center gap-2 flex-wrap",
                                                                span { class: "font-medium", "{cred_name}" }
                                                                span { class: "badge badge-outline font-mono badge-sm", "{app_id}" }
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
                                                                        edit_cred_id.set(id_for_edit.clone());
                                                                        edit_cred_name.set(String::new());
                                                                        edit_cred_app_id.set(String::new());
                                                                        edit_cred_app_secret.set(String::new());
                                                                        show_edit_cred_modal.set(true);
                                                                    }, "编辑"
                                                                }
                                                                button { class: "btn btn-ghost btn-xs text-error",
                                                                    onclick: move |_| {
                                                                        pending_delete_cred_id.set(id_for_delete.clone());
                                                                        show_delete_cred_confirm.set(true);
                                                                    }, "删除"
                                                                }
                                                            }
                                                        }
                                                        if channels.is_empty() {
                                                            div { class: "text-xs text-base-content/40 mt-2", "暂无渠道引用" }
                                                        } else {
                                                            div { class: "flex gap-2 flex-wrap mt-2",
                                                                span { class: "text-xs text-base-content/60", "关联渠道：" }
                                                                for ch in channels.iter() {
                                                                    {
                                                                        let ch_id = ch.channel_id.clone();
                                                                        let ch_name = ch.channel_name.clone();
                                                                        let enabled_mark = if ch.enabled { "✓" } else { "✗" };
                                                                        rsx! {
                                                                            Link {
                                                                                key: "{ch_id}",
                                                                                class: "btn btn-ghost btn-xs",
                                                                                to: crate::pages::Route::FinanceMessageChannelDetail { id: ch_id },
                                                                                "{ch_name} {enabled_mark}"
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
                                }
                            }

                            // ===== 用户身份卡 =====
                            div { class: "border border-base-300 rounded-lg p-4 mt-4",
                                div { class: "flex items-center justify-between flex-wrap gap-2",
                                    h4 { class: "font-semibold", "用户身份" }
                                    div { class: "flex gap-2 items-center",
                                        if let Some(auth) = &user_auth {
                                            if auth.logged_in {
                                                span { class: "badge badge-sm badge-success",
                                                    "已授权{auth_name_suffix}"
                                                }
                                                button { class: "btn btn-sm btn-ghost", onclick: handle_auth_logout, "取消授权" }
                                            } else {
                                                span { class: "{auth_state_badge(\"未授权\")}", "未授权" }
                                                button { class: "btn btn-sm btn-primary", disabled: auth_starting(), onclick: handle_auth_start,
                                                    if auth_starting() { "发起中..." } else { "授权用户身份" }
                                                }
                                            }
                                        }
                                    }
                                }
                                if let Some(auth) = &user_auth {
                                    if auth.degraded {
                                        div { class: "alert alert-warning mt-3",
                                            span { "{degraded_hint}" }
                                        }
                                    }
                                }
                                p { class: "text-xs text-base-content/50 mt-2",
                                    "用户身份授权后，渠道身份模式为「用户身份」时可代表你操作个人资源（日历/云文档等）"
                                }
                            }
                        }
                    }

                    // ==================== GitHub 凭证子区块 ====================
                    IdentityGithubSection {}

                    // ==================== 通用 API Token 凭证子区块 ====================
                    IdentityGenericTokenSection {}
                }
            }

            // ===== 手动录入凭证 Modal =====
            Modal {
                title: "绑定飞书应用凭证".to_string(),
                show: show_create_cred_modal(),
                on_close: move |_| show_create_cred_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_create_cred_modal.set(false), "取消" }
                    button { class: "btn btn-primary", disabled: creating_cred(), onclick: handle_create_credential,
                        if creating_cred() { "绑定中..." } else { "绑定" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "凭证名称 *" } }
                        input { class: "input input-bordered w-full", value: "{new_cred_name}",
                            oninput: move |e| new_cred_name.set(e.value()), placeholder: "如：我的飞书应用" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "App ID *" } }
                        input { class: "input input-bordered w-full font-mono", value: "{new_cred_app_id}",
                            oninput: move |e| new_cred_app_id.set(e.value()), placeholder: "cli_xxx" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "App Secret *" } }
                        input { class: "input input-bordered w-full font-mono", r#type: "password", value: "{new_cred_app_secret}",
                            oninput: move |e| new_cred_app_secret.set(e.value()), placeholder: "加密存储，永不回显" }
                    }
                }
            }

            // ===== 编辑凭证 Modal =====
            Modal {
                title: "编辑飞书凭证".to_string(),
                show: show_edit_cred_modal(),
                on_close: move |_| show_edit_cred_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_edit_cred_modal.set(false), "取消" }
                    button { class: "btn btn-primary", disabled: saving_cred(), onclick: handle_save_credential,
                        if saving_cred() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "alert alert-info", span { "修改 App ID/Secret 后，关联渠道将自动重建监听连接" } }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "凭证名称" } }
                        input { class: "input input-bordered w-full", value: "{edit_cred_name}",
                            oninput: move |e| edit_cred_name.set(e.value()), placeholder: "留空保持不变" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "App ID" } }
                        input { class: "input input-bordered w-full font-mono", value: "{edit_cred_app_id}",
                            oninput: move |e| edit_cred_app_id.set(e.value()), placeholder: "留空保持不变" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "App Secret" } }
                        input { class: "input input-bordered w-full font-mono", r#type: "password", value: "{edit_cred_app_secret}",
                            oninput: move |e| edit_cred_app_secret.set(e.value()), placeholder: "留空保留原值，填写则覆盖" }
                    }
                }
            }

            // ===== 用户授权 Modal（device flow） =====
            Modal {
                title: "授权用户身份".to_string(),
                show: show_auth_modal(),
                on_close: move |_| show_auth_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_auth_modal.set(false), "取消" }
                    button { class: "btn btn-primary", disabled: auth_completing(), onclick: handle_auth_complete,
                        if auth_completing() { "确认中..." } else { "已完成授权" }
                    }
                },
                div { class: "space-y-4",
                    p { class: "text-sm", "请在浏览器打开以下链接完成飞书授权，完成后点击「已完成授权」：" }
                    a { class: "btn btn-primary w-fit", href: "{auth_url_value}", target: "_blank", "打开授权链接" }
                    div { class: "font-mono text-xs break-all bg-base-200 p-2 rounded", "{auth_url_value}" }
                }
            }

            ConfirmDialog {
                show: show_delete_cred_confirm(),
                title: "确认删除凭证".to_string(),
                message: "删除后引用该凭证的渠道将无法推送，确定删除？".to_string(),
                on_confirm: handle_delete_credential,
                on_cancel: move |_| show_delete_cred_confirm.set(false),
            }
        }
    }
}
