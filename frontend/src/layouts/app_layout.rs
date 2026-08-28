//! 应用主布局 - Navbar + Content + 权限守卫

use dioxus::prelude::*;
use dioxus_router::use_route;

use super::navbar::Navbar;
use crate::hooks::use_require_auth;
use crate::pages::Route;

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

    // Workspace 工作台走 HUD 全屏模式：main 去掉容器/内边距/最大宽度，由页面自身绝对定位铺满
    let is_workspace = matches!(use_route::<Route>(), Route::Workspace {});

    rsx! {
        div { class: "min-h-screen bg-base-100 flex flex-col",
            Navbar {}
            main { class: if is_workspace { "flex-1 relative overflow-hidden" } else { "flex-1 container mx-auto px-4 py-6 max-w-7xl" },
                {children}
            }
        }
    }
}
