//! 确认对话框组件 - 用于删除等高危操作的二次确认

use dioxus::prelude::*;

use crate::components::modal::Modal;

/// 确认对话框 Props
#[derive(Props, Clone, PartialEq)]
pub struct ConfirmDialogProps {
    /// 是否显示
    pub show: bool,
    /// 标题
    pub title: String,
    /// 提示信息
    pub message: String,
    /// 确认按钮文案（默认"确定"）
    pub confirm_text: Option<String>,
    /// 取消按钮文案（默认"取消"）
    pub cancel_text: Option<String>,
    /// 确认按钮样式（默认 btn-error）
    pub confirm_class: Option<String>,
    /// 确认回调
    pub on_confirm: EventHandler<()>,
    /// 取消回调
    pub on_cancel: EventHandler<()>,
}

/// 确认对话框组件
///
/// 用法：
/// ```rust,ignore
/// let mut show_delete_confirm = use_signal(|| false);
/// let delete_id = use_signal(|| String::new());
///
/// rsx! {
///     button {
///         onclick: move |_| {
///             delete_id.set(id.clone());
///             show_delete_confirm.set(true);
///         },
///         "删除"
///     }
///     ConfirmDialog {
///         show: show_delete_confirm(),
///         title: "确认删除".to_string(),
///         message: "确定删除此项？此操作不可撤销。".to_string(),
///         on_confirm: move |_| {
///             let id = delete_id();
///             show_delete_confirm.set(false);
///             spawn(async move {
///                 // 执行删除...
///             });
///         },
///         on_cancel: move |_| {
///             show_delete_confirm.set(false);
///         }
///     }
/// }
/// ```
#[component]
pub fn ConfirmDialog(props: ConfirmDialogProps) -> Element {
    let confirm_text = props
        .confirm_text
        .clone()
        .unwrap_or_else(|| "确定".to_string());
    let cancel_text = props
        .cancel_text
        .clone()
        .unwrap_or_else(|| "取消".to_string());
    let confirm_class = props
        .confirm_class
        .clone()
        .unwrap_or_else(|| "btn btn-error".to_string());

    rsx! {
        Modal {
            title: props.title.clone(),
            show: props.show,
            on_close: move |_| props.on_cancel.call(()),
            footer: rsx! {
                button {
                    class: "btn btn-ghost",
                    onclick: move |_| props.on_cancel.call(()),
                    "{cancel_text}"
                }
                button {
                    class: "{confirm_class}",
                    onclick: move |_| props.on_confirm.call(()),
                    "{confirm_text}"
                }
            },
            div { class: "py-4",
                p { class: "text-base-content/80", "{props.message}" }
            }
        }
    }
}
