use dioxus::prelude::*;
use common::api::{InitializeSystemRequest, LoginRequest, OrganizationListItem};
use crate::api::organization::{
    check_initialized, list_organizations, initialize_system, login,
};

#[component]
pub fn Reception() -> Element {
    // 状态：加载中、已初始化（有组织）、未初始化（需要初始化）
    let mut loading = use_signal(|| true);
    let mut initialized = use_signal(|| false);
    let mut organizations = use_signal(Vec::<OrganizationListItem>::new);
    let mut error = use_signal(|| String::new());

    // 登录表单状态
    let mut selected_org_id = use_signal(|| String::new());
    let mut login_username = use_signal(|| String::new());
    let mut login_password = use_signal(|| String::new());
    let mut login_submitting = use_signal(|| false);

    // 初始化表单状态
    let mut org_name = use_signal(|| String::new());
    let mut org_description = use_signal(|| String::new());
    let mut init_username = use_signal(|| String::new());
    let mut init_password = use_signal(|| String::new());
    let mut display_name = use_signal(|| String::new());
    let mut email = use_signal(|| String::new());
    let mut init_submitting = use_signal(|| false);

    // 页面加载时检查初始化状态
    use_effect(move || {
        spawn(async move {
            match check_initialized().await {
                Ok(_is_initialized) => {
                    // 如果已初始化，获取组织列表
                    if _is_initialized {
                        match list_organizations().await {
                            Ok(orgs) => {
                                organizations.set(orgs);
                                initialized.set(true);
                            }
                            Err(e) => {
                                error.set(e);
                            }
                        }
                    } else {
                        initialized.set(false);
                    }
                }
                Err(e) => {
                    error.set(e);
                }
            }
            loading.set(false);
        });
    });

    // 处理初始化提交
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
                admin_password_hash: init_password(), // 前端应该已经是 bcrypt hash 了
                admin_display_name: if display_name().is_empty() { None } else { Some(display_name()) },
                admin_email: if email().is_empty() { None } else { Some(email()) },
            };

            match initialize_system(req).await {
                Ok(_resp) => {
                    // 初始化成功，刷新页面重新检查
                    web_sys::window().unwrap().location().reload().unwrap();
                }
                Err(e) => {
                    error.set(e);
                    init_submitting.set(false);
                }
            }
        });
    };

    // 处理登录提交
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
                Ok(_resp) => {
                    // 登录成功，跳转到系统主页
                    web_sys::window().unwrap().location().set_href("/").unwrap();
                    web_sys::window().unwrap().location().reload().unwrap();
                }
                Err(e) => {
                    error.set(e);
                    login_submitting.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            style: "
                background: white;
                border-radius: 12px;
                padding: 3rem 2rem;
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
                text-align: center;
                max-width: 600px;
                margin: 0 auto;
            ",

            div {
                style: "
                    font-size: 4rem;
                    margin-bottom: 1.5rem;
                ",
                "👋"
            }

            h2 {
                style: "
                    color: #2c3e50;
                    margin-bottom: 1rem;
                    font-size: 1.75rem;
                ",
                "欢迎来到 AI Orz"
            }

            p {
                style: "
                    color: #666;
                    font-size: 1.1rem;
                    line-height: 1.6;
                    margin-bottom: 2rem;
                ",
                "AI Orz 是一个智能的 AI 代理执行框架，帮助您组织和管理各类 AI 智能体，让它们协同工作完成复杂任务。"
            }

            // 加载中状态
            if loading() {
                div {
                    style: "
                        color: #666;
                        font-size: 1.1rem;
                        padding: 2rem;
                    ",
                    "正在检查系统状态..."
                }
            } else if !error().is_empty() {
                div {
                    style: "
                        background: #fee;
                        border: 1px solid #faa;
                        border-radius: 6px;
                        padding: 1rem;
                        margin-bottom: 1.5rem;
                        color: #c33;
                        text-align: left;
                    ",
                    "错误: {error}"
                }
            } else if initialized() {
                // 已初始化：显示组织选择 + 登录表单
                div {
                    style: "text-align: left;",
                    h3 {
                        style: "
                            color: #2c3e50;
                            margin-bottom: 1.5rem;
                            font-size: 1.3rem;
                        ",
                        "🔐 请选择组织并登录"
                    }

                    // 组织选择列表
                    div {
                        style: "
                            background: #f8f9fa;
                            border-radius: 8px;
                            padding: 1rem;
                            margin-bottom: 1.5rem;
                        ",
                        for org in organizations() {
                            {
                                let is_selected = selected_org_id() == org.organization_id;
                                let bg = if is_selected { "#e3f2fd" } else { "white" };
                                let border = if is_selected { "#3498db" } else { "transparent" };
                                let check_mark = if is_selected { "✓ " } else { "" };
                                rsx! {
                                    div {
                                        style: "
                                            padding: 0.8rem;
                                            border-radius: 6px;
                                            margin-bottom: 0.5rem;
                                            cursor: pointer;
                                            border: 2px solid {border};
                                            background: {bg};
                                            transition: all 0.2s;
                                            &:hover {{
                                                border-color: #90caf9;
                                            }}
                                        ",
                                        onclick: move |_| selected_org_id.set(org.organization_id.clone()),
                                        div {
                                            style: "
                                                font-size: 1.1rem;
                                                font-weight: 600;
                                                color: #2c3e50;
                                                margin-bottom: 0.25rem;
                                            ",
                                            "{check_mark}{org.name}"
                                        }
                                        if !org.description.is_none() {
                                            p {
                                                style: "
                                                    color: #666;
                                                    font-size: 0.9rem;
                                                    margin: 0;
                                                ",
                                                "{org.description.clone().unwrap_or_default()}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 登录表单
                    form {
                        onsubmit: on_submit_login,

                        // 用户名
                        div {
                            style: "margin-bottom: 1.5rem;",
                            label {
                                style: "
                                    display: block;
                                    color: #2c3e50;
                                    font-weight: 500;
                                    margin-bottom: 0.5rem;
                                ",
                                "用户名 *"
                            }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{login_username}",
                                oninput: move |e| login_username.set(e.value()),
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    box-sizing: border-box;
                                    &:focus {{
                                        outline: none;
                                        border-color: #3498db;
                                        box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                    }}
                                ",
                                placeholder: "请输入用户名",
                            }
                        }

                        // 密码
                        div {
                            style: "margin-bottom: 2rem;",
                            label {
                                style: "
                                    display: block;
                                    color: #2c3e50;
                                    font-weight: 500;
                                    margin-bottom: 0.5rem;
                                ",
                                "密码 *（bcrypt 哈希后）"
                            }
                            input {
                                r#type: "password",
                                required: true,
                                value: "{login_password}",
                                oninput: move |e| login_password.set(e.value()),
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    box-sizing: border-box;
                                    &:focus {{
                                        outline: none;
                                        border-color: #3498db;
                                        box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                    }}
                                ",
                                placeholder: "请输入密码哈希",
                            }
                        }

                        // 登录按钮
                        div {
                            style: "text-align: center;",
                            button {
                                r#type: "submit",
                                disabled: "{login_submitting}",
                                style: "
                                    background: #3498db;
                                    color: white;
                                    border: none;
                                    padding: 1rem 2.5rem;
                                    border-radius: 6px;
                                    cursor: pointer;
                                    font-size: 1.1rem;
                                    transition: background-color 0.2s;
                                    &:hover:not(:disabled) {{
                                        background-color: #2980b9;
                                    }}
                                    &:disabled {{
                                        background: #ccc;
                                        cursor: not-allowed;
                                    }}
                                ",
                                if login_submitting() {
                                    "登录中..."
                                } else {
                                    "登录"
                                }
                            }
                        }
                    }
                }
            } else {
                // 未初始化：显示初始化表单
                div {
                    style: "text-align: left;",
                    h3 {
                        style: "
                            color: #2c3e50;
                            margin-bottom: 1rem;
                            font-size: 1.3rem;
                        ",
                        "🚀 首次使用 - 初始化系统"
                    }

                    p {
                        style: "
                            color: #666;
                            margin-bottom: 2rem;
                        ",
                        "欢迎使用 AI Orz！系统检测到您还没有创建组织，请填写以下信息完成初始化，创建您的第一个组织和超级管理员用户。"
                    }

                    form {
                        onsubmit: on_submit_init,

                        // 组织名称
                        div {
                            style: "margin-bottom: 1.5rem;",
                            label {
                                style: "
                                    display: block;
                                    color: #2c3e50;
                                    font-weight: 500;
                                    margin-bottom: 0.5rem;
                                ",
                                "组织名称 *"
                            }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{org_name}",
                                oninput: move |e| org_name.set(e.value()),
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    box-sizing: border-box;
                                    &:focus {{
                                        outline: none;
                                        border-color: #3498db;
                                        box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                    }}
                                ",
                                placeholder: "例如：我的组织",
                            }
                        }

                        // 组织描述
                        div {
                            style: "margin-bottom: 1.5rem;",
                            label {
                                style: "
                                    display: block;
                                    color: #2c3e50;
                                    font-weight: 500;
                                    margin-bottom: 0.5rem;
                                ",
                                "组织描述"
                            }
                            textarea {
                                value: "{org_description}",
                                oninput: move |e| org_description.set(e.value()),
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    box-sizing: border-box;
                                    min-height: 80px;
                                    &:focus {{
                                        outline: none;
                                        border-color: #3498db;
                                        box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                    }}
                                ",
                                placeholder: "简单描述一下您的组织...",
                            }
                        }

                        // 用户名
                        div {
                            style: "margin-bottom: 1.5rem;",
                            label {
                                style: "
                                    display: block;
                                    color: #2c3e50;
                                    font-weight: 500;
                                    margin-bottom: 0.5rem;
                                ",
                                "管理员用户名 *"
                            }
                            input {
                                r#type: "text",
                                required: true,
                                value: "{init_username}",
                                oninput: move |e| init_username.set(e.value()),
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    box-sizing: border-box;
                                    &:focus {{
                                        outline: none;
                                        border-color: #3498db;
                                        box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                    }}
                                ",
                                placeholder: "例如：admin",
                            }
                        }

                        // 密码
                        div {
                            style: "margin-bottom: 1.5rem;",
                            label {
                                style: "
                                    display: block;
                                    color: #2c3e50;
                                    font-weight: 500;
                                    margin-bottom: 0.5rem;
                                ",
                                "管理员密码 *（bcrypt 哈希后）"
                            }
                            input {
                                r#type: "password",
                                required: true,
                                value: "{init_password}",
                                oninput: move |e| init_password.set(e.value()),
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    box-sizing: border-box;
                                    &:focus {{
                                        outline: none;
                                        border-color: #3498db;
                                        box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                    }}
                                ",
                                placeholder: "请输入密码哈希...",
                            }
                        }

                        // 显示名称
                        div {
                            style: "margin-bottom: 1.5rem;",
                            label {
                                style: "
                                    display: block;
                                    color: #2c3e50;
                                    font-weight: 500;
                                    margin-bottom: 0.5rem;
                                ",
                                "显示名称"
                            }
                            input {
                                r#type: "text",
                                value: "{display_name}",
                                oninput: move |e| display_name.set(e.value()),
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    box-sizing: border-box;
                                    &:focus {{
                                        outline: none;
                                        border-color: #3498db;
                                        box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                    }}
                                ",
                                placeholder: "例如：超级管理员",
                            }
                        }

                        // 邮箱
                        div {
                            style: "margin-bottom: 2rem;",
                            label {
                                style: "
                                    display: block;
                                    color: #2c3e50;
                                    font-weight: 500;
                                    margin-bottom: 0.5rem;
                                ",
                                "邮箱"
                            }
                            input {
                                r#type: "email",
                                value: "{email}",
                                oninput: move |e| email.set(e.value()),
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                    box-sizing: border-box;
                                    &:focus {{
                                        outline: none;
                                        border-color: #3498db;
                                        box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                    }}
                                ",
                                placeholder: "例如：admin@example.com",
                            }
                        }

                        // 提交按钮
                        div {
                            style: "text-align: center;",
                            button {
                                r#type: "submit",
                                disabled: "{init_submitting}",
                                style: "
                                    background: #27ae60;
                                    color: white;
                                    border: none;
                                    padding: 1rem 2.5rem;
                                    border-radius: 6px;
                                    cursor: pointer;
                                    font-size: 1.1rem;
                                    transition: background-color 0.2s;
                                    &:hover:not(:disabled) {{
                                        background-color: #219653;
                                    }}
                                    &:disabled {{
                                        background: #ccc;
                                        cursor: not-allowed;
                                    }}
                                ",
                                if init_submitting() {
                                    "初始化中..."
                                } else {
                                    "完成初始化"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
