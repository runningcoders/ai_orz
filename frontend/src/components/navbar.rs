use dioxus::prelude::*;
use crate::Page;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserRole {
    Member,
    Admin,
    SuperAdmin,
}

impl UserRole {
    pub fn is_admin_or_above(self) -> bool {
        matches!(self, UserRole::Admin | UserRole::SuperAdmin)
    }
}

#[component]
pub fn Navbar(on_navigate: EventHandler<Page>) -> Element {
    let mut hr_menu_open = use_signal(|| false);
    let mut finance_menu_open = use_signal(|| false);
    let mut user_menu_open = use_signal(|| false);
    
    // TODO: 从登录状态获取真实用户信息和角色
    // 暂时使用占位值，后续对接后端获取
    let current_username = "admin";
    let current_role = UserRole::SuperAdmin; // 暂时默认超级管理员，实际从登录状态获取

    rsx! {
        nav {
            style: "
                background-color: #2c3e50;
                padding: 1rem 2rem;
                display: flex;
                align-items: center;
                justify-content: space-between;
                box-shadow: 0 2px 4px rgba(0,0,0,0.1);
                position: relative;
                z-index: 100;
            ",
            // 左侧：品牌 + 导航链接
            div {
                style: "display: flex; align-items: center; gap: 2rem;",
                span {
                    style: "color: white; font-size: 1.25rem; font-weight: bold;",
                    "AI Orz"
                }
                a {
                    style: "
                        color: #ecf0f1;
                        text-decoration: none;
                        padding: 0.5rem 1rem;
                        border-radius: 4px;
                        transition: background-color 0.2s;
                        cursor: pointer;
                        &:hover {{
                            background-color: rgba(255,255,255,0.1);
                        }}
                    ",
                    onclick: move |_| {
                        on_navigate.call(Page::Reception);
                        hr_menu_open.set(false);
                        finance_menu_open.set(false);
                        user_menu_open.set(false);
                    },
                    "前台接待"
                }

                // 人力资源 - 带下拉菜单
                div {
                    style: "position: relative;",
                    button {
                        style: "
                            color: #ecf0f1;
                            background: transparent;
                            border: none;
                            padding: 0.5rem 1rem;
                            border-radius: 4px;
                            cursor: pointer;
                            font-size: 1rem;
                            transition: background-color 0.2s;
                            display: flex;
                            align-items: center;
                            gap: 0.3rem;
                            &:hover {{
                                background-color: rgba(255,255,255,0.1);
                            }}
                        ",
                        onclick: move |_| {
                            hr_menu_open.set(!hr_menu_open());
                            finance_menu_open.set(false);
                            user_menu_open.set(false);
                        },
                        "人力资源"
                        span { "▼" }
                    }

                    if *hr_menu_open.read() {
                        div {
                            style: "
                                position: absolute;
                                top: 100%;
                                left: 0;
                                margin-top: 0.5rem;
                                background: white;
                                border-radius: 4px;
                                box-shadow: 0 4px 12px rgba(0,0,0,0.15);
                                min-width: 160px;
                                overflow: hidden;
                            ",
                            a {
                                style: "
                                    display: block;
                                    padding: 0.75rem 1rem;
                                    color: #333;
                                    text-decoration: none;
                                    transition: background-color 0.2s;
                                    cursor: pointer;
                                    &:hover {{
                                        background-color: #f5f5f5;
                                    }}
                                ",
                                onclick: move |_| {
                                    // TODO: 员工管理页面 - 后端设计好实体后再实现
                                    hr_menu_open.set(false);
                                },
                                "员工管理"
                            }
                            a {
                                style: "
                                    display: block;
                                    padding: 0.75rem 1rem;
                                    color: #333;
                                    text-decoration: none;
                                    transition: background-color 0.2s;
                                    cursor: pointer;
                                    &:hover {{
                                        background-color: #f5f5f5;
                                    }}
                                ",
                                onclick: move |_| {
                                    on_navigate.call(Page::AgentManagement);
                                    hr_menu_open.set(false);
                                },
                                "Agent 管理"
                            }
                        }
                    }
                }

                // 财务管理 - 带下拉菜单
                div {
                    style: "position: relative;",
                    button {
                        style: "
                            color: #ecf0f1;
                            background: transparent;
                            border: none;
                            padding: 0.5rem 1rem;
                            border-radius: 4px;
                            cursor: pointer;
                            font-size: 1rem;
                            transition: background-color 0.2s;
                            display: flex;
                            align-items: center;
                            gap: 0.3rem;
                            &:hover {{
                                background-color: rgba(255,255,255,0.1);
                            }}
                        ",
                        onclick: move |_| {
                            finance_menu_open.set(!finance_menu_open());
                            hr_menu_open.set(false);
                            user_menu_open.set(false);
                        },
                        "财务管理"
                        span { "▼" }
                    }

                    if *finance_menu_open.read() {
                        div {
                            style: "
                                position: absolute;
                                top: 100%;
                                left: 0;
                                margin-top: 0.5rem;
                                background: white;
                                border-radius: 4px;
                                box-shadow: 0 4px 12px rgba(0,0,0,0.15);
                                min-width: 160px;
                                overflow: hidden;
                            ",
                            a {
                                style: "
                                    display: block;
                                    padding: 0.75rem 1rem;
                                    color: #333;
                                    text-decoration: none;
                                    transition: background-color 0.2s;
                                    cursor: pointer;
                                    &:hover {{
                                        background-color: #f5f5f5;
                                    }}
                                ",
                                onclick: move |_| {
                                    on_navigate.call(Page::ModelProviderManagement);
                                    finance_menu_open.set(false);
                                },
                                "模型管理"
                            }
                        }
                    }
                }
            }

            // 右侧：健康状态指示器 + 用户状态栏
            div {
                style: "display: flex; align-items: center; gap: 1.5rem;",
                crate::components::HealthCheck {}

                // 用户下拉菜单
                div {
                    style: "position: relative;",
                    button {
                        style: "
                            color: #ecf0f1;
                            background: transparent;
                            border: none;
                            padding: 0.5rem 0.75rem;
                            border-radius: 4px;
                            cursor: pointer;
                            display: flex;
                            align-items: center;
                            gap: 0.5rem;
                            transition: background-color 0.2s;
                            &:hover {{
                                background-color: rgba(255,255,255,0.1);
                            }}
                        ",
                        onclick: move |_| {
                            user_menu_open.set(!user_menu_open());
                            hr_menu_open.set(false);
                            finance_menu_open.set(false);
                        },
                        div {
                            style: "
                                width: 32px;
                                height: 32px;
                                border-radius: 50%;
                                background: #3498db;
                                display: flex;
                                align-items: center;
                                justify-content: center;
                                color: white;
                                font-weight: bold;
                                font-size: 14px;
                            ",
                            // 用户名首字母作为头像
                            "{current_username.chars().next().unwrap_or_default()}"
                        }
                        span {
                            style: "
                                color: #ecf0f1;
                                font-size: 0.95rem;
                            ",
                            "{current_username}"
                        }
                        span { "▼" }
                    }

                    if *user_menu_open.read() {
                        div {
                            style: "
                                position: absolute;
                                top: 100%;
                                right: 0;
                                margin-top: 0.5rem;
                                background: white;
                                border-radius: 4px;
                                box-shadow: 0 4px 12px rgba(0,0,0,0.15);
                                min-width: 180px;
                                overflow: hidden;
                            ",
                            // 个人信息 - 所有用户可见
                            a {
                                style: "
                                    display: block;
                                    padding: 0.75rem 1rem;
                                    color: #333;
                                    text-decoration: none;
                                    transition: background-color 0.2s;
                                    cursor: pointer;
                                    &:hover {{
                                        background-color: #f5f5f5;
                                    }}
                                ",
                                onclick: move |_| {
                                    on_navigate.call(Page::UserProfile);
                                    user_menu_open.set(false);
                                },
                                "👤 个人信息"
                            }
                            // 组织信息 - 仅管理员可见
                            if current_role.is_admin_or_above() {
                                a {
                                    style: "
                                        display: block;
                                        padding: 0.75rem 1rem;
                                        color: #333;
                                        text-decoration: none;
                                        transition: background-color 0.2s;
                                        cursor: pointer;
                                        &:hover {{
                                            background-color: #f5f5f5;
                                        }}
                                    ",
                                    onclick: move |_| {
                                        on_navigate.call(Page::OrganizationInfo);
                                        user_menu_open.set(false);
                                    },
                                    "🏢 组织信息"
                                }
                            }
                            // 用户管理 - 仅管理员可见
                            if current_role.is_admin_or_above() {
                                a {
                                    style: "
                                        display: block;
                                        padding: 0.75rem 1rem;
                                        color: #333;
                                        text-decoration: none;
                                        transition: background-color 0.2s;
                                        cursor: pointer;
                                        &:hover {{
                                            background-color: #f5f5f5;
                                        }}
                                    ",
                                    onclick: move |_| {
                                        on_navigate.call(Page::UserManagement);
                                        user_menu_open.set(false);
                                    },
                                    "👥 用户管理"
                                }
                            }
                            // 设置 - 所有用户可见
                            div { style: "border-top: 1px solid #eee;" }
                            a {
                                style: "
                                    display: block;
                                    padding: 0.75rem 1rem;
                                    color: #333;
                                    text-decoration: none;
                                    transition: background-color 0.2s;
                                    cursor: pointer;
                                    &:hover {{
                                        background-color: #f5f5f5;
                                    }}
                                ",
                                onclick: move |_| {
                                    on_navigate.call(Page::SettingsPage);
                                    user_menu_open.set(false);
                                },
                                "⚙️ 设置"
                            }
                        }
                    }
                }
            }
        }
    }
}
