mod components;
mod api;

use dioxus::prelude::*;
use components::{HealthCheck, Navbar};

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        div {
            style: "
                min-height: 100vh;
                background-color: #f5f5f5;
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            ",
            // 顶部导航栏
            Navbar {}

            // 主内容区
            main {
                style: "
                    max-width: 800px;
                    margin: 2rem auto;
                    padding: 0 1rem;
                ",
                h1 {
                    style: "color: #2c3e50; margin-bottom: 2rem; text-align: center;",
                    "AI Orz"
                }
                p {
                    style: "text-align: center; color: #666; margin-bottom: 2rem;",
                    "AI 代理执行框架 - 智能体组织化协作平台"
                }

                HealthCheck {}

                footer {
                    style: "text-align: center; color: #999; margin-top: 3rem; font-size: 0.9rem;",
                    "Built with Dioxus + Rust"
                }
            }
        }
    }
}
