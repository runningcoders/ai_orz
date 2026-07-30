//! 顶部导航栏

use dioxus::prelude::*;

use crate::api::auth::logout as api_logout;
use crate::hooks::use_breakpoint;
use crate::pages::Route;
use crate::store::auth::{logout, use_auth_state};

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

    // 修复 HIGH #9 + #8：之前登出仅 clear_login_state()，不更新 AuthState 信号，
    // 也不调用后端 logout API 使 cookie 失效。现在统一调用 logout() 清内存+localStorage，
    // 并调用后端 logout 接口清除 HttpOnly cookie。
    let handle_logout = move |_| {
        // 关闭所有菜单
        hr_menu_open.set(false);
        finance_menu_open.set(false);
        project_menu_open.set(false);
        system_menu_open.set(false);
        user_menu_open.set(false);
        drawer_open.set(false);
        // 调用后端 logout 清除 cookie（不阻塞前端跳转）
        spawn(async move {
            let _ = api_logout().await;
        });
        // 清前端状态（内存信号 + localStorage）
        logout();
    };

    let username = if auth().username.is_empty() {
        "用户".to_string()
    } else {
        auth().username.clone()
    };
    let is_admin = auth().is_admin();
    let avatar_char = username
        .chars()
        .next()
        .unwrap_or('U')
        .to_string()
        .to_uppercase();

    rsx! {
        nav { class: "navbar bg-neutral text-neutral-content sticky top-0 z-50 shadow-md",
            // 左侧：品牌
            div { class: "flex-1",
                Link { to: Route::MessageChat {}, class: "text-yellow-300 font-bold text-lg tracking-tight cursor-pointer", "AI Orz" }
            }

            // 中间：桌面导航
            if !is_mobile() {
                div { class: "flex-none",
                    Link { to: Route::MessageChat {}, class: "btn btn-ghost btn-sm text-neutral-content", "💬 对话" }
                    Link { to: Route::MessageSearch {}, class: "btn btn-ghost btn-sm text-neutral-content", "🔍 消息搜索" }
                    Link { to: Route::Workspace {}, class: "btn btn-ghost btn-sm text-neutral-content", "🚀 工作台" }

                    // 人力资源
                    div { class: "dropdown dropdown-end relative",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content",
                            onclick: move |_| {
                                finance_menu_open.set(false);
                                project_menu_open.set(false);
                                system_menu_open.set(false);
                                user_menu_open.set(false);
                                hr_menu_open.set(!hr_menu_open());
                            },
                            "人力资源",
                            span { " ▾" }
                        }
                        if hr_menu_open() {
                            ul {
                                tabindex: 0,
                                class: "dropdown-content menu absolute top-full right-0 bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content mt-1",
                                li { class: "menu-title", span { "人力资源" } }
                                li {
                                    Link {
                                        to: Route::HrAgents {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "Agent 管理"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::HrSkills {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "技能库"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::HrMemorySearch {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "记忆搜索"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::HrKnowledgeGraph {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "知识图谱"
                                    }
                                }
                            }
                        }
                    }

                    // 财务管理
                    div { class: "dropdown dropdown-end relative",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content",
                            onclick: move |_| {
                                hr_menu_open.set(false);
                                project_menu_open.set(false);
                                system_menu_open.set(false);
                                user_menu_open.set(false);
                                finance_menu_open.set(!finance_menu_open());
                            },
                            "财务管理",
                            span { " ▾" }
                        }
                        if finance_menu_open() {
                            ul {
                                tabindex: 0,
                                class: "dropdown-content menu absolute top-full right-0 bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content mt-1",
                                li { class: "menu-title", span { "财务管理" } }
                                li {
                                    Link {
                                        to: Route::FinanceModelProviders {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "模型提供商"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::FinanceTools {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "工具管理"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::FinanceMessageChannels {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "消息渠道"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::FinanceAttachments {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "附件管理"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::FinanceMcpServers {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "MCP 服务器"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::FinanceToolCallEntries {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "📋 工具调用记录"
                                    }
                                }
                            }
                        }
                    }

                    // 项目管理
                    div { class: "dropdown dropdown-end relative",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content",
                            onclick: move |_| {
                                hr_menu_open.set(false);
                                finance_menu_open.set(false);
                                system_menu_open.set(false);
                                user_menu_open.set(false);
                                project_menu_open.set(!project_menu_open());
                            },
                            "项目管理",
                            span { " ▾" }
                        }
                        if project_menu_open() {
                            ul {
                                tabindex: 0,
                                class: "dropdown-content menu absolute top-full right-0 bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content mt-1",
                                li { class: "menu-title", span { "项目管理" } }
                                li {
                                    Link {
                                        to: Route::ProjectList {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "项目列表"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::ProjectArtifacts {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "项目产物"
                                    }
                                }
                            }
                        }
                    }

                    // 系统管理
                    div { class: "dropdown dropdown-end relative",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content",
                            onclick: move |_| {
                                hr_menu_open.set(false);
                                finance_menu_open.set(false);
                                project_menu_open.set(false);
                                user_menu_open.set(false);
                                system_menu_open.set(!system_menu_open());
                            },
                            "系统",
                            span { " ▾" }
                        }
                        if system_menu_open() {
                            ul {
                                tabindex: 0,
                                class: "dropdown-content menu absolute top-full right-0 bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content mt-1",
                                li { class: "menu-title", span { "系统" } }
                                li {
                                    Link {
                                        to: Route::SystemTriggers {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "定时触发器"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::SystemHealth {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "健康检查"
                                    }
                                }
                                if is_admin {
                                    li {
                                        hr { class: "divider my-0" }
                                    }
                                    li {
                                        Link {
                                            to: Route::SystemLogs {},
                                            onclick: move |_| {
                                                hr_menu_open.set(false);
                                                finance_menu_open.set(false);
                                                project_menu_open.set(false);
                                                system_menu_open.set(false);
                                                user_menu_open.set(false);
                                            },
                                            "日志查询"
                                        }
                                    }
                                    li {
                                        Link {
                                            to: Route::SystemBackup {},
                                            onclick: move |_| {
                                                hr_menu_open.set(false);
                                                finance_menu_open.set(false);
                                                project_menu_open.set(false);
                                                system_menu_open.set(false);
                                                user_menu_open.set(false);
                                            },
                                            "备份管理"
                                        }
                                    }
                                    li {
                                        Link {
                                            to: Route::SystemAop {},
                                            onclick: move |_| {
                                                hr_menu_open.set(false);
                                                finance_menu_open.set(false);
                                                project_menu_open.set(false);
                                                system_menu_open.set(false);
                                                user_menu_open.set(false);
                                            },
                                            "AOP 监控"
                                        }
                                    }
                                    li {
                                        Link {
                                            to: Route::SystemSeed {},
                                            onclick: move |_| {
                                                hr_menu_open.set(false);
                                                finance_menu_open.set(false);
                                                project_menu_open.set(false);
                                                system_menu_open.set(false);
                                                user_menu_open.set(false);
                                            },
                                            "Seed 配置迁移"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 右侧：桌面用户菜单 / 移动端汉堡按钮
            div { class: "flex-none",
                if !is_mobile() {
                    // 桌面端用户菜单
                    div { class: "dropdown dropdown-end relative",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content gap-2",
                            onclick: move |_| {
                                hr_menu_open.set(false);
                                finance_menu_open.set(false);
                                project_menu_open.set(false);
                                system_menu_open.set(false);
                                user_menu_open.set(!user_menu_open());
                            },
                            div { class: "avatar",
                                div { class: "w-8 rounded-full bg-primary text-primary-content flex items-center justify-center text-sm font-bold", "{avatar_char}" }
                            }
                            span { "{username}" }
                            span { " ▾" }
                        }
                        if user_menu_open() {
                            ul {
                                tabindex: 0,
                                class: "dropdown-content menu absolute top-full right-0 bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content mt-1",
                                li { class: "menu-title", span { "账户" } }
                                li {
                                    Link {
                                        to: Route::UserProfile {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "👤 个人信息"
                                    }
                                }
                                if is_admin {
                                    li {
                                        Link {
                                            to: Route::OrganizationInfo {},
                                            onclick: move |_| {
                                                hr_menu_open.set(false);
                                                finance_menu_open.set(false);
                                                project_menu_open.set(false);
                                                system_menu_open.set(false);
                                                user_menu_open.set(false);
                                            },
                                            "🏢 组织信息"
                                        }
                                    }
                                    li {
                                        Link {
                                            to: Route::OrganizationUsers {},
                                            onclick: move |_| {
                                                hr_menu_open.set(false);
                                                finance_menu_open.set(false);
                                                project_menu_open.set(false);
                                                system_menu_open.set(false);
                                                user_menu_open.set(false);
                                            },
                                            "👥 用户管理"
                                        }
                                    }
                                }
                                li {
                                    hr { class: "divider my-0" }
                                }
                                li {
                                    Link {
                                        to: Route::Settings {},
                                        onclick: move |_| {
                                            hr_menu_open.set(false);
                                            finance_menu_open.set(false);
                                            project_menu_open.set(false);
                                            system_menu_open.set(false);
                                            user_menu_open.set(false);
                                        },
                                        "⚙️ 设置"
                                    }
                                }
                                li {
                                    Link {
                                        to: Route::Reception {},
                                        onclick: handle_logout,
                                        "🚪 退出登录"
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // 移动端汉堡按钮
                    label {
                        r#for: "mobile-drawer",
                        class: "btn btn-square btn-ghost text-neutral-content",
                        onclick: move |_| drawer_open.set(true),
                        "☰"
                    }
                }
            }
        }

        // 移动端抽屉
        if is_mobile() {
            div { class: "drawer drawer-end",
                input {
                    id: "mobile-drawer",
                    r#type: "checkbox",
                    class: "drawer-toggle",
                    checked: drawer_open(),
                    onchange: move |e| drawer_open.set(e.checked()),
                }
                div { class: "drawer-side z-50",
                    label {
                        r#for: "mobile-drawer",
                        class: "drawer-overlay",
                        onclick: move |_| drawer_open.set(false),
                    }
                    ul { class: "menu bg-base-100 text-base-content min-h-full w-80 p-4",
                        // 用户信息头部
                        li { class: "mb-4",
                            div { class: "flex items-center gap-3 py-2",
                                div { class: "avatar",
                                    div { class: "w-12 rounded-full bg-primary text-primary-content flex items-center justify-center text-lg font-bold", "{avatar_char}" }
                                }
                                div { class: "font-semibold text-lg", "{username}" }
                            }
                        }

                        li { class: "menu-title", span { "导航" } }
                        li {
                            Link {
                                to: Route::MessageChat {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "💬 对话"
                            }
                        }
                        li {
                            Link {
                                to: Route::MessageSearch {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "🔍 消息搜索"
                            }
                        }
                        li {
                            Link {
                                to: Route::Workspace {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "🚀 工作台"
                            }
                        }

                        li { class: "menu-title", span { "人力资源" } }
                        li {
                            Link {
                                to: Route::HrAgents {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "Agent 管理"
                            }
                        }
                        li {
                            Link {
                                to: Route::HrSkills {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "技能库"
                            }
                        }
                        li {
                            Link {
                                to: Route::HrMemorySearch {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "记忆搜索"
                            }
                        }
                        li {
                            Link {
                                to: Route::HrKnowledgeGraph {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "知识图谱"
                            }
                        }

                        li { class: "menu-title", span { "财务管理" } }
                        li {
                            Link {
                                to: Route::FinanceModelProviders {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "模型提供商"
                            }
                        }
                        li {
                            Link {
                                to: Route::FinanceTools {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "工具管理"
                            }
                        }
                        li {
                            Link {
                                to: Route::FinanceMessageChannels {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "消息渠道"
                            }
                        }
                        li {
                            Link {
                                to: Route::FinanceAttachments {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "附件管理"
                            }
                        }
                        li {
                            Link {
                                to: Route::FinanceMcpServers {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "MCP 服务器"
                            }
                        }
                        li {
                            Link {
                                to: Route::FinanceToolCallEntries {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "📋 工具调用记录"
                            }
                        }

                        li { class: "menu-title", span { "项目管理" } }
                        li {
                            Link {
                                to: Route::ProjectList {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "项目列表"
                            }
                        }
                        li {
                            Link {
                                to: Route::ProjectArtifacts {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "项目产物"
                            }
                        }

                        li { class: "menu-title", span { "系统" } }
                        li {
                            Link {
                                to: Route::SystemTriggers {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "定时触发器"
                            }
                        }
                        li {
                            Link {
                                to: Route::SystemHealth {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "健康检查"
                            }
                        }
                        if is_admin {
                            li {
                                Link {
                                    to: Route::SystemLogs {},
                                    onclick: move |_| {
                                        hr_menu_open.set(false);
                                        finance_menu_open.set(false);
                                        project_menu_open.set(false);
                                        system_menu_open.set(false);
                                        user_menu_open.set(false);
                                        drawer_open.set(false);
                                    },
                                    "日志查询"
                                }
                            }
                            li {
                                Link {
                                    to: Route::SystemBackup {},
                                    onclick: move |_| {
                                        hr_menu_open.set(false);
                                        finance_menu_open.set(false);
                                        project_menu_open.set(false);
                                        system_menu_open.set(false);
                                        user_menu_open.set(false);
                                        drawer_open.set(false);
                                    },
                                    "备份管理"
                                }
                            }
                            li {
                                Link {
                                    to: Route::SystemAop {},
                                    onclick: move |_| {
                                        hr_menu_open.set(false);
                                        finance_menu_open.set(false);
                                        project_menu_open.set(false);
                                        system_menu_open.set(false);
                                        user_menu_open.set(false);
                                        drawer_open.set(false);
                                    },
                                    "AOP 监控"
                                }
                            }
                            li {
                                Link {
                                    to: Route::SystemSeed {},
                                    onclick: move |_| {
                                        hr_menu_open.set(false);
                                        finance_menu_open.set(false);
                                        project_menu_open.set(false);
                                        system_menu_open.set(false);
                                        user_menu_open.set(false);
                                        drawer_open.set(false);
                                    },
                                    "Seed 配置迁移"
                                }
                            }
                        }

                        li {
                            hr { class: "divider my-0" }
                        }
                        li { class: "menu-title", span { "账户" } }
                        li {
                            Link {
                                to: Route::UserProfile {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "👤 个人信息"
                            }
                        }
                        if is_admin {
                            li {
                                Link {
                                    to: Route::OrganizationInfo {},
                                    onclick: move |_| {
                                        hr_menu_open.set(false);
                                        finance_menu_open.set(false);
                                        project_menu_open.set(false);
                                        system_menu_open.set(false);
                                        user_menu_open.set(false);
                                        drawer_open.set(false);
                                    },
                                    "🏢 组织信息"
                                }
                            }
                            li {
                                Link {
                                    to: Route::OrganizationUsers {},
                                    onclick: move |_| {
                                        hr_menu_open.set(false);
                                        finance_menu_open.set(false);
                                        project_menu_open.set(false);
                                        system_menu_open.set(false);
                                        user_menu_open.set(false);
                                        drawer_open.set(false);
                                    },
                                    "👥 用户管理"
                                }
                            }
                        }
                        li {
                            Link {
                                to: Route::Settings {},
                                onclick: move |_| {
                                    hr_menu_open.set(false);
                                    finance_menu_open.set(false);
                                    project_menu_open.set(false);
                                    system_menu_open.set(false);
                                    user_menu_open.set(false);
                                    drawer_open.set(false);
                                },
                                "⚙️ 设置"
                            }
                        }
                        li {
                            Link {
                                to: Route::Reception {},
                                onclick: handle_logout,
                                "🚪 退出登录"
                            }
                        }
                    }
                }
            }
        }
    }
}
