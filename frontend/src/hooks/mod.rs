//! 响应式 Hook

use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::MediaQueryListEvent;

/// 返回当前是否为移动端（≤ 768px）的信号。
///
/// 基于 `window.matchMedia("(max-width: 768px)")` 监听，窗口尺寸变化时自动更新。
/// 通过 `use_context_provider` 在根组件注入，全局共享同一信号与监听器。
pub fn use_breakpoint() -> Signal<bool> {
    use_context_provider(|| {
        let mut is_mobile = use_signal(|| false);
        use_effect(move || {
            let Some(window) = web_sys::window() else {
                return;
            };
            let Ok(Some(mql)) = window.match_media("(max-width: 768px)") else {
                return;
            };
            is_mobile.set(mql.matches());
            let cb = Closure::new(move |e: MediaQueryListEvent| {
                is_mobile.set(e.matches());
            });
            let _ = mql.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref());
            // 监听器随页面生命周期保活；Closure 不回收以避免监听器失效
            std::mem::forget(cb);
        });
        is_mobile
    })
}
