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
    /// 自定义宽度 class，如 "max-w-3xl" / "max-w-5xl" / "max-w-[90rem]" 等；
    /// 不传则使用 DaisyUI 默认 max-w-[90rem]。
    #[props(default = None)]
    width_class: Option<String>,
}

#[component]
pub fn Modal(props: ModalProps) -> Element {
    if !props.show {
        return rsx! {};
    }
    let width_cls = props.width_class.clone().unwrap_or_default();
    rsx! {
        dialog {
            class: "modal modal-open",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-box overflow-x-clip {width_cls}",
                onclick: |e| e.stop_propagation(),
                form {
                    method: "dialog",
                    button {
                        class: "btn hud-btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
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
