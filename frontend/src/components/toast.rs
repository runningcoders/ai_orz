//! Toast 通知组件 - 全局容器 + 单条通知

use dioxus::prelude::*;
use crate::store::toast::{ToastType, use_toast};

/// 全局 Toast 容器（放在根组件中）
#[component]
pub fn ToastContainer() -> Element {
    let toast_state = use_toast();
    let toasts = (toast_state.toasts)();

    rsx! {
        div { class: "toast-container",
            for item in toasts.into_iter() {
                ToastItemView {
                    key: "{item.id}",
                    id: item.id,
                    message: item.message,
                    toast_type: item.toast_type,
                    duration_ms: item.duration_ms,
                }
            }
        }
    }
}

/// 单条 Toast 通知
#[component]
fn ToastItemView(id: u64, message: String, toast_type: ToastType, duration_ms: u64) -> Element {
    let toast_state = use_toast();
    let mut visible = use_signal(|| false);
    let mut leaving = use_signal(|| false);

    // 进入动画延迟（下一帧触发，确保 transition 生效）
    use_effect(move || {
        spawn(async move {
            sleep_ms(10).await;
            visible.set(true);
        });
    });

    // 自动关闭
    use_effect(move || {
        let toast = toast_state;
        spawn(async move {
            sleep_ms(duration_ms).await;
            leaving.set(true);
            sleep_ms(300).await;
            toast.dismiss(id);
        });
    });

    let type_class = match toast_type {
        ToastType::Success => "toast toast-success",
        ToastType::Error => "toast toast-error",
        ToastType::Warning => "toast toast-warning",
        ToastType::Info => "toast toast-info",
    };

    let icon = match toast_type {
        ToastType::Success => "✓",
        ToastType::Error => "✕",
        ToastType::Warning => "!",
        ToastType::Info => "i",
    };

    let animation_class = if leaving() {
        "toast-leaving"
    } else if visible() {
        "toast-visible"
    } else {
        ""
    };

    let handle_close = move |_| {
        leaving.set(true);
        let toast = toast_state;
        spawn(async move {
            sleep_ms(300).await;
            toast.dismiss(id);
        });
    };

    rsx! {
        div { class: "{type_class} {animation_class}",
            span { class: "toast-icon", "{icon}" }
            span { class: "toast-message", "{message}" }
            button { class: "toast-close", onclick: handle_close, "×" }
            div {
                class: "toast-progress",
                style: "animation-duration: {duration_ms}ms;",
            }
        }
    }
}

/// 简易 sleep 实现（基于 js_sys Promise + setTimeout）
async fn sleep_ms(ms: u64) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}
