//! 应用主布局 - Navbar + Content + 权限守卫

use dioxus::prelude::*;

use super::navbar::Navbar;
use crate::hooks::use_require_auth;

#[component]
pub fn AppLayout(children: Element) -> Element {
    // 权限检查：未登录时重定向到登录页
    if !use_require_auth() {
        return rsx! {
            div { class: "min-h-screen bg-base-100 flex items-center justify-center",
                span { class: "loading loading-spinner loading-lg text-primary" }
            }
        };
    }

    rsx! {
        div { class: "min-h-screen bg-base-100 flex flex-col",
            Navbar {}
            main { class: "flex-1 container mx-auto px-4 py-6 max-w-7xl",
                {children}
            }
        }
    }
}
