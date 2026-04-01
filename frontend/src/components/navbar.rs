use dioxus::prelude::*;
use crate::Page;

#[component]
pub fn Navbar(on_navigate: EventHandler<Page>) -> Element {
    let mut hr_menu_open = use_signal(|| false);

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
                                    // TODO: 员工管理页面
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
            }

            // 右侧：健康状态指示器
            div {
                style: "display: flex; align-items: center;",
                crate::components::HealthCheck {}
            }
        }
    }
}
