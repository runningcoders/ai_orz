use dioxus::prelude::*;
use dioxus_router::use_navigator;

pub mod use_resource;

use crate::pages::Route;
use crate::store::auth::use_auth_state;

#[allow(unused_imports)]
pub use use_resource::{use_resource, ResourceState};

pub fn use_breakpoint() -> Signal<bool> {
    use_context::<Signal<bool>>()
}

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
