//! 按钮组件

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Accent,
    Secondary,
    Danger,
    Ghost,
}

impl ButtonVariant {
    fn class(self) -> &'static str {
        match self {
            ButtonVariant::Primary => "btn btn-primary",
            ButtonVariant::Accent => "btn btn-accent",
            ButtonVariant::Secondary => "btn btn-secondary",
            ButtonVariant::Danger => "btn btn-danger",
            ButtonVariant::Ghost => "btn btn-ghost",
        }
    }
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
    let mut class = props.variant.class().to_string();
    if props.small {
        class.push_str(" btn-sm");
    }
    rsx! {
        button {
            class: "{class}",
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
