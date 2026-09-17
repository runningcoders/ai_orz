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
    /// 原生 `title` 悬停提示。图标/符号按钮至少要给一个，否则用户猜不到它做什么。
    #[props(default)]
    title: Option<String>,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
}

#[component]
pub fn Button(props: ButtonProps) -> Element {
    let variant_class = match props.variant {
        ButtonVariant::Primary => "btn hud-btn btn-primary",
        ButtonVariant::Accent => "btn hud-btn btn-accent",
        ButtonVariant::Secondary => "btn hud-btn btn-secondary",
        ButtonVariant::Danger => "btn hud-btn btn-error",
        ButtonVariant::Ghost => "btn hud-btn btn-ghost",
    };
    let size_class = if props.small { "btn-sm" } else { "" };
    rsx! {
        button {
            class: "{variant_class} {size_class}",
            disabled: props.disabled,
            title: props.title.clone().unwrap_or_default(),
            onclick: move |e| {
                if let Some(handler) = &props.onclick {
                    handler.call(e);
                }
            },
            {props.children}
        }
    }
}
