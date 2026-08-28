//! 顶部导航栏

use dioxus::prelude::*;

use crate::api::auth::logout as api_logout;
use crate::hooks::use_breakpoint;
use crate::pages::Route;
use crate::store::auth::{logout, use_auth_state};

/// 根据任意字符串生成稳定的 avatar 背景色（纯前端 hash，零外部依赖）
///
/// 色板来自 Tailwind 色板，全部是明亮饱和色，适合白底文字
fn avatar_color(seed: &str) -> &'static str {
    let palette = [
        "bg-rose-500",
        "bg-pink-500",
        "bg-fuchsia-500",
        "bg-purple-500",
        "bg-indigo-500",
        "bg-blue-500",
        "bg-sky-500",
        "bg-cyan-500",
        "bg-teal-500",
        "bg-emerald-500",
        "bg-green-500",
        "bg-lime-500",
        "bg-yellow-500",
        "bg-amber-500",
        "bg-orange-500",
        "bg-red-500",
        "bg-violet-500",
        "bg-blue-600",
        "bg-emerald-600",
        "bg-amber-600",
    ];
    // 简单 FNV-1a 风格 hash：累计乘 31 加字节
    let mut hash: u32 = 2166136261;
    for b in seed.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    palette[(hash as usize) % palette.len()]
}

