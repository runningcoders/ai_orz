mod api;
mod components;
mod config;
mod hooks;
mod layouts;
mod pages;
mod store;
mod utils;

// Include compile-time generated configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/compiled_config.rs"));

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

#[allow(unused_imports)]
use store::auth::AuthState;
use store::toast::use_provide_toast;

use crate::components::toast::ToastContainer;
use crate::hooks::use_provide_theme;
use crate::pages::Route;

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(|| Signal::new(AuthState::restore()));
    let _toast = use_provide_toast();
    let _theme_ctrl = use_provide_theme();

    // 初始化全局断点信号（移动端检测）
    let mut is_mobile = use_signal(|| false);
    use_effect(move || {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(Some(mql)) = window.match_media("(max-width: 768px)") else {
            return;
        };
        is_mobile.set(mql.matches());
        let cb = wasm_bindgen::closure::Closure::new(move |e: web_sys::MediaQueryListEvent| {
            is_mobile.set(e.matches());
        });
        let _ = mql.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref());
        std::mem::forget(cb);
    });
    use_context_provider(|| is_mobile);

    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        Router::<Route> {}
        ToastContainer {}
    }
}