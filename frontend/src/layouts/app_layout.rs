//! 应用主布局 - Navbar + Content

use dioxus::prelude::*;

use super::navbar::Navbar;

#[component]
pub fn AppLayout(children: Element) -> Element {
    rsx! {
        div { class: "app-container",
            Navbar {}
            main { class: "content-area",
                {children}
            }
        }
    }
}
