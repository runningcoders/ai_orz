use dioxus::prelude::*;

#[component]
pub fn Navbar() -> Element {
    rsx! {
        nav {
            style: "
                background-color: #2c3e50;
                padding: 1rem 2rem;
                display: flex;
                align-items: center;
                justify-content: space-between;
                box-shadow: 0 2px 4px rgba(0,0,0,0.1);
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
                        &:hover {{
                            background-color: rgba(255,255,255,0.1);
                        }}
                    ",
                    href: "#",
                    "前台接待"
                }
                a {
                    style: "
                        color: #ecf0f1;
                        text-decoration: none;
                        padding: 0.5rem 1rem;
                        border-radius: 4px;
                        transition: background-color 0.2s;
                        &:hover {{
                            background-color: rgba(255,255,255,0.1);
                        }}
                    ",
                    href: "#",
                    "人力资源"
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
