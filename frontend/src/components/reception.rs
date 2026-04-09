use dioxus::prelude::*;
use crate::api::organization::{
    check_initialized, list_organizations, initialize_system,
    InitializeSystemRequest, OrganizationInfo,
};

#[component]
pub fn Reception() -> Element {
    // 状态：加载中、已初始化（有组织）、未初始化（需要初始化）
    let mut loading = use_signal(|| true);
    let mut initialized = use_signal(|| false);
    let mut organizations = use_signal(Vec::<OrganizationInfo>::new);
    let mut error = use_signal(|| String::new());

    // 初始化表单状态
    let mut org_name = use_signal(|| String::new());
    let mut org_description = use_signal(|| String::new());
    let mut username = use_signal(|| String::new());
    let mut password = use_signal(|| String::new());
    let mut display_name = use_signal(|| String::new());
    let mut email = use_signal(|| String::new());
    let mut submitting = use_signal(|| false);

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
    let on_submit = move |_| {
        spawn(async move {
            if org_name().is_empty() || username().is_empty() || password().is_empty() {
                error.set("组织名称、用户名、密码不能为空".to_string());
                return;
            }

            submitting.set(true);
            error.set(String::new());

            let req = InitializeSystemRequest {
                organization_name: org_name(),
                description: if org_description().is_empty() { None } else { Some(org_description()) },
                username: username(),
                password_hash: password(), // 前端应该已经是 bcrypt hash 了
                display_name: if display_name().is_empty() { None } else { Some(display_name()) },
                email: if email().is_empty() { None } else { Some(email()) },
            };

            match initialize_system(req).await {
                Ok(_resp) => {
                    // 初始化成功，刷新页面重新检查
                    web_sys::window().unwrap().location().reload().unwrap();
                }
                Err(e) => {
                    error.set(e);
                    submitting.set(false);
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
                    "错误: {error()}"
                }
            } else if initialized() {
                // 已初始化：显示组织信息和登录按钮
                div {
                    style: "text-align: left;",
                    h3 {
                        style: "
                            color: #2c3e50;
                            margin-bottom: 1.5rem;
                            font-size: 1.3rem;
                        ",
                        "📢 系统已就绪"
                    }

                    div {
                        style: "
                            background: #f8f9fa;
                            border-radius: 8px;
                            padding: 1.5rem;
                            margin-bottom: 2rem;
                        ",
                        for org in organizations() {
                            div {
                                style: "margin-bottom: 1rem;",
                                div {
                                    style: "
                                        font-size: 1.2rem;
                                        font-weight: 600;
                                        color: #2c3e50;
                                        margin-bottom: 0.5rem;
                                    ",
                                    "{org.name}"
                                }
                                if !org.description.is_empty() {
                                    p {
                                        style: "
                                            color: #666;
                                            font-size: 0.95rem;
                                            margin-bottom: 0.5rem;
                                        ",
                                        "{org.description}"
                                    }
                                }
                                if !org.base_url.is_empty() {
                                    a {
                                        href: "{org.base_url}",
                                        target: "_blank",
                                        style: "
                                            color: #3498db;
                                            font-size: 0.9rem;
                                            text-decoration: none;
                                            &:hover {{
                                                text-decoration: underline;
                                            }}
                                        ",
                                        "{org.base_url}"
                                    }
                                }
                            }
                            if organizations().len() > 1 {
                                hr { style: "margin: 1rem 0; border: none; border-top: 1px solid #ddd;" }
                            }
                        }
                    }

                    div {
                        style: "
                            text-align: center;
                        ",
                        button {
                            style: "
                                background: #3498db;
                                color: white;
                                border: none;
                                padding: 0.9rem 2rem;
                                border-radius: 6px;
                                cursor: pointer;
                                font-size: 1.1rem;
                                transition: background-color 0.2s;
                                &:hover {{
                                    background-color: #2980b9;
                                }}
                            ",
                            "进入系统"
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
                        onsubmit: on_submit,

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
                                value: "{org_name()}",
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
                                value: "{org_description()}",
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
                                value: "{username()}",
                                oninput: move |e| username.set(e.value()),
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
                                value: "{password()}",
                                oninput: move |e| password.set(e.value()),
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
                                value: "{display_name()}",
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
                                value: "{email()}",
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
                                disabled: submitting(),
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
                                if submitting() {
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
