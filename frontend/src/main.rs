mod api;
mod components;
mod config;
mod hooks;
mod layouts;
mod pages;
mod store;

// Include compile-time generated configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/compiled_config.rs"));

use dioxus::prelude::*;

#[allow(unused_imports)]
use store::auth::AuthState;
use store::toast::use_provide_toast;

use crate::components::toast::ToastContainer;
use crate::pages::Route;

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    // 初始化全局认证状态
    use_context_provider(|| Signal::new(AuthState::restore()));
    // 初始化全局 Toast 状态
    let _toast = use_provide_toast();

    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        Router::<Route> {}
        ToastContainer {}
    }
}
