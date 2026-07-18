use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::pages::Route;
use crate::store::auth::use_auth_state;

pub fn use_breakpoint() -> Signal<bool> {
    use_context::<Signal<bool>>()
}

/// 权限守卫：未登录时返回 false 并重定向到登录页
/// 在需要权限的页面开头调用，如果返回 false 则提前 return
pub fn use_require_auth() -> bool {
    let auth = use_auth_state();
    let navigator = use_navigator();

    use_effect(move || {
        if !auth.read().logged_in {
            navigator.replace(Route::Reception {});
        }
    });

    auth.read().logged_in
}