mod components;
mod api;

// Include compile-time generated configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/compiled_config.rs"));

use dioxus::prelude::*;
use components::{Navbar, Reception, AgentManagement, ModelProviderManagement, UserProfile, OrganizationInfo, UserManagement};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Reception,
    AgentManagement,
    ModelProviderManagement,
    UserProfile,
    OrganizationInfo,
    UserManagement,
}

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    let mut current_page = use_signal(|| Page::Reception);

    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        div {
            style: "
                min-height: 100vh;
                background-color: #f5f5f5;
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            ",
            // 顶部导航栏 - 需要传递页面切换回调
            Navbar {
                on_navigate: move |page| current_page.set(page)
            }

            // 主内容区 - 根据当前页面渲染
            main {
                style: "
                    padding: 2rem 1rem;
                ",
                match current_page() {
                    Page::Reception => rsx! { Reception {} },
                    Page::AgentManagement => rsx! { AgentManagement {} },
                    Page::ModelProviderManagement => rsx! { ModelProviderManagement {} },
                    Page::UserProfile => rsx! { UserProfile {} },
                    Page::OrganizationInfo => rsx! { OrganizationInfo {} },
                    Page::UserManagement => rsx! { UserManagement {} },
                }
            }
        }
    }
}
