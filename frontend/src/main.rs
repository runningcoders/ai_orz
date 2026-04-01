mod components;
mod api;

use dioxus::prelude::*;
use components::HealthCheck;

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
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

            HealthCheck {}

            footer {
                style: "text-align: center; color: #999; margin-top: 3rem; font-size: 0.9rem;",
                "Built with Dioxus + Rust"
            }
        }
    }
}
