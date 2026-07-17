//! 顶部导航栏

use dioxus::prelude::*;

use crate::hooks::use_breakpoint;
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
    let is_mobile = use_breakpoint();
    let mut drawer_open = use_signal(|| false);

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

    // 桌面端 Navbar（与原实现完全一致，仅在 !is_mobile() 时渲染）
    let desktop_navbar = rsx! {
        div { class: "navbar-desktop-only navbar-section",
            Link { to: Route::MessageChat {}, class: "navbar-item", "💬 对话" }
            Link { to: Route::MessageSearch {}, class: "navbar-item", "🔍 消息搜索" }

            // 人力资源
            div { class: "navbar-dropdown-container",
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
            div { class: "navbar-dropdown-container",
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
            div { class: "navbar-dropdown-container",
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
            div { class: "navbar-dropdown-container",
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
                        if is_admin {
                            div { class: "navbar-divider" }
                            Link { to: Route::SystemLogs {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "日志查询"
                            }
                            Link { to: Route::SystemBackup {}, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "备份管理"
                            }
                        }
                    }
                }
            }
        }
    };

    // 桌面端右侧用户菜单（与原实现完全一致）
    let desktop_user_menu = rsx! {
        div { class: "navbar-desktop-only navbar-section",
            div { class: "navbar-dropdown-container",
                button {
                    class: "navbar-item",
                    onclick: move |_| { close_all(); user_menu_open.set(!user_menu_open()); },
                    span { class: "navbar-avatar",
                        "{username.chars().next().unwrap_or('U')}"
                    }
                    span { "{username}" }
                    span { " ▾" }
                }
                if user_menu_open() {
                    div { class: "navbar-dropdown navbar-dropdown-right",
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
                        div { class: "navbar-divider" }
                        Link { to: Route::Settings {}, class: "navbar-dropdown-item",
                            onclick: move |_| close_all(),
                            "⚙️ 设置"
                        }
                        Link { to: Route::Reception {}, class: "navbar-dropdown-item",
                            onclick: move |_| {
                                close_all();
                                crate::store::auth::clear_login_state();
                            },
                            "🚪 退出登录"
                        }
                    }
                }
            }
        }
    };

    rsx! {
        nav { class: "navbar",
            // 左侧：品牌 + 桌面导航 + 移动端汉堡按钮
            div { class: "navbar-section",
                Link { to: Route::MessageChat {}, class: "navbar-brand", "AI Orz" }
                {desktop_navbar}
                if is_mobile() {
                    button {
                        class: "navbar-mobile-toggle",
                        onclick: move |_| drawer_open.set(true),
                        "☰"
                    }
                }
            }

            // 右侧：桌面用户菜单 / 移动端头像（点击打开抽屉）
            if !is_mobile() {
                {desktop_user_menu}
            } else {
                button {
                    class: "navbar-mobile-toggle",
                    onclick: move |_| drawer_open.set(true),
                    style: "font-size: 20px;",
                    span { class: "navbar-avatar",
                        "{username.chars().next().unwrap_or('U')}"
                    }
                }
            }
        }

        // 移动端抽屉
        if is_mobile() && drawer_open() {
            div { class: "navbar-overlay", onclick: move |_| drawer_open.set(false) }
            div { class: "navbar-drawer open",
                div { class: "navbar-drawer-section", "导航" }
                Link { to: Route::MessageChat {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "💬 对话" }
                Link { to: Route::MessageSearch {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "🔍 消息搜索" }

                div { class: "navbar-drawer-section", "人力资源" }
                Link { to: Route::HrAgents {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "Agent 管理" }
                Link { to: Route::HrSkills {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "技能库" }
                Link { to: Route::HrMemorySearch {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "记忆搜索" }
                Link { to: Route::HrKnowledgeGraph {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "知识图谱" }

                div { class: "navbar-drawer-section", "财务管理" }
                Link { to: Route::FinanceModelProviders {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "模型提供商" }
                Link { to: Route::FinanceTools {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "工具管理" }
                Link { to: Route::FinanceMessageChannels {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "消息渠道" }
                Link { to: Route::FinanceAttachments {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "附件管理" }
                Link { to: Route::FinanceMcpServers {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "MCP 服务器" }

                div { class: "navbar-drawer-section", "项目管理" }
                Link { to: Route::ProjectList {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "项目列表" }
                Link { to: Route::ProjectArtifacts {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "项目产物" }

                div { class: "navbar-drawer-section", "系统" }
                Link { to: Route::SystemTriggers {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "定时触发器" }
                Link { to: Route::SystemHealth {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "健康检查" }
                if is_admin {
                    Link { to: Route::SystemLogs {}, class: "navbar-drawer-item",
                        onclick: move |_| drawer_open.set(false), "日志查询" }
                    Link { to: Route::SystemBackup {}, class: "navbar-drawer-item",
                        onclick: move |_| drawer_open.set(false), "备份管理" }
                }

                div { class: "navbar-drawer-divider" }
                div { class: "navbar-drawer-section", "账户" }
                Link { to: Route::UserProfile {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "👤 个人信息" }
                if is_admin {
                    Link { to: Route::OrganizationInfo {}, class: "navbar-drawer-item",
                        onclick: move |_| drawer_open.set(false), "🏢 组织信息" }
                    Link { to: Route::OrganizationUsers {}, class: "navbar-drawer-item",
                        onclick: move |_| drawer_open.set(false), "👥 用户管理" }
                }
                Link { to: Route::Settings {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "⚙️ 设置" }
                Link { to: Route::Reception {}, class: "navbar-drawer-item",
                    onclick: move |_| {
                        drawer_open.set(false);
                        crate::store::auth::clear_login_state();
                    }, "🚪 退出登录" }
            }
        }
    }
}
