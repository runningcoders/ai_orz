//! 应用主布局 - Navbar + Content + 权限守卫

use dioxus::prelude::*;

use super::navbar::Navbar;
use crate::hooks::use_require_auth;

#[component]
pub fn AppLayout(children: Element) -> Element {
    // 权限检查：未登录时重定向到登录页
    if !use_require_auth() {
        return rsx! {
            div { class: "loading-screen",
                div { class: "loading-spinner" }
            }
        };
    }

    rsx! {
        div { class: "app-container",
            Navbar {}
            main { class: "content-area",
                {children}
            }
        }
    }
}
