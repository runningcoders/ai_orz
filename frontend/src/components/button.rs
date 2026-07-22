//! 按钮组件

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ButtonVariant {
    Primary,
    Accent,
    Secondary,
    Danger,
    Ghost,
}

#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    #[props(default = ButtonVariant::Primary)]
    variant: ButtonVariant,
    #[props(default = false)]
    disabled: bool,
    #[props(default = false)]
    small: bool,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
}

#[component]
pub fn Button(props: ButtonProps) -> Element {
    let variant_class = match props.variant {
        ButtonVariant::Primary => "btn btn-primary",
        ButtonVariant::Accent => "btn btn-secondary",
        ButtonVariant::Secondary => "btn btn-outline",
        ButtonVariant::Danger => "btn btn-error",
        ButtonVariant::Ghost => "btn btn-ghost",
    };
    let size_class = if props.small { "btn-sm" } else { "" };
    rsx! {
        button {
            class: "{variant_class} {size_class}",
            disabled: props.disabled,
            onclick: move |e| {
                if let Some(handler) = &props.onclick {
                    handler.call(e);
                }
            },
            {props.children}
        }
    }
}
