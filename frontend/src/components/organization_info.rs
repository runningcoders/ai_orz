use dioxus::prelude::*;
use crate::api::organization::{get_organization_info, update_organization_info, OrganizationInfo as OrganizationInfoDto, UpdateOrganizationRequest};

#[component]
pub fn OrganizationInfo() -> Element {
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| String::new());
    let mut success = use_signal(|| String::new());
    let mut org_info = use_signal(|| Option::<OrganizationInfoDto>::None);

    // 编辑状态
    let mut editing_name = use_signal(|| String::new());
    let mut editing_description = use_signal(|| String::new());
    let mut editing_base_url = use_signal(|| String::new());

    // 页面加载时获取组织信息
    use_effect(move || {
        spawn(async move {
            match get_organization_info().await {
                Ok(info) => {
                    editing_name.set(info.name.clone());
                    editing_description.set(info.description.clone());
                    editing_base_url.set(info.base_url.clone());
                    org_info.set(Some(info));
                    error.set(String::new());
                }
                Err(e) => {
                    error.set(format!("获取组织信息失败: {}", e));
                }
            }
            loading.set(false);
        });
    });

    // 提交修改
    let on_submit = move |_| {
        spawn(async move {
            if let Some(info) = org_info() {
                saving.set(true);
                error.set(String::new());
                success.set(String::new());

                let req = UpdateOrganizationRequest {
                    name: if editing_name() != info.name {
                        Some(editing_name())
                    } else {
                        None
                    },
                    description: if editing_description() != info.description {
                        Some(editing_description())
                    } else {
                        None
                    },
                    base_url: if editing_base_url() != info.base_url {
                        Some(editing_base_url())
                    } else {
                        None
                    },
                };

                match update_organization_info(req).await {
                    Ok(_) => {
                        success.set("组织信息更新成功！".to_string());
                        // 重新获取最新信息
                        match get_organization_info().await {
                            Ok(new_info) => {
                                editing_name.set(new_info.name.clone());
                                editing_description.set(new_info.description.clone());
                                editing_base_url.set(new_info.base_url.clone());
                                org_info.set(Some(new_info));
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
                "🏢 组织信息"
            }

            // 加载中
            if loading() {
                div {
                    style: "
                        text-align: center;
                        color: #666;
                        padding: 2rem;
                    ",
                    "正在加载组织信息..."
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
            } else if let Some(info) = org_info() {
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
                            "组织 ID"
                        }
                        input {
                            r#type: "text",
                            value: "{info.id}",
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
                            "组织 ID 不可修改"
                        }
                    }

                    // 组织名称（可修改）
                    div {
                        style: "margin-bottom: 1.5rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "组织名称"
                        }
                        input {
                            r#type: "text",
                            value: "{editing_name}",
                            oninput: move |e| editing_name.set(e.value()),
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
                            placeholder: "请输入组织名称",
                        }
                    }

                    // 组织描述（可修改）
                    div {
                        style: "margin-bottom: 1.5rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "组织描述"
                        }
                        textarea {
                            value: "{editing_description}",
                            oninput: move |e| editing_description.set(e.value()),
                            style: "
                                width: 100%;
                                min-height: 100px;
                                padding: 0.75rem;
                                border: 1px solid #ddd;
                                border-radius: 6px;
                                font-size: 1rem;
                                box-sizing: border-box;
                                resize: vertical;
                                font-family: inherit;
                                &:focus {{
                                    outline: none;
                                    border-color: #3498db;
                                    box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                }}
                            ",
                            placeholder: "请输入组织描述",
                        }
                    }

                    // 外部访问地址（可修改）
                    div {
                        style: "margin-bottom: 2rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "外部访问地址"
                        }
                        input {
                            r#type: "url",
                            value: "{editing_base_url}",
                            oninput: move |e| editing_base_url.set(e.value()),
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
                            placeholder: "https://example.com",
                        }
                        p {
                            style: "
                                color: #999;
                                font-size: 0.85rem;
                                margin: 0.5rem 0 0 0;
                            ",
                            "组织对外访问地址，用于邮件/分享链接等场景"
                        }
                    }

                    // 组织状态（不可修改）
                    div {
                        style: "margin-bottom: 2rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "组织状态"
                        }
                        input {
                            r#type: "text",
                            value: if info.status == 1 { "正常启用" } else { "已禁用" },
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

                    // 创建时间（不可修改）
                    div {
                        style: "margin-bottom: 2rem;",
                        label {
                            style: "
                                display: block;
                                color: #2c3e50;
                                font-weight: 600;
                                margin-bottom: 0.5rem;
                            ",
                            "创建时间"
                        }
                        input {
                            r#type: "text",
                            value: "{info.created_at}",
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
