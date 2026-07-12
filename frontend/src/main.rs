mod api;
mod components;
mod config;
mod layouts;
mod pages;
mod store;

// Include compile-time generated configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/compiled_config.rs"));

use dioxus::prelude::*;

#[allow(unused_imports)]
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
