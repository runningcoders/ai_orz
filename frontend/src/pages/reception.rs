//! 前台接待 - 系统初始化 + 登录

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::auth::{check_initialized, initialize_system, login};
use crate::api::organization::list_organizations_public;
use crate::components::state::{ErrorAlert, Loading};
use crate::store::auth::{save_token, AuthState};
use common::api::{
    InitializeSystemRequest, LoginRequest, OrganizationListItem,
};

#[component]
pub fn Reception() -> Element {
    let mut loading = use_signal(|| true);
    let mut initialized = use_signal(|| false);
    let mut organizations = use_signal(Vec::<OrganizationListItem>::new);
    let mut error = use_signal(String::new);

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

    let auth = use_context::<Signal<AuthState>>();

    // 页面加载检查初始化状态
    use_effect(move || {
        spawn(async move {
            match check_initialized().await {
                Ok(resp) => {
                    if resp.initialized {
                        match list_organizations_public().await {
                            Ok(list) => {
                                organizations.set(list.organizations);
                                initialized.set(true);
                            }
                            Err(e) => error.set(e),
                        }
                    } else {
                        initialized.set(false);
                    }
                }
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    // 登录提交
    let on_submit_login = move |_| {
        spawn(async move {
            if selected_org_id().is_empty() {
                error.set("请先选择一个组织".to_string());
                return;
            }
            if login_username().is_empty() || login_password().is_empty() {
                error.set("用户名和密码不能为空".to_string());
                return;
            }
            login_submitting.set(true);
            error.set(String::new());

            let req = LoginRequest {
                organization_id: selected_org_id(),
                username: login_username(),
                password_hash: login_password(),
            };

            match login(req).await {
                Ok(resp) => {
                    save_token(&resp.token);
                    // 更新全局认证状态
                    let mut state = auth.write();
                    state.token = Some(resp.token);
                    state.username = resp.username.unwrap_or_default();
                    state.role = resp.role.unwrap_or(1);
                    state.org_id = resp.organization_id.unwrap_or_default();
                    drop(state);
                    // 跳转 - 使用 window location 触发完整刷新
                    let _ = web_sys::window().unwrap().location().set_href("/");
                }
                Err(e) => {
                    error.set(e);
                    login_submitting.set(false);
                }
            }
        });
    };

    // 初始化提交
    let on_submit_init = move |_| {
        spawn(async move {
            if org_name().is_empty() || init_username().is_empty() || init_password().is_empty() {
                error.set("组织名称、用户名、密码不能为空".to_string());
                return;
            }
            init_submitting.set(true);
            error.set(String::new());

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
                    let _ = web_sys::window().unwrap().location().reload();
                }
                Err(e) => {
                    error.set(e);
                    init_submitting.set(false);
                }
            }
        });
    };

    rsx! {
        div { style: "max-width: 600px; margin: 0 auto;",
            div { class: "card", style: "text-align: center;",
                div { style: "font-size: 56px; margin-bottom: 24px;", "👋" }
                h2 { style: "color: var(--color-mistral-black); margin-bottom: 16px; font-size: 28px;",
                    "欢迎来到 AI Orz"
                }
                p { class: "text-secondary", style: "margin-bottom: 32px; font-size: 16px;",
                    "AI Orz 是一个智能的 AI 代理执行框架，帮助您组织和管理各类 AI 智能体，让它们协同工作完成复杂任务。"
                }

                if loading() {
                    Loading {}
                } else {
                    ErrorAlert { message: error() }

                    if initialized() {
                        // 已初始化：登录表单
                        div { style: "text-align: left;",
                            h3 { class: "mb-4", "🔐 请选择组织并登录" }

                            // 组织列表
                            div { class: "mb-4",
                                for org in organizations() {
                                    {
                                        let is_selected = selected_org_id() == org.organization_id;
                                        let border = if is_selected { "var(--color-mistral-orange)" } else { "var(--color-border)" };
                                        let bg = if is_selected { "var(--color-cream)" } else { "var(--color-warm-ivory)" };
                                        rsx! {
                                            div {
                                                key: "{org.organization_id}",
                                                style: "padding: 12px; border-radius: 4px; margin-bottom: 8px; cursor: pointer; border: 2px solid {border}; background: {bg};",
                                                onclick: move |_| selected_org_id.set(org.organization_id.clone()),
                                                div { style: "font-weight: 600; color: var(--color-text-primary);",
                                                    "{org.name}"
                                                }
                                                if let Some(desc) = &org.description {
                                                    p { class: "text-secondary", style: "font-size: 13px; margin-top: 4px;", "{desc}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            form { onsubmit: move |e| { e.prevent_default(); on_submit_login.call(()); },
                                div { class: "form-group",
                                    label { class: "form-label", "用户名" }
                                    input {
                                        class: "form-input",
                                        r#type: "text",
                                        value: "{login_username}",
                                        oninput: move |e| login_username.set(e.value()),
                                        placeholder: "请输入用户名",
                                    }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "密码" }
                                    input {
                                        class: "form-input",
                                        r#type: "password",
                                        value: "{login_password}",
                                        oninput: move |e| login_password.set(e.value()),
                                        placeholder: "请输入密码",
                                    }
                                }
                                button {
                                    class: "btn btn-accent btn-lg w-full",
                                    r#type: "submit",
                                    disabled: login_submitting(),
                                    if login_submitting() { "登录中..." } else { "登录" }
                                }
                            }
                        }
                    } else {
                        // 未初始化：初始化表单
                        div { style: "text-align: left;",
                            h3 { class: "mb-4", "🚀 首次使用 - 初始化系统" }
                            p { class: "text-secondary mb-6",
                                "欢迎使用 AI Orz！请填写以下信息完成初始化，创建您的第一个组织和超级管理员用户。"
                            }
                            form { onsubmit: move |e| { e.prevent_default(); on_submit_init.call(()); },
                                div { class: "form-group",
                                    label { class: "form-label", "组织名称 *" }
                                    input { class: "form-input", r#type: "text", value: "{org_name}",
                                        oninput: move |e| org_name.set(e.value()), placeholder: "例如：我的组织" }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "组织描述" }
                                    textarea { class: "form-textarea", value: "{org_description}",
                                        oninput: move |e| org_description.set(e.value()), placeholder: "简单描述一下您的组织..." }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "管理员用户名 *" }
                                    input { class: "form-input", r#type: "text", value: "{init_username}",
                                        oninput: move |e| init_username.set(e.value()), placeholder: "例如：admin" }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "管理员密码 *" }
                                    input { class: "form-input", r#type: "password", value: "{init_password}",
                                        oninput: move |e| init_password.set(e.value()), placeholder: "请输入密码" }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "显示名称" }
                                    input { class: "form-input", r#type: "text", value: "{display_name}",
                                        oninput: move |e| display_name.set(e.value()), placeholder: "例如：超级管理员" }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "邮箱" }
                                    input { class: "form-input", r#type: "email", value: "{email}",
                                        oninput: move |e| email.set(e.value()), placeholder: "admin@example.com" }
                                }
                                button { class: "btn btn-accent btn-lg w-full", r#type: "submit", disabled: init_submitting(),
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
