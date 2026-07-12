//! 模态对话框组件

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ModalProps {
    title: String,
    show: bool,
    on_close: EventHandler<()>,
    children: Element,
    #[props(default = None)]
    footer: Option<Element>,
}

#[component]
pub fn Modal(props: ModalProps) -> Element {
    if !props.show {
        return rsx! {};
    }
    rsx! {
        div {
            class: "modal-overlay",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-content",
                onclick: |e| e.stop_propagation(),
                div {
                    class: "modal-header",
                    h3 { class: "modal-title", "{props.title}" }
                    button {
                        class: "modal-close",
                        onclick: move |_| props.on_close.call(()),
                        "×"
                    }
                }
                {props.children}
                if let Some(footer) = &props.footer {
                    div { class: "modal-footer", {footer.clone()} }
                }
            }
        }
    }
}
