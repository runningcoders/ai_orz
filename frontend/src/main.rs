use dioxus::prelude::*;

#[derive(Clone, Debug)]
struct HealthStatus {
    checked: bool,
    healthy: bool,
    message: String,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            checked: false,
            healthy: false,
            message: "未检查".to_string(),
        }
    }
}

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    let mut health_status = use_signal(HealthStatus::default);

    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        div {
            style: "
                max-width: 800px;
                margin: 0 auto;
                padding: 2rem;
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            ",
            h1 {
                style: "color: #2c3e50; margin-bottom: 2rem; text-align: center;",
                "AI Orz"
            }
            p {
                style: "text-align: center; color: #666; margin-bottom: 3rem;",
                "AI 代理执行框架"
            }

            div {
                style: "
                    background: #f8f9fa;
                    border-radius: 8px;
                    padding: 2rem;
                    margin-bottom: 2rem;
                    box-shadow: 0 2px 4px rgba(0,0,0,0.1);
                ",

                h2 { style: "margin-top: 0; color: #333; font-size: 1.2rem;", "后端健康检查" }

                button {
                    style: "
                        background: #007bff;
                        color: white;
                        border: none;
                        padding: 0.75rem 1.5rem;
                        border-radius: 4px;
                        cursor: pointer;
                        font-size: 1rem;
                        margin: 1rem 0;
                    ",
                    onclick: move |_| async move {
                        let mut status = health_status.write();
                        status.checked = true;
                        status.healthy = false;
                        status.message = "检查中...".to_string();

                        // 调用后端健康检查接口
                        match fetch_health().await {
                            Ok(ok) => {
                                status.healthy = true;
                                status.message = ok;
                            }
                            Err(e) => {
                                status.healthy = false;
                                status.message = format!("检查失败: {}", e);
                            }
                        }
                    },
                    "检查健康状态"
                }

                div {
                    style: "margin-top: 1rem;",
                    if health_status().checked {
                        if health_status().healthy {
                            div {
                                style: "
                                    background: #d4edda;
                                    color: #155724;
                                    padding: 1rem;
                                    border-radius: 4px;
                                ",
                                "✅ " { health_status().message }
                            }
                        } else {
                            div {
                                style: "
                                    background: #f8d7da;
                                    color: #721c24;
                                    padding: 1rem;
                                    border-radius: 4px;
                                ",
                                "❌ " { health_status().message }
                            }
                        }
                    } else {
                        div {
                            style: "color: #6c757d; padding: 1rem 0;",
                            "点击上方按钮检查后端连接状态"
                        }
                    }
                }
            }

            footer {
                style: "text-align: center; color: #999; margin-top: 3rem; font-size: 0.9rem;",
                "Built with Dioxus + Rust"
            }
        }
    }
}

async fn fetch_health() -> Result<String, String> {
    // 从环境变量读取后端 API 地址，默认 http://localhost:3000
    let backend_url = option_env!("BACKEND_API_URL").unwrap_or("http://localhost:3000");
    let url = format!("{}/health", backend_url);

    let client = reqwest::Client::new();
    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        let text = response.text().await.map_err(|e: reqwest::Error| e.to_string())?;
        Ok(format!("后端服务正常: {}", text))
    } else {
        Err(format!("HTTP 错误: {}", response.status()))
    }
}
