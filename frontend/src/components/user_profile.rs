use dioxus::prelude::*;
use crate::api::organization::{get_current_user_info, update_current_user_info, UserInfo, UpdateUserRequest};

#[component]
pub fn UserProfile() -> Element {
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| String::new());
    let mut success = use_signal(|| String::new());
    let mut user_info = use_signal(|| Option::<UserInfo>::None);

    // 编辑状态
    let mut editing_display_name = use_signal(|| String::new());
    let mut editing_email = use_signal(|| String::new());
    let mut editing_password = use_signal(|| String::new());

    // 页面加载时获取用户信息
    use_effect(move || {
        spawn(async move {
            match get_current_user_info().await {
                Ok(info) => {
                    editing_display_name.set(info.display_name.clone().unwrap_or_default());
                    editing_email.set(info.email.clone().unwrap_or_default());
                    user_info.set(Some(info));
                    error.set(String::new());
                }
                Err(e) => {
                    error.set(format!("获取用户信息失败: {}", e));
                }
            }
            loading.set(false);
        });
    });

    // 提交修改
    let on_submit = move |_| {
        spawn(async move {
            if let Some(info) = user_info() {
                saving.set(true);
                error.set(String::new());
                success.set(String::new());

                let req = UpdateUserRequest {
                    display_name: if editing_display_name() != info.display_name.unwrap_or_default() {
                        Some(editing_display_name())
                    } else {
                        None
                    },
                    email: if editing_email() != info.email.unwrap_or_default() {
                        Some(editing_email())
                    } else {
                        None
                    },
                    password_hash: if !editing_password().is_empty() {
                        Some(editing_password())
                    } else {
                        None
                    },
                };

                match update_current_user_info(req).await {
                    Ok(_) => {
                        success.set("用户信息更新成功！".to_string());
                        editing_password.set(String::new());
                        // 重新获取最新信息
                        match get_current_user_info().await {
                            Ok(new_info) => {
                                editing_display_name.set(new_info.display_name.clone().unwrap_or_default());
                                editing_email.set(new_info.email.clone().unwrap_or_default());
                                user_info.set(Some(new_info));
                            }
                            Err(_) => {}
                        }
                    }
                    Err(e) => {
                        error.set(format!("更新失败: {}", e));
                    }
                }
                saving.set(false);
            }
        });
    };

    rsx! {
        div {
            style: "
                max-width: 700px;
                margin: 0 auto;
                background: white;
                border-radius: 12px;
                padding: 2.5rem;
                box-shadow: 0 2px 12px rgba(0,0,0,0.08);
            ",

            h2 {
                style: "
                    color: #2c3e50;
                    margin-bottom: 2rem;
                    font-size: 1.75rem;
                ",
                "👤 个人信息"
            }

            // 加载中
            if loading() {
                div {
                    style: "
                        text-align: center;
                        color: #666;
                        padding: 2rem;
                    ",
                    "正在加载用户信息..."
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
                    ",
                    "{error}"
                }
            } else if let Some(info) = user_info() {
                // 成功消息
                if !success().is_empty() {
                    div {
                        style: "
                            background: #e8f5e9;
                            border: 1px solid #a5d6a7;
                            border-radius: 6px;
                            padding: 1rem;
                            margin-bottom: 1.5rem;
                            color: #2e7d32;
                        ",
                        "{success}"
                    }
                }

                form {
                    onsubmit: on_submit,

                    // 用户名（不可修改）
                    div {
                        style: "margin-bottom: 1.5rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "用户名"
                        }
                        input {
                            r#type: "text",
                            value: "{info.username}",
                            disabled: true,
                            style: "
                                width: 100%;
                                padding: 0.75rem;
                                border: 1px solid #ddd;
                                border-radius: 6px;
                                font-size: 1rem;
                                box-sizing: border-box;
                                background: #f5f5f5;
                                color: #666;
                            ",
                        }
                        p {
                            style: "
                                color: #999;
                                font-size: 0.85rem;
                                margin: 0.5rem 0 0 0;
                            ",
                            "用户名不可修改"
                        }
                    }

                    // 用户角色（不可修改）
                    div {
                        style: "margin-bottom: 1.5rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "角色"
                        }
                        input {
                            r#type: "text",
                            value: "{info.role_name}",
                            disabled: true,
                            style: "
                                width: 100%;
                                padding: 0.75rem;
                                border: 1px solid #ddd;
                                border-radius: 6px;
                                font-size: 1rem;
                                box-sizing: border-box;
                                background: #f5f5f5;
                                color: #666;
                            ",
                        }
                    }

                    // 组织 ID（不可修改）
                    div {
                        style: "margin-bottom: 1.5rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "所属组织 ID"
                        }
                        input {
                            r#type: "text",
                            value: "{info.organization_id}",
                            disabled: true,
                            style: "
                                width: 100%;
                                padding: 0.75rem;
                                border: 1px solid #ddd;
                                border-radius: 6px;
                                font-size: 1rem;
                                box-sizing: border-box;
                                background: #f5f5f5;
                                color: #666;
                            ",
                        }
                    }

                    // 显示名称（可修改）
                    div {
                        style: "margin-bottom: 1.5rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "显示名称"
                        }
                        input {
                            r#type: "text",
                            value: "{editing_display_name}",
                            oninput: move |e| editing_display_name.set(e.value()),
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
                            placeholder: "请输入显示名称",
                        }
                        p {
                            style: "
                                color: #999;
                                font-size: 0.85rem;
                                margin: 0.5rem 0 0 0;
                            ",
                            "这是您在系统中显示的昵称，可以随时修改"
                        }
                    }

                    // 邮箱（可修改）
                    div {
                        style: "margin-bottom: 1.5rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "邮箱"
                        }
                        input {
                            r#type: "email",
                            value: "{editing_email}",
                            oninput: move |e| editing_email.set(e.value()),
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
                            placeholder: "请输入邮箱地址",
                        }
                    }

                    // 新密码（可选修改）
                    div {
                        style: "margin-bottom: 2rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "修改密码"
                        }
                        input {
                            r#type: "password",
                            value: "{editing_password}",
                            oninput: move |e| editing_password.set(e.value()),
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
                            placeholder: "留空则不修改密码，需要填入 bcrypt 哈希",
                        }
                        p {
                            style: "
                                color: #999;
                                font-size: 0.85rem;
                                margin: 0.5rem 0 0 0;
                            ",
                            "前端需要将密码做 bcrypt 哈希后提交，请确保已经哈希处理"
                        }
                    }

                    // 提交按钮
                    div {
                        style: "text-align: right;",
                        button {
                            r#type: "submit",
                            disabled: saving(),
                            style: "
                                background: #3498db;
                                color: white;
                                border: none;
                                padding: 0.875rem 2rem;
                                border-radius: 6px;
                                cursor: pointer;
                                font-size: 1rem;
                                font-weight: 500;
                                transition: background-color 0.2s;
                                &:hover:not(:disabled) {{
                                    background-color: #2980b9;
                                }}
                                &:disabled {{
                                    background: #ccc;
                                    cursor: not-allowed;
                                }}
                            ",
                            if saving() {
                                "保存中..."
                            } else {
                                "保存修改"
                            }
                        }
                    }
                }
            }
        }
    }
}
