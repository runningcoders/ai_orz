//! 前台接待 - 系统初始化 + 登录

use dioxus::prelude::*;

use crate::api::auth::{
    check_initialized, initialize_system, login, register_by_invite, validate_invite_code,
};
use crate::api::organization::{get_current_user_info, list_organizations_public};
use crate::api::seed::get_task_progress;
use crate::components::state::Loading;
use crate::components::task_progress::TaskProgress;
use crate::store::auth::{AuthState, mark_logged_in, save_role};
use crate::store::toast::use_toast;
use common::api::{
    InitializeSystemRequest, LoginRequest, OrganizationListItem, RegisterByInviteRequest,
    TaskProgressSnapshot, TaskStatus,
};

#[component]
pub fn Reception() -> Element {
    let mut loading = use_signal(|| true);
    let mut initialized = use_signal(|| false);
    let mut organizations = use_signal(Vec::<OrganizationListItem>::new);
    let toast = use_toast();

    // 登录表单
    let mut selected_org_id = use_signal(String::new);
    let mut login_username = use_signal(String::new);
    let mut login_password = use_signal(String::new);
    let mut login_submitting = use_signal(|| false);

    // 注册表单
    let mut show_register = use_signal(|| false); // false=登录 Tab, true=注册 Tab
    let mut reg_invite_code = use_signal(String::new);
    let mut reg_username = use_signal(String::new);
    let mut reg_password = use_signal(String::new);
    let mut reg_display_name = use_signal(String::new);
    let mut reg_submitting = use_signal(|| false);
    let mut invite_valid = use_signal(|| Option::<(bool, String)>::None); // Some((is_valid, org_name)) 或 None=未校验

    // 初始化表单
    let mut org_name = use_signal(String::new);
    let mut org_description = use_signal(String::new);
    let mut init_username = use_signal(String::new);
    let mut init_password = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut init_submitting = use_signal(|| false);
    let mut init_progress = use_signal(|| Option::<TaskProgressSnapshot>::None);

    // 两步表单状态
    let mut init_step = use_signal(|| 1u8); // 1=基础信息 2=模型配置

    // 对话模型配置（可选 — 可跳过，创建 Agent 前补配）
    let mut enable_chat = use_signal(|| true); // 默认启用
    let mut chat_provider_name = use_signal(String::new);
    let mut chat_provider_type = use_signal(|| 0i32); // 0=OpenAI
    let mut chat_model_name = use_signal(String::new);
    let mut chat_api_key = use_signal(String::new);
    let mut chat_base_url = use_signal(String::new);

    // 向量模型配置（可选）
    let mut enable_embedding = use_signal(|| true); // 默认启用
    let mut embedding_provider_name = use_signal(String::new);
    let mut embedding_provider_type = use_signal(|| 6i32); // 6=FastEmbed
    let mut embedding_model_name = use_signal(String::new);
    let mut embedding_api_key = use_signal(String::new);
    let mut embedding_base_url = use_signal(String::new);

    let mut auth = use_context::<Signal<AuthState>>();

    // 页面加载检查初始化状态
    use_effect(move || {
        spawn(async move {
            match check_initialized().await {
                Ok(resp) => {
                    if resp.initialized {
                        match list_organizations_public().await {
                            Ok(list) => {
                                organizations.set(list.data);
                                initialized.set(true);
                            }
                            Err(e) => toast.error(&e),
                        }
                    } else {
                        initialized.set(false);
                    }
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    // 登录提交
    let on_submit_login = move |_| {
        spawn(async move {
            if selected_org_id().is_empty() {
                toast.error("请先选择一个组织");
                return;
            }
            if login_username().is_empty() || login_password().is_empty() {
                toast.error("用户名和密码不能为空");
                return;
            }
            login_submitting.set(true);

            let req = LoginRequest {
                organization_id: selected_org_id(),
                username: login_username(),
                password_hash: login_password(),
            };

            match login(req).await {
                Ok(resp) => {
                    mark_logged_in();
                    let mut state = auth.write();
                    state.logged_in = true;
                    state.user_id = resp.user_id.clone();
                    state.username = resp.username.clone();
                    state.display_name = resp.display_name.clone();
                    state.org_id = resp.organization_id.clone();
                    // 修复 R-M1：之前硬编码 role=1 导致管理员登录后看不到管理菜单。
                    // LoginResponse 不含 role，登录后立即调用 /user/me 获取真实 role。
                    drop(state);
                    match get_current_user_info().await {
                        Ok(user_info) => {
                            let mut state = auth.write();
                            state.role = user_info.data.role;
                            state.user_id = user_info.data.user_id.clone();
                            state.username = user_info.data.username.clone();
                            state.display_name =
                                user_info.data.display_name.clone().unwrap_or_default();
                            state.org_id = user_info.data.organization_id.clone();
                            save_role(user_info.data.role);
                            drop(state);
                        }
                        Err(_) => {
                            // 获取用户信息失败，仍允许登录（role 保持默认 0）
                            // use_require_auth 会再次尝试回填
                        }
                    }
                    // 修复 L_NEW：web_sys::window() 在 non-Window 环境（如 SSR）返回 None，
                    // unwrap 会 panic。改为 if let 安全处理
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href("/");
                    }
                }
                Err(e) => {
                    toast.error(&e);
                    login_submitting.set(false);
                }
            }
        });
    };

    // 邀请码实时校验（debounce：输入停止 400ms 后触发）
    let mut validate_debounce = use_signal(|| 0u64);
    let mut on_invite_code_input = move |_| {
        let code = reg_invite_code().trim().to_string();
        invite_valid.set(None);
        if code.is_empty() {
            return;
        }
        validate_debounce.set(validate_debounce() + 1);
        let snapshot = validate_debounce();
        spawn(async move {
            gloo_timers::future::sleep(std::time::Duration::from_millis(400)).await;
            if snapshot != validate_debounce() {
                return;
            }
            match validate_invite_code(&code).await {
                Ok(resp) => {
                    invite_valid.set(Some((
                        resp.valid,
                        resp.organization_name.unwrap_or_default(),
                    )));
                }
                Err(e) => {
                    // 网络/服务异常不能等同"码无效"：提示错误并复位为未校验，
                    // 用户继续输入或重新聚焦即可再次触发
                    toast.error(&e);
                    invite_valid.set(None);
                }
            }
        });
    };

    // 注册提交
    let on_submit_register = move |_| {
        spawn(async move {
            let code = reg_invite_code().trim().to_string();
            if code.is_empty() {
                toast.error("请输入邀请码");
                return;
            }
            // 前端已明确校验为无效时直接拦截，省一次无效往返；
            // 未校验/校验中仍放行 —— 后端是权威校验方
            if matches!(invite_valid(), Some((false, _))) {
                toast.error("邀请码无效或已过期，请检查后重试");
                return;
            }
            if reg_username().trim().is_empty() || reg_password().is_empty() {
                toast.error("用户名和密码不能为空");
                return;
            }
            if reg_password().len() < 6 {
                toast.error("密码至少 6 位");
                return;
            }
            reg_submitting.set(true);

            let req = RegisterByInviteRequest {
                invite_code: code,
                username: reg_username(),
                password_hash: reg_password(),
                display_name: if reg_display_name().is_empty() {
                    None
                } else {
                    Some(reg_display_name())
                },
            };

            match register_by_invite(req).await {
                Ok(resp) => {
                    mark_logged_in();
                    let mut state = auth.write();
                    state.logged_in = true;
                    state.user_id = resp.user_id.clone();
                    state.username = resp.username.clone();
                    state.display_name = resp.display_name.clone();
                    state.org_id = resp.organization_id.clone();
                    drop(state);
                    // 注册后立即获取完整用户信息
                    if let Ok(user_info) = get_current_user_info().await {
                        let mut state = auth.write();
                        state.role = user_info.data.role;
                        state.user_id = user_info.data.user_id.clone();
                        state.username = user_info.data.username.clone();
                        state.display_name =
                            user_info.data.display_name.clone().unwrap_or_default();
                        state.org_id = user_info.data.organization_id.clone();
                        save_role(user_info.data.role);
                        drop(state);
                    }
                    toast.success("注册成功！欢迎加入");
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href("/");
                    }
                }
                Err(e) => {
                    toast.error(&e);
                    reg_submitting.set(false);
                }
            }
        });
    };

    // 向导导航：Step 1「下一步」仅校验基础信息区
    let on_next_step = move |_| {
        if org_name().is_empty() || init_username().is_empty() || init_password().is_empty() {
            toast.error("组织名称、用户名、密码不能为空");
            return;
        }
        init_step.set(2);
    };

    let on_prev_step = move |_| {
        init_step.set(1);
    };

    // 初始化提交
    let on_submit_init = move |_| {
        spawn(async move {
            // 基础信息已在 Step 1 校验，这里仅校验模型区
            if enable_chat()
                && (chat_provider_name().is_empty()
                    || chat_model_name().is_empty()
                    || chat_api_key().is_empty())
            {
                toast.error("对话模型的 Provider 名称、模型名称、API Key 不能为空");
                return;
            }
            if enable_embedding()
                && (embedding_provider_name().is_empty() || embedding_model_name().is_empty())
            {
                toast.error("向量模型的 Provider 名称、模型名称不能为空");
                return;
            }
            init_submitting.set(true);

            let req = InitializeSystemRequest {
                organization_name: org_name(),
                description: if org_description().is_empty() {
                    None
                } else {
                    Some(org_description())
                },
                admin_username: init_username(),
                admin_password_hash: init_password(),
                admin_display_name: if display_name().is_empty() {
                    None
                } else {
                    Some(display_name())
                },
                admin_email: if email().is_empty() {
                    None
                } else {
                    Some(email())
                },
                chat_model: if enable_chat() {
                    Some(common::api::ModelProviderInitConfig {
                        name: chat_provider_name(),
                        provider_type: chat_provider_type(),
                        model_name: chat_model_name(),
                        api_key: chat_api_key(),
                        base_url: if chat_base_url().is_empty() {
                            None
                        } else {
                            Some(chat_base_url())
                        },
                        description: None,
                    })
                } else {
                    None
                },
                embedding_model: if enable_embedding() {
                    Some(common::api::ModelProviderInitConfig {
                        name: embedding_provider_name(),
                        provider_type: embedding_provider_type(),
                        model_name: embedding_model_name(),
                        api_key: embedding_api_key(),
                        base_url: if embedding_base_url().is_empty() {
                            None
                        } else {
                            Some(embedding_base_url())
                        },
                        description: None,
                    })
                } else {
                    None
                },
            };

            match initialize_system(req).await {
                Ok(resp) => {
                    // 异步初始化：保存 task_id，启动进度轮询
                    let task_id = resp.task_id;
                    init_progress.set(Some(TaskProgressSnapshot {
                        task_id: task_id.clone(),
                        task_type: "initialize_system".to_string(),
                        status: TaskStatus::Pending,
                        current_step: 0,
                        total_steps: 0,
                        step_message: "正在启动...".to_string(),
                        started_at: 0,
                        finished_at: None,
                        error: None,
                        result: None,
                    }));
                    // 启动轮询（统一接口）
                    spawn(async move {
                        loop {
                            gloo_timers::future::TimeoutFuture::new(300).await;
                            match get_task_progress(&task_id).await {
                                Ok(progress) => {
                                    let is_completed = progress.status == TaskStatus::Completed;
                                    let is_failed = progress.status == TaskStatus::Failed;
                                    if is_completed {
                                        // 初始化完成：提示已创建预设前台 Agent，稍作停留后刷新进入登录
                                        let has_reception = progress
                                            .result
                                            .as_ref()
                                            .and_then(|r| r.get("reception_agent_id"))
                                            .and_then(|v| v.as_str())
                                            .map(|s| !s.is_empty())
                                            .unwrap_or(false);
                                        init_progress.set(Some(progress));
                                        if has_reception {
                                            toast.success("初始化完成！已为你创建「前台接待」Agent，登录后即可开始对话");
                                        } else {
                                            toast.success("初始化完成！可登录后在「Agent 管理」中创建前台 Agent");
                                        }
                                        spawn(async move {
                                            gloo_timers::future::TimeoutFuture::new(1200).await;
                                            if let Some(window) = web_sys::window() {
                                                let _ = window.location().reload();
                                            }
                                        });
                                        break;
                                    }
                                    if is_failed {
                                        init_submitting.set(false);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    toast.error(&e);
                                    init_submitting.set(false);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    toast.error(&e);
                    init_submitting.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "reception-page",
            // 左侧品牌展示区
            div { class: "reception-brand",
                div { class: "reception-brand-content",
                    div { class: "reception-brand-logo",
                        div { class: "reception-brand-logo-mark", "Orz" }
                        span { class: "reception-brand-logo-text", "AI Orz" }
                    }

                    h1 { class: "reception-brand-headline",
                        "让 AI Agent "
                        span { class: "reception-brand-headline-accent", "协同工作" }
                        br {}
                        "完成复杂任务"
                    }

                    p { class: "reception-brand-subtitle",
                        "全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务。"
                    }

                    div { class: "reception-brand-features",
                        div { class: "reception-brand-feature",
                            div { class: "reception-brand-feature-icon", "🤝" }
                            div { class: "reception-brand-feature-text",
                                strong { "多 Agent 协作" }
                                "Agent 间消息通信、任务分配、技能共享"
                            }
                        }
                        div { class: "reception-brand-feature",
                            div { class: "reception-brand-feature-icon", "🧠" }
                            div { class: "reception-brand-feature-text",
                                strong { "四层记忆系统" }
                                "核心认知、工作记忆、短期摘要、长期知识图谱"
                            }
                        }
                        div { class: "reception-brand-feature",
                            div { class: "reception-brand-feature-icon", "🛠️" }
                            div { class: "reception-brand-feature-text",
                                strong { "混合模式工具调用" }
                                "MCP 集成、工具包机制、神经工具免绑定"
                            }
                        }
                        div { class: "reception-brand-feature",
                            div { class: "reception-brand-feature-icon", "🔎" }
                            div { class: "reception-brand-feature-text",
                                strong { "综合搜索引擎" }
                                "FTS5 关键词 + 向量语义 + 图谱关系三态匹配"
                            }
                        }
                    }
                }
            }

            // 右侧表单区
            div { class: "reception-form-side",
                div { class: "reception-form-card",
                    if loading() {
                        Loading {}
                    } else {
                        if initialized() {
                            // 已初始化：登录 / 注册 Tab
                            div { class: "reception-form-header",
                                h2 { class: "reception-form-title", "欢迎回来" }
                            }

                            // Tab 切换
                            div { class: "reception-tabs",
                                div {
                                    class: if !show_register() { "reception-tab active" } else { "reception-tab" },
                                    onclick: move |_| show_register.set(false),
                                    "登录"
                                }
                                div {
                                    class: if show_register() { "reception-tab active" } else { "reception-tab" },
                                    onclick: move |_| show_register.set(true),
                                    "注册"
                                }
                            }

                            if !show_register() {
                                // === 登录 Tab ===
                                div { class: "reception-form-desc", "选择组织并登录您的账户" }

                                // 组织列表
                                div { class: "reception-org-list",
                                    for org in organizations() {
                                        {
                                            let is_selected = selected_org_id() == org.organization_id;
                                            let class = if is_selected { "reception-org-item selected" } else { "reception-org-item" };
                                            rsx! {
                                                div {
                                                    key: "{org.organization_id}",
                                                    class: "{class}",
                                                    onclick: move |_| selected_org_id.set(org.organization_id.clone()),
                                                    div { class: "reception-org-name", "{org.name}" }
                                                    if let Some(desc) = &org.description {
                                                        p { class: "reception-org-desc", "{desc}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                form { onsubmit: move |e| { e.prevent_default(); on_submit_login(e); },
                                    div { class: "form-control w-full",
                                        label { class: "form-label", "用户名" }
                                        input {
                                            class: "input input-bordered w-full",
                                            r#type: "text",
                                            "data-testid": "login-username",
                                            value: "{login_username}",
                                            oninput: move |e| login_username.set(e.value()),
                                            placeholder: "请输入用户名",
                                        }
                                    }
                                    div { class: "form-control w-full",
                                        label { class: "form-label", "密码" }
                                        input {
                                            class: "input input-bordered w-full",
                                            r#type: "password",
                                            "data-testid": "login-password",
                                            value: "{login_password}",
                                            oninput: move |e| login_password.set(e.value()),
                                            placeholder: "请输入密码",
                                        }
                                    }
                                    button {
                                        class: "btn btn-primary btn-lg w-full",
                                        r#type: "submit",
                                        "data-testid": "login-submit",
                                        disabled: login_submitting(),
                                        if login_submitting() { "登录中..." } else { "登录" }
                                    }
                                }
                            } else {
                                // === 注册 Tab ===
                                div { class: "reception-form-desc", "使用邀请码加入一个组织" }

                                form { onsubmit: move |e| { e.prevent_default(); on_submit_register(e); },
                                    // 邀请码
                                    div { class: "form-control w-full",
                                        label { class: "form-label", "邀请码" }
                                        input {
                                            class: "input input-bordered w-full",
                                            r#type: "text",
                                            "data-testid": "reg-invite-code",
                                            value: "{reg_invite_code}",
                                            oninput: move |e| { reg_invite_code.set(e.value()); on_invite_code_input(()); },
                                            placeholder: "请输入组织邀请码",
                                        }
                                        // 邀请码校验反馈
                                        if let Some((valid, org_name)) = &invite_valid() {
                                            if *valid {
                                                div { class: "reception-invite-ok", "✓ 邀请码有效，将加入「{org_name}」" }
                                            } else {
                                                div { class: "reception-invite-bad", "✗ 邀请码无效或已过期" }
                                            }
                                        }
                                    }
                                    div { class: "form-control w-full",
                                        label { class: "form-label", "用户名" }
                                        input {
                                            class: "input input-bordered w-full",
                                            r#type: "text",
                                            "data-testid": "reg-username",
                                            value: "{reg_username}",
                                            oninput: move |e| reg_username.set(e.value()),
                                            placeholder: "请输入用户名",
                                        }
                                    }
                                    div { class: "form-control w-full",
                                        label { class: "form-label", "密码（至少 6 位）" }
                                        input {
                                            class: "input input-bordered w-full",
                                            r#type: "password",
                                            "data-testid": "reg-password",
                                            value: "{reg_password}",
                                            oninput: move |e| reg_password.set(e.value()),
                                            placeholder: "请输入密码",
                                        }
                                    }
                                    div { class: "form-control w-full",
                                        label { class: "form-label", "显示名（可选）" }
                                        input {
                                            class: "input input-bordered w-full",
                                            r#type: "text",
                                            "data-testid": "reg-display-name",
                                            value: "{reg_display_name}",
                                            oninput: move |e| reg_display_name.set(e.value()),
                                            placeholder: "不填则使用用户名",
                                        }
                                    }
                                    button {
                                        class: "btn btn-primary btn-lg w-full",
                                        r#type: "submit",
                                        "data-testid": "reg-submit",
                                        disabled: reg_submitting(),
                                        if reg_submitting() { "注册中..." } else { "注册" }
                                    }
                                }
                            }
                        } else {
                            // 未初始化：初始化表单或进度条
                            div { class: "reception-form-header",
                                h2 { class: "reception-form-title", "系统初始化" }
                                p { class: "reception-form-desc", "创建您的第一个组织和超级管理员" }
                            }

                            if init_submitting() {
                                // 初始化进度条（使用通用 TaskProgress 组件）
                                if let Some(p) = &init_progress() {
                                    TaskProgress {
                                        progress: p.clone(),
                                        on_cancel: move |_| {
                                            init_submitting.set(false);
                                            init_progress.set(None);
                                        },
                                    }
                                } else {
                                    div { class: "init-progress-container",
                                        div { class: "init-progress-spinner" }
                                        p { "正在提交..." }
                                    }
                                }
                            } else {
                                // 初始化表单
                                form { onsubmit: move |e| { e.prevent_default(); on_submit_init(e); },
                                    // ===== 步骤指示器 =====
                                    ul { class: "steps steps-primary w-full mb-2",
                                        "data-testid": "init-wizard-steps",
                                        li { class: "step step-primary", "基础信息" }
                                        li {
                                            class: if init_step() >= 2 { "step step-primary" } else { "step" },
                                            "模型配置"
                                        }
                                    }

                                    if init_step() == 1 {
                                        // ===== Step 1: 基础信息 =====
                                        div { "data-testid": "init-wizard-step-1",
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "组织名称 *" }
                                                input {
                                                    class: "input input-bordered w-full",
                                                    r#type: "text",
                                                    "data-testid": "init-org-name",
                                                    value: "{org_name}",
                                                    oninput: move |e| org_name.set(e.value()),
                                                    placeholder: "例如：我的组织",
                                                }
                                            }
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "组织描述" }
                                                textarea {
                                                    class: "textarea textarea-bordered w-full",
                                                    value: "{org_description}",
                                                    oninput: move |e| org_description.set(e.value()),
                                                    placeholder: "简单描述一下您的组织...",
                                                }
                                            }
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "管理员用户名 *" }
                                                input {
                                                    class: "input input-bordered w-full",
                                                    r#type: "text",
                                                    "data-testid": "init-username",
                                                    value: "{init_username}",
                                                    oninput: move |e| init_username.set(e.value()),
                                                    placeholder: "例如：admin",
                                                }
                                            }
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "管理员密码 *" }
                                                input {
                                                    class: "input input-bordered w-full",
                                                    r#type: "password",
                                                    "data-testid": "init-password",
                                                    value: "{init_password}",
                                                    oninput: move |e| init_password.set(e.value()),
                                                    placeholder: "请输入密码",
                                                }
                                            }
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "显示名称" }
                                                input {
                                                    class: "input input-bordered w-full",
                                                    r#type: "text",
                                                    value: "{display_name}",
                                                    oninput: move |e| display_name.set(e.value()),
                                                    placeholder: "例如：超级管理员",
                                                }
                                            }
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "邮箱" }
                                                input {
                                                    class: "input input-bordered w-full",
                                                    r#type: "email",
                                                    value: "{email}",
                                                    oninput: move |e| email.set(e.value()),
                                                    placeholder: "admin@example.com",
                                                }
                                            }

                                            div { class: "form-control mt-4",
                                                button {
                                                    r#type: "button",
                                                    class: "btn btn-primary w-full",
                                                    "data-testid": "init-next-step",
                                                    onclick: on_next_step,
                                                    "下一步"
                                                }
                                            }
                                        }
                                    } else {
                                        // ===== Step 2: 模型配置 =====
                                        div { class: "flex flex-col gap-3", "data-testid": "init-wizard-step-2",

                                            // 对话模型卡片
                                            div { class: "init-model-card",
                                                div { class: "init-model-card-header",
                                                    label { class: "label cursor-pointer justify-start gap-2",
                                                        input {
                                                            r#type: "checkbox",
                                                            class: "checkbox checkbox-primary",
                                                            "data-testid": "init-enable-chat",
                                                            checked: enable_chat(),
                                                            onchange: move |e| enable_chat.set(e.checked()),
                                                        }
                                                        span { class: "label-text font-medium", "配置对话模型" }
                                                    }
                                                    span { class: "init-model-card-sub", "用于 Agent 思考与对话" }
                                                }

                                                if enable_chat() {
                                                    div { class: "init-model-card-body",

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "Provider 名称 *" }
                                                            input {
                                                                class: "input input-bordered w-full",
                                                                r#type: "text",
                                                                "data-testid": "init-chat-provider-name",
                                                                value: "{chat_provider_name}",
                                                                oninput: move |e| chat_provider_name.set(e.value()),
                                                                placeholder: "例如：OpenAI",
                                                            }
                                                        }

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "服务商类型 *" }
                                                            select {
                                                                class: "select select-bordered w-full",
                                                                "data-testid": "init-chat-provider-type",
                                                                value: "{chat_provider_type}",
                                                                onchange: move |e| {
                                                                    if let Ok(v) = e.value().parse::<i32>() {
                                                                        chat_provider_type.set(v);
                                                                    }
                                                                },
                                                                option { value: "0", "OpenAI" }
                                                                option { value: "1", "DeepSeek" }
                                                                option { value: "2", "Qwen" }
                                                                option { value: "3", "Doubao" }
                                                                option { value: "4", "Ollama" }
                                                                option { value: "5", "Custom" }
                                                            }
                                                        }

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "模型名称 *" }
                                                            input {
                                                                class: "input input-bordered w-full",
                                                                r#type: "text",
                                                                "data-testid": "init-chat-model-name",
                                                                value: "{chat_model_name}",
                                                                oninput: move |e| chat_model_name.set(e.value()),
                                                                placeholder: "例如：gpt-4o-mini",
                                                            }
                                                        }

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "API Key *" }
                                                            input {
                                                                class: "input input-bordered w-full",
                                                                r#type: "password",
                                                                "data-testid": "init-chat-api-key",
                                                                value: "{chat_api_key}",
                                                                oninput: move |e| chat_api_key.set(e.value()),
                                                                placeholder: "sk-...",
                                                            }
                                                        }

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "Base URL" }
                                                            input {
                                                                class: "input input-bordered w-full",
                                                                r#type: "text",
                                                                value: "{chat_base_url}",
                                                                oninput: move |e| chat_base_url.set(e.value()),
                                                                placeholder: "自定义代理地址（可选）",
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    div {
                                                        class: "init-model-card-hint",
                                                        "data-testid": "init-chat-skip-hint",
                                                        "未配置对话模型：可稍后使用，但创建 Agent 前需先在「模型提供商管理」中配置"
                                                    }
                                                }
                                            }

                                            // 向量模型卡片
                                            div { class: "init-model-card",
                                                div { class: "init-model-card-header",
                                                    label { class: "label cursor-pointer justify-start gap-2",
                                                        input {
                                                            r#type: "checkbox",
                                                            class: "checkbox checkbox-primary",
                                                            "data-testid": "init-enable-embedding",
                                                            checked: enable_embedding(),
                                                            onchange: move |e| enable_embedding.set(e.checked()),
                                                        }
                                                        span { class: "label-text font-medium", "启用向量模型" }
                                                    }
                                                    span { class: "init-model-card-sub", "用于语义搜索" }
                                                }

                                                if enable_embedding() {
                                                    div { class: "init-model-card-body",

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "Provider 名称 *" }
                                                            input {
                                                                class: "input input-bordered w-full",
                                                                r#type: "text",
                                                                "data-testid": "init-embedding-provider-name",
                                                                value: "{embedding_provider_name}",
                                                                oninput: move |e| embedding_provider_name.set(e.value()),
                                                                placeholder: "例如：FastEmbed",
                                                            }
                                                        }

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "服务商类型 *" }
                                                            select {
                                                                class: "select select-bordered w-full",
                                                                "data-testid": "init-embedding-provider-type",
                                                                value: "{embedding_provider_type}",
                                                                onchange: move |e| {
                                                                    if let Ok(v) = e.value().parse::<i32>() {
                                                                        embedding_provider_type.set(v);
                                                                    }
                                                                },
                                                                option { value: "0", "OpenAI" }
                                                                option { value: "1", "DeepSeek" }
                                                                option { value: "2", "Qwen" }
                                                                option { value: "3", "Doubao" }
                                                                option { value: "4", "Ollama" }
                                                                option { value: "5", "Custom" }
                                                                option { value: "6", "FastEmbed（本地，无需 API Key）" }
                                                                option { value: "7", "DoubaoVision（豆包多模态 Embedding）" }
                                                            }
                                                        }

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "模型名称 *" }
                                                            input {
                                                                class: "input input-bordered w-full",
                                                                r#type: "text",
                                                                "data-testid": "init-embedding-model-name",
                                                                value: "{embedding_model_name}",
                                                                oninput: move |e| embedding_model_name.set(e.value()),
                                                                placeholder: "例如：BAAI/bge-small-en-v1.5",
                                                            }
                                                        }

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "API Key" }
                                                            input {
                                                                class: "input input-bordered w-full",
                                                                r#type: "password",
                                                                value: "{embedding_api_key}",
                                                                oninput: move |e| embedding_api_key.set(e.value()),
                                                                placeholder: "FastEmbed 无需填写",
                                                            }
                                                        }

                                                        div { class: "form-control w-full",
                                                            label { class: "form-label", "Base URL" }
                                                            input {
                                                                class: "input input-bordered w-full",
                                                                r#type: "text",
                                                                value: "{embedding_base_url}",
                                                                oninput: move |e| embedding_base_url.set(e.value()),
                                                                placeholder: "自定义代理地址（可选）",
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    div {
                                                        class: "init-model-card-hint",
                                                        "data-testid": "init-embedding-skip-hint",
                                                        "未启用向量模型：语义搜索不可用；后续补配时将自动全量重建向量索引，实体较多时耗时较长"
                                                    }
                                                }
                                            }

                                            // 按钮组
                                            div { class: "flex gap-2 mt-4",
                                                button {
                                                    r#type: "button",
                                                    class: "btn btn-ghost flex-1",
                                                    onclick: on_prev_step,
                                                    "上一步"
                                                }
                                                button {
                                                    r#type: "submit",
                                                    class: "btn btn-primary flex-1",
                                                    "data-testid": "init-submit",
                                                    disabled: init_submitting(),
                                                    "完成初始化"
                                                }
                                            }
                                        }
                                    }
                                } // form 结束
                            } // else (非 init_submitting) 结束
                        } // 未初始化分支结束
                    } // loading 结束
                } // form-card 结束
            } // form-side 结束
        } // reception-page 结束
    }
}
