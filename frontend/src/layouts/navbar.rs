//! 顶部导航栏

use dioxus::prelude::*;

use crate::pages::Route;
use crate::store::auth::use_auth_state;

#[component]
pub fn Navbar() -> Element {
    let mut hr_menu_open = use_signal(|| false);
    let mut finance_menu_open = use_signal(|| false);
    let mut project_menu_open = use_signal(|| false);
    let mut system_menu_open = use_signal(|| false);
    let mut user_menu_open = use_signal(|| false);
    let auth = use_auth_state();

    let mut close_all = move || {
        hr_menu_open.set(false);
        finance_menu_open.set(false);
        project_menu_open.set(false);
        system_menu_open.set(false);
        user_menu_open.set(false);
    };

    let username = if auth().username.is_empty() {
        "用户".to_string()
    } else {
        auth().username.clone()
    };
    let is_admin = auth().is_admin();

    rsx! {
        nav { class: "navbar",
            // 左侧：品牌 + 导航
            div { class: "navbar-section",
                Link { to: Route::MessageChat {}, class: "navbar-brand", "AI Orz" }

                Link { to: Route::MessageChat {}, class: "navbar-item", "💬 对话" }
                Link { to: Route::MessageSearch {}, class: "navbar-item", "🔍 消息搜索" }

                // 人力资源
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); hr_menu_open.set(!hr_menu_open()); },
                        "人力资源"
                        span { " ▾" }
                    }
                    if hr_menu_open() {
                        div { class: "navbar-dropdown",
                            Link { to: Route::HrAgents {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "Agent 管理"
                            }
                            Link { to: Route::HrSkills {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "技能库"
                            }
                            Link { to: Route::HrMemorySearch {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "记忆搜索"
                            }
                            Link { to: Route::HrKnowledgeGraph {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "知识图谱"
                            }
                        }
                    }
                }

                // 财务管理
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); finance_menu_open.set(!finance_menu_open()); },
                        "财务管理"
                        span { " ▾" }
                    }
                    if finance_menu_open() {
                        div { class: "navbar-dropdown",
                            Link { to: Route::FinanceModelProviders {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "模型提供商"
                            }
                            Link { to: Route::FinanceTools {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "工具管理"
                            }
                            Link { to: Route::FinanceMessageChannels {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "消息渠道"
                            }
                            Link { to: Route::FinanceAttachments {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "附件管理"
                            }
                            Link { to: Route::FinanceMcpServers {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "MCP 服务器"
                            }
                        }
                    }
                }

                // 项目管理
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); project_menu_open.set(!project_menu_open()); },
                        "项目管理"
                        span { " ▾" }
                    }
                    if project_menu_open() {
                        div { class: "navbar-dropdown",
                            Link { to: Route::ProjectList {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "项目列表"
                            }
                            Link { to: Route::ProjectArtifacts {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "项目产物"
                            }
                        }
                    }
                }

                // 系统管理
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); system_menu_open.set(!system_menu_open()); },
                        "系统"
                        span { " ▾" }
                    }
                    if system_menu_open() {
                        div { class: "navbar-dropdown",
                            Link { to: Route::SystemTriggers {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "定时触发器"
                            }
                            Link { to: Route::SystemHealth {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "健康检查"
                            }
                        }
                    }
                }
            }

            // 右侧：用户菜单
            div { class: "navbar-section",
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); user_menu_open.set(!user_menu_open()); },
                        span { style: "background: var(--color-mistral-orange); width: 28px; height: 28px; border-radius: 50%; display: flex; align-items: center; justify-content: center; color: white; font-weight: bold; font-size: 13px;",
                            "{username.chars().next().unwrap_or('U')}"
                        }
                        span { "{username}" }
                        span { " ▾" }
                    }
                    if user_menu_open() {
                        div { class: "navbar-dropdown", style: "right: 0; left: auto;",
                            Link { to: Route::UserProfile {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "👤 个人信息"
                            }
                            if is_admin {
                                Link { to: Route::OrganizationInfo {}, class: "navbar-dropdown-item",
                                    onclick: move |_| close_all(),
                                    "🏢 组织信息"
                                }
                                Link { to: Route::OrganizationUsers {}, class: "navbar-dropdown-item",
                                    onclick: move |_| close_all(),
                                    "👥 用户管理"
                                }
                            }
                            div { style: "border-top: 1px solid var(--color-border-light);" }
                            Link { to: Route::Settings {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "⚙️ 设置"
                            }
                            Link { to: Route::Reception {}, class: "navbar-dropdown-item",
                                onclick: move |_| {
                                    close_all();
                                    crate::store::auth::clear_token();
                                },
                                "🚪 退出登录"
                            }
                        }
                    }
                }
            }
        }
    }
}
