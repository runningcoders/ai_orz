use dioxus::prelude::*;
use dioxus_router::use_navigator;

pub mod use_resource;

use crate::pages::Route;
use crate::store::auth::use_auth_state;
use crate::utils::local_storage;

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

pub const AVAILABLE_THEMES: &[(&str, &str)] = &[
    ("orz-light", "Orz 默认"),
    ("light", "Light"),
    ("dark", "Dark"),
    ("cupcake", "Cupcake"),
    ("bumblebee", "Bumblebee"),
    ("emerald", "Emerald"),
    ("corporate", "Corporate"),
    ("synthwave", "Synthwave"),
    ("retro", "Retro"),
    ("cyberpunk", "Cyberpunk"),
    ("valentine", "Valentine"),
    ("halloween", "Halloween"),
    ("garden", "Garden"),
    ("forest", "Forest"),
    ("aqua", "Aqua"),
    ("lofi", "Lofi"),
    ("pastel", "Pastel"),
    ("fantasy", "Fantasy"),
    ("luxury", "Luxury"),
    ("dracula", "Dracula"),
    ("autumn", "Autumn"),
    ("business", "Business"),
    ("night", "Night"),
    ("coffee", "Coffee"),
    ("winter", "Winter"),
    ("dim", "Dim"),
    ("nord", "Nord"),
    ("sunset", "Sunset"),
];

fn get_saved_theme() -> String {
    local_storage()
        .and_then(|s| s.get_item("ai_orz_theme").ok().flatten())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "orz-light".to_string())
}

fn set_html_theme(theme: &str) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(html) = doc.document_element() {
            let _ = html.set_attribute("data-theme", theme);
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThemeController {
    theme: Signal<String>,
}

impl ThemeController {
    pub fn current(&self) -> String {
        (self.theme)()
    }

    pub fn set(&mut self, new_theme: String) {
        if let Some(storage) = local_storage() {
            let _ = storage.set_item("ai_orz_theme", &new_theme);
        }
        set_html_theme(&new_theme);
        self.theme.set(new_theme);
    }
}

pub fn use_theme() -> ThemeController {
    let theme = use_signal(|| get_saved_theme());

    use_effect(move || {
        set_html_theme(&(theme)());
    });

    ThemeController { theme }
}
