//! 设置页面
//! 允许用户在浏览器中修改前端配置（存储在 localStorage）

use dioxus::prelude::*;
use crate::config::FrontendConfig;

#[component]
pub fn SettingsPage() -> Element {
    // 加载当前配置
    let mut config = use_signal(|| FrontendConfig::load());
    let mut save_status = use_signal(|| Option::<String>::None); // None = 未保存, Some(msg) = 保存状态消息

    // 预处理状态消息显示
    let status_content = save_status().map(|status| {
        let is_success = status.starts_with("ok:") || status.starts_with("reset:");
        let bg_color = if is_success { "#f8f9fa" } else { "#ffeaea" };
        let text_color = if is_success { "#2c3e50" } else { "#c0392b" };
        let style = format!(
            "margin-top: 1rem;\
            padding: 1rem;\
            border-radius: 4px;\
            background-color: {};\
            color: {};",
            bg_color, text_color
        );
        // 添加 emoji 前缀显示
        let display_status = if status.starts_with("ok:") {
            format!("✅ {}", &status[3..])
        } else if status.starts_with("err:") {
            format!("❌ {}", &status[4..])
        } else if status.starts_with("reset:") {
            format!("🔄 {}", &status[6..])
        } else {
            status
        };
        (style, display_status)
    });

    rsx! {
        div {
            style: "
                max-width: 800px;
                margin: 0 auto;
                background: white;
                border-radius: 8px;
                padding: 2rem;
                box-shadow: 0 2px 8px rgba(0,0,0,0.1);
            ",
            h1 {
                style: "
                    font-size: 1.75rem;
                    margin-bottom: 1.5rem;
                    color: #2c3e50;
                ",
                "⚙️ 系统设置"
            }

            p {
                style: "
                    color: #666;
                    margin-bottom: 2rem;
                    line-height: 1.6;
                ",
                "这里的配置存储在浏览器的本地存储（localStorage）中，修改后无需重新编译前端。",
                br {},
                "如果清空浏览器数据，配置会重置为编译时默认值。"
            }

            // API 基础地址设置
            div {
                style: "margin-bottom: 1.5rem;",
                label {
                    style: "
                        display: block;
                        font-weight: 500;
                        margin-bottom: 0.5rem;
                        color: #333;
                    ",
                    "后端 API 基础地址"
                }
                input {
                    r#type: "text",
                    value: config().api_base_url,
                    placeholder: "例如: http://localhost:3000 或 https://api.your-domain.com",
                    style: "
                        width: 100%;
                        padding: 0.75rem;
                        border: 1px solid #ddd;
                        border-radius: 4px;
                        font-size: 1rem;
                        &:focus {{
                            outline: none;
                            border-color: #3498db;
                            box-shadow: 0 0 0 2px rgba(52, 152, 219, 0.2);
                        }}
                    ",
                    oninput: move |evt| {
                        config_mutate(|c| c.api_base_url = evt.value());
                    }
                }
                p {
                    style: "
                        font-size: 0.875rem;
                        color: #888;
                        margin-top: 0.5rem;
                        margin-bottom: 0;
                    ",
                    "API 请求会发送到这个地址，当后端部署在其他域名或端口时需要修改。"
                }
            }

            // 操作按钮区域
            div {
                style: "
                    display: flex;
                    gap: 1rem;
                    align-items: center;
                    margin-top: 2rem;
                    padding-top: 1.5rem;
                    border-top: 1px solid #eee;
                ",

                // 保存按钮
                button {
                    style: "
                        padding: 0.75rem 1.5rem;
                        background-color: #3498db;
                        color: white;
                        border: none;
                        border-radius: 4px;
                        font-size: 1rem;
                        cursor: pointer;
                        transition: background-color 0.2s;
                        &:hover {{
                            background-color: #2980b9;
                        }}
                    ",
                    onclick: move |_| {
                        match config().save() {
                            Ok(_) => {
                                save_status.set(Some("ok:配置已保存到本地存储，请刷新页面使新配置生效。".to_string()));
                            },
                            Err(e) => {
                                save_status.set(Some(format!("err:保存失败: {}", e)));
                            }
                        }
                    },
                    "保存配置"
                }

                // 重置按钮
                button {
                    style: "
                        padding: 0.75rem 1.5rem;
                        background-color: #e74c3c;
                        color: white;
                        border: none;
                        border-radius: 4px;
                        font-size: 1rem;
                        cursor: pointer;
                        transition: background-color 0.2s;
                        &:hover {{
                            background-color: #c0392b;
                        }}
                    ",
                    onclick: move |_| {
                        if let Some(window) = web_sys::window() {
                            let confirmed = window.confirm_with_message("确定要重置为默认配置吗？").unwrap_or(true);
                            if confirmed {
                                config.write().reset_to_default();
                                let _ = config().save();
                                save_status.set(Some("reset:已重置为编译时默认配置，请刷新页面。".to_string()));
                            }
                        }
                    },
                    "重置为默认"
                }
            }

            // 状态消息
            if let Some((style, display_status)) = status_content {
                div {
                    style: "{style}",
                    "{display_status}"
                }
            }

            // 当前状态信息
            div {
                style: "
                    margin-top: 2rem;
                    padding: 1rem;
                    background-color: #f8f9fa;
                    border-radius: 4px;
                    font-size: 0.875rem;
                ",
                p {
                    style: "margin: 0 0 0.5rem 0; color: #555;",
                    "当前实际生效 API 地址: "
                    code {
                        style: "
                            background: #e9ecef;
                            padding: 0.125rem 0.375rem;
                            border-radius: 3px;
                            color: #2c3e50;
                        ",
                        "{config().api_base_url}"
                    }
                }
                p {
                    style: "margin: 0; color: #666;",
                    "修改保存后需要刷新页面才能生效。"
                }
            }
        }
    }
}

/// 辅助函数：修改配置
fn config_mutate<F: FnOnce(&mut FrontendConfig)>(f: F) {
    let mut current = FrontendConfig::load();
    f(&mut current);
}
