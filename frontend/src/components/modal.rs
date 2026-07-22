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
        dialog {
            class: "modal modal-open",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-box",
                onclick: |e| e.stop_propagation(),
                form {
                    method: "dialog",
                    button {
                        class: "btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
                        onclick: move |_| props.on_close.call(()),
                        "✕"
                    }
                }
                h3 { class: "font-bold text-lg mb-4", "{props.title}" }
                {props.children}
                if let Some(footer) = &props.footer {
                    div { class: "modal-action", {footer.clone()} }
                }
            }
        }
    }
}
