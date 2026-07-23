//! 前台接待 - 系统初始化 + 登录

use dioxus::prelude::*;

use crate::api::auth::{check_initialized, initialize_system, login};
use crate::api::organization::{get_current_user_info, list_organizations_public};
use crate::components::state::Loading;
use crate::store::auth::{mark_logged_in, save_role, AuthState};
use crate::store::toast::use_toast;
use common::api::{InitializeSystemRequest, LoginRequest, OrganizationListItem};

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

    // 初始化表单
    let mut org_name = use_signal(String::new);
    let mut org_description = use_signal(String::new);
    let mut init_username = use_signal(String::new);
    let mut init_password = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut init_submitting = use_signal(|| false);

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
                    state.username = resp.username.clone();
                    state.org_id = resp.organization_id.clone();
                    // 修复 R-M1：之前硬编码 role=1 导致管理员登录后看不到管理菜单。
                    // LoginResponse 不含 role，登录后立即调用 /user/me 获取真实 role。
                    drop(state);
                    match get_current_user_info().await {
                        Ok(user_info) => {
                            let mut state = auth.write();
                            state.role = user_info.data.role;
                            state.username = user_info.data.username.clone();
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

    // 初始化提交
    let on_submit_init = move |_| {
        spawn(async move {
            if org_name().is_empty() || init_username().is_empty() || init_password().is_empty() {
                toast.error("组织名称、用户名、密码不能为空");
                return;
            }
            init_submitting.set(true);

            let req = InitializeSystemRequest {
                organization_name: org_name(),
                description: if org_description().is_empty() { None } else { Some(org_description()) },
                admin_username: init_username(),
                admin_password_hash: init_password(),
                admin_display_name: if display_name().is_empty() { None } else { Some(display_name()) },
                admin_email: if email().is_empty() { None } else { Some(email()) },
            };

            match initialize_system(req).await {
                Ok(_) => {
                    // 修复 L_NEW：同上，window() 可能返回 None
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
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
                            // 已初始化：登录表单
                            div { class: "reception-form-header",
                                h2 { class: "reception-form-title", "欢迎回来" }
                                p { class: "reception-form-desc", "选择组织并登录您的账户" }
                            }

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
                                        value: "{login_password}",
                                        oninput: move |e| login_password.set(e.value()),
                                        placeholder: "请输入密码",
                                    }
                                }
                                button {
                                    class: "btn btn-primary btn-lg w-full",
                                    r#type: "submit",
                                    disabled: login_submitting(),
                                    if login_submitting() { "登录中..." } else { "登录" }
                                }
                            }
                        } else {
                            // 未初始化：初始化表单
                            div { class: "reception-form-header",
                                h2 { class: "reception-form-title", "系统初始化" }
                                p { class: "reception-form-desc", "创建您的第一个组织和超级管理员" }
                            }

                            form { onsubmit: move |e| { e.prevent_default(); on_submit_init(e); },
                                div { class: "form-control w-full",
                                    label { class: "form-label", "组织名称 *" }
                                    input {
                                        class: "input input-bordered w-full",
                                        r#type: "text",
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
                                button {
                                    class: "btn btn-primary btn-lg w-full",
                                    r#type: "submit",
                                    disabled: init_submitting(),
                                    if init_submitting() { "初始化中..." } else { "完成初始化" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