#[component]
pub fn Navbar() -> Element {
    let auth = use_auth_state();
    let is_mobile = use_breakpoint();
    let mut drawer_open = use_signal(|| false);

    let handle_logout = move |_| {
        drawer_open.set(false);
        spawn(async move {
            let _ = api_logout().await;
        });
        logout(auth);
    };

    let label = auth().display_label().to_string();
    let avatar_char = label
        .chars()
        .next()
        .unwrap_or('U')
        .to_string()
        .to_uppercase();
    let avatar_bg = avatar_color(&label);
    let is_admin = auth().is_admin();

    rsx! {
        nav { class: "navbar bg-neutral text-neutral-content sticky top-0 z-50 shadow-md",
            // 左侧：品牌
            div { class: "flex-1",
                Link { to: Route::MessageChat {}, class: "orz-brand-logo text-lg", "AI Orz" }
            }

            // 中间：桌面导航
            if !is_mobile() {
                div { class: "flex-none",
                    Link { to: Route::MessageChat {}, class: "btn btn-ghost btn-sm text-neutral-content", "💬 对话" }
                    Link { to: Route::MessageSearch {}, class: "btn btn-ghost btn-sm text-neutral-content", "🔍 消息搜索" }
                    Link { to: Route::Workspace {}, class: "btn btn-ghost btn-sm text-neutral-content", "🚀 工作台" }

                    // 人力资源（DaisyUI 原生 dropdown，依赖 tabindex + focus-within）
                    div { class: "dropdown dropdown-end",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content",
                            "人力资源",
                            span { " ▾" }
                        }
                        ul {
                            tabindex: 0,
                            class: "dropdown-content menu bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content",
                            li { class: "menu-title", span { "人力资源" } }
                            li { Link { to: Route::HrAgents {}, "Agent 管理" } }
                            li { Link { to: Route::HrSkills {}, "技能库" } }
                            li { Link { to: Route::HrMemorySearch {}, "记忆搜索" } }
                            li { Link { to: Route::HrKnowledgeGraph {}, "知识图谱" } }
                        }
                    }

                    // 财务管理
                    div { class: "dropdown dropdown-end",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content",
                            "财务管理",
                            span { " ▾" }
                        }
                        ul {
                            tabindex: 0,
                            class: "dropdown-content menu bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content",
                            li { class: "menu-title", span { "财务管理" } }
                            li { Link { to: Route::FinanceModelProviders {}, "模型提供商" } }
                            li { Link { to: Route::FinanceTools {}, "工具管理" } }
                            li { Link { to: Route::FinanceIdentity {}, "身份凭证" } }
                            li { Link { to: Route::FinanceMessageChannels {}, "消息渠道" } }
                            li { Link { to: Route::FinanceAttachments {}, "附件管理" } }
                            li { Link { to: Route::FinanceMcpServers {}, "MCP 服务器" } }
                            li { Link { to: Route::FinanceToolCallEntries {}, "📋 工具调用记录" } }
                        }
                    }

                    // 项目管理
                    div { class: "dropdown dropdown-end",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content",
                            "项目管理",
                            span { " ▾" }
                        }
                        ul {
                            tabindex: 0,
                            class: "dropdown-content menu bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content",
                            li { class: "menu-title", span { "项目管理" } }
                            li { Link { to: Route::ProjectList {}, "项目列表" } }
                            li { Link { to: Route::ProjectArtifacts {}, "项目产物" } }
                        }
                    }

                    // 系统管理
                    div { class: "dropdown dropdown-end",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content",
                            "系统",
                            span { " ▾" }
                        }
                        ul {
                            tabindex: 0,
                            class: "dropdown-content menu bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content",
                            li { class: "menu-title", span { "系统" } }
                            li { Link { to: Route::SystemTriggers {}, "定时触发器" } }
                            li { Link { to: Route::SystemHealth {}, "健康检查" } }
                            li { Link { to: Route::SystemDocs {}, "文档中心" } }
                            if is_admin {
                                li { hr { class: "divider my-0" } }
                                li { Link { to: Route::SystemLogs {}, "日志查询" } }
                                li { Link { to: Route::SystemBackup {}, "备份管理" } }
                                li { Link { to: Route::SystemProcesses {}, "进程管理" } }
                                li { Link { to: Route::SystemAop {}, "AOP 监控" } }
                                li { Link { to: Route::SystemSeed {}, "Seed 配置迁移" } }
                                li { Link { to: Route::SystemTasks {}, "后台任务" } }
                            }
                        }
                    }
                }
            }

            // 右侧：桌面用户菜单 / 移动端汉堡按钮
            div { class: "flex-none",
                if !is_mobile() {
                    // 桌面端用户菜单
                    div { class: "dropdown dropdown-end",
                        div {
                            tabindex: 0,
                            role: "button",
                            class: "btn btn-ghost btn-sm text-neutral-content gap-2",
                            div { class: "avatar",
                                div { class: "w-8 rounded-full {avatar_bg} text-white flex items-center justify-center text-sm font-bold", "{avatar_char}" }
                            }
                            span { "{label}" }
                            span { " ▾" }
                        }
                        ul {
                            tabindex: 0,
                            class: "dropdown-content menu bg-base-100 rounded-box z-[200] w-52 p-2 shadow text-base-content",
                            li { class: "menu-title", span { "账户" } }
                            li { Link { to: Route::UserProfile {}, "👤 个人信息" } }
                            if is_admin {
                                li { Link { to: Route::OrganizationInfo {}, "🏢 组织信息" } }
                                li { Link { to: Route::OrganizationUsers {}, "👥 用户管理" } }
                            }
                            li { hr { class: "divider my-0" } }
                            li { Link { to: Route::Settings {}, "⚙️ 设置" } }
                            li { Link { to: Route::Reception {}, onclick: handle_logout, "🚪 退出登录" } }
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
                                    div { class: "w-12 rounded-full {avatar_bg} text-white flex items-center justify-center text-lg font-bold", "{avatar_char}" }
                                }
                                div { class: "font-semibold text-lg", "{label}" }
                            }
                        }

                        li { class: "menu-title", span { "导航" } }
                        li { Link { to: Route::MessageChat {}, onclick: move |_| drawer_open.set(false), "💬 对话" } }
                        li { Link { to: Route::MessageSearch {}, onclick: move |_| drawer_open.set(false), "🔍 消息搜索" } }
                        li { Link { to: Route::Workspace {}, onclick: move |_| drawer_open.set(false), "🚀 工作台" } }

                        li { class: "menu-title", span { "人力资源" } }
                        li { Link { to: Route::HrAgents {}, onclick: move |_| drawer_open.set(false), "Agent 管理" } }
                        li { Link { to: Route::HrSkills {}, onclick: move |_| drawer_open.set(false), "技能库" } }
                        li { Link { to: Route::HrMemorySearch {}, onclick: move |_| drawer_open.set(false), "记忆搜索" } }
                        li { Link { to: Route::HrKnowledgeGraph {}, onclick: move |_| drawer_open.set(false), "知识图谱" } }

                        li { class: "menu-title", span { "财务管理" } }
                        li { Link { to: Route::FinanceModelProviders {}, onclick: move |_| drawer_open.set(false), "模型提供商" } }
                        li { Link { to: Route::FinanceTools {}, onclick: move |_| drawer_open.set(false), "工具管理" } }
                        li { Link { to: Route::FinanceIdentity {}, onclick: move |_| drawer_open.set(false), "身份凭证" } }
                        li { Link { to: Route::FinanceMessageChannels {}, onclick: move |_| drawer_open.set(false), "消息渠道" } }
                        li { Link { to: Route::FinanceAttachments {}, onclick: move |_| drawer_open.set(false), "附件管理" } }
                        li { Link { to: Route::FinanceMcpServers {}, onclick: move |_| drawer_open.set(false), "MCP 服务器" } }
                        li { Link { to: Route::FinanceToolCallEntries {}, onclick: move |_| drawer_open.set(false), "📋 工具调用记录" } }

                        li { class: "menu-title", span { "项目管理" } }
                        li { Link { to: Route::ProjectList {}, onclick: move |_| drawer_open.set(false), "项目列表" } }
                        li { Link { to: Route::ProjectArtifacts {}, onclick: move |_| drawer_open.set(false), "项目产物" } }

                        li { class: "menu-title", span { "系统" } }
                        li { Link { to: Route::SystemTriggers {}, onclick: move |_| drawer_open.set(false), "定时触发器" } }
                        li { Link { to: Route::SystemHealth {}, onclick: move |_| drawer_open.set(false), "健康检查" } }
                        li { Link { to: Route::SystemDocs {}, onclick: move |_| drawer_open.set(false), "文档中心" } }
                        if is_admin {
                            li { Link { to: Route::SystemLogs {}, onclick: move |_| drawer_open.set(false), "日志查询" } }
                            li { Link { to: Route::SystemBackup {}, onclick: move |_| drawer_open.set(false), "备份管理" } }
                            li { Link { to: Route::SystemProcesses {}, onclick: move |_| drawer_open.set(false), "进程管理" } }
                            li { Link { to: Route::SystemAop {}, onclick: move |_| drawer_open.set(false), "AOP 监控" } }
                            li { Link { to: Route::SystemSeed {}, onclick: move |_| drawer_open.set(false), "Seed 配置迁移" } }
                            li { Link { to: Route::SystemTasks {}, onclick: move |_| drawer_open.set(false), "后台任务" } }
                        }

                        li { hr { class: "divider my-0" } }
                        li { class: "menu-title", span { "账户" } }
                        li { Link { to: Route::UserProfile {}, onclick: move |_| drawer_open.set(false), "👤 个人信息" } }
                        if is_admin {
                            li { Link { to: Route::OrganizationInfo {}, onclick: move |_| drawer_open.set(false), "🏢 组织信息" } }
                            li { Link { to: Route::OrganizationUsers {}, onclick: move |_| drawer_open.set(false), "👥 用户管理" } }
                        }
                        li { Link { to: Route::Settings {}, onclick: move |_| drawer_open.set(false), "⚙️ 设置" } }
                        li { Link { to: Route::Reception {}, onclick: handle_logout, "🚪 退出登录" } }
                    }
                }
            }
        }
    }
}
