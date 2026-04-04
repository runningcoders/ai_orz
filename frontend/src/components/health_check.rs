use dioxus::prelude::*;
use crate::api::health::fetch_health;

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
            message: "未连接".to_string(),
        }
    }
}

#[component]
pub fn HealthCheck() -> Element {
    let mut health_status = use_signal(HealthStatus::default);

    rsx! {
        div {
            style: "
                display: flex;
                align-items: center;
                gap: 0.75rem;
            ",
            button {
                style: "
                    background: transparent;
                    color: #ecf0f1;
                    border: 1px solid #ecf0f1;
                    padding: 0.4rem 0.8rem;
                    border-radius: 4px;
                    cursor: pointer;
                    font-size: 0.875rem;
                    transition: all 0.2s;
                    &:hover {{
                        background-color: rgba(255, 255, 255, 0.1);
                    }}
                ",
                onclick: move |_| async move {
                    let mut status = health_status.write();
                    status.checked = true;
                    status.healthy = false;
                    status.message = "检查中...".to_string();

                    match fetch_health().await {
                        Ok(ok) => {
                            status.healthy = true;
                            status.message = ok;
                        }
                        Err(e) => {
                            status.healthy = false;
                            status.message = format!("失败: {}", e);
                        }
                    }
                },
                "检查"
            }

            {
                let status = health_status.read();
                match *status {
                    HealthStatus { checked: false, .. } => rsx! {
                        div {
                            style: "color: #f39c12; font-size: 0.875rem;",
                            "? 未检查"
                        }
                    },
                    HealthStatus { checked: true, healthy: true, .. } => rsx! {
                        div {
                            style: "color: #2ecc71; font-size: 0.875rem;",
                            "✅ OK"
                        }
                    },
                    HealthStatus { checked: true, healthy: false, ref message, .. } => rsx! {
                        div {
                            style: "color: #e74c3c; font-size: 0.875rem;",
                            "❌ " {message.clone()}
                        }
                    },
                }
            }
        }
    }
}
