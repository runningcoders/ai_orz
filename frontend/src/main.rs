mod api;
mod components;
mod config;
mod layouts;
mod pages;
mod store;

// Include compile-time generated configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/compiled_config.rs"));

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use store::auth::{save_token, AuthState};

use crate::pages::Route;

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    // 初始化全局认证状态
    use_context_provider(|| Signal::new(AuthState::restore()));

    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        Router::<Route> {}
    }
}

// ===== 路由组件渲染入口 =====
// Dioxus Router 会根据 Route 枚举自动调用对应的组件函数

// 前台接待
#[component]
fn Reception() -> Element {
    crate::pages::reception::Reception()
}

// 组织模块
#[component]
fn OrganizationInfo() -> Element {
    crate::pages::organization::info::OrganizationInfo()
}

#[component]
fn OrganizationUsers() -> Element {
    crate::pages::organization::users::OrganizationUsers()
}

// HR 模块
#[component]
fn HrAgents() -> Element {
    crate::pages::hr::agents::HrAgents()
}

#[component]
fn HrAgentDetail(id: String) -> Element {
    crate::pages::hr::agent_detail::HrAgentDetail { id }
}

#[component]
fn HrSkills() -> Element {
    crate::pages::hr::skills::HrSkills()
}

// Finance 模块
#[component]
fn FinanceModelProviders() -> Element {
    crate::pages::finance::model_providers::FinanceModelProviders()
}

#[component]
fn FinanceTools() -> Element {
    crate::pages::finance::tools::FinanceTools()
}

#[component]
fn FinanceMessageChannels() -> Element {
    crate::pages::finance::message_channels::FinanceMessageChannels()
}

// Project 模块
#[component]
fn ProjectList() -> Element {
    crate::pages::project::projects::ProjectList()
}

#[component]
fn ProjectDetail(id: String) -> Element {
    crate::pages::project::project_detail::ProjectDetail { id }
}

// Message 模块
#[component]
fn MessageChat() -> Element {
    crate::pages::message::chat::MessageChat()
}

// System 模块
#[component]
fn SystemTriggers() -> Element {
    crate::pages::system::triggers::SystemTriggers()
}

#[component]
fn SystemHealth() -> Element {
    crate::pages::system::health::SystemHealth()
}

// 用户
#[component]
fn UserProfile() -> Element {
    crate::pages::user::profile::UserProfile()
}

// 设置
#[component]
fn Settings() -> Element {
    crate::pages::settings::Settings()
}
