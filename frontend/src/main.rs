mod components;
mod api;

use dioxus::prelude::*;
use components::{Navbar, Reception};

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

            // 主内容区 - 默认前台接待页面
            main {
                style: "
                    padding: 2rem 1rem;
                ",
                Reception {}
            }
        }
    }
}
