//! 模态对话框组件
//!
//! ## 为什么走原生 `<dialog>.showModal()`，而不是 DaisyUI 的 `modal-open`
//!
//! DaisyUI 的 `.modal` 本质只是 `position: fixed; z-index: 999` 的普通元素。一旦它被渲染在
//! 某个**创建了堆叠上下文**的祖先内部——例如 `.hud-panel` 的 `clip-path`、
//! `.hud-panel-body` 的 `z-index: 1`，或任何 transform / filter / backdrop-filter /
//! opacity<1 / isolation 的祖先——它的 `z-index: 999` 就只在那个上下文内部有意义。
//!
//! 堆叠上下文是**原子的**：子树整体对外只有一个层级，子元素的 z-index 无法越过父上下文
//! 去参与外部比较。于是弹窗会被 navbar（`sticky z-50`）之类的根层级元素盖住，
//! 而且 **把 z-index 调到多大都无解**（加到 9999 也一样）。
//!
//! 原生 `showModal()` 会把 dialog 提升到浏览器的 **top layer**；top layer 独立于文档中
//! 所有堆叠上下文与 clip-path，恒定渲染在最上层，从根本上消除这一整类 bug
//! （全站 33 处 `Modal` + 内嵌 Modal 的 `ConfirmDialog` 一并受益）。
//!
//! ## 生命周期
//! 沿用「show 为 true 才渲染」的条件渲染，与原生 dialog 的开关天然对齐：
//! - `show=true`  → dialog 挂载 → `onmounted` 里调 `showModal()`
//! - `show=false` → dialog 卸载 → 浏览器自动释放 top layer
//!
//! 因此不需要手动 `close()`，也不存在「原生关了但 show 信号没复位」的错位。
//! 唯一会绕过信号的原生关闭路径是 ESC，用 `oncancel` 拦下默认行为后转交 `on_close`。

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlDialogElement;

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
            // 刻意不加 `modal-open`：显隐与层级完全交给原生 [open] + top layer。
            class: "modal",
            onmounted: move |evt: MountedEvent| {
                let data = evt.data();
                if let Some(element) = data.downcast::<web_sys::Element>() {
                    let dlg = element.clone().unchecked_into::<HtmlDialogElement>();
                    // 进入浏览器 top layer，不受任何祖先堆叠上下文 / clip-path 约束
                    let _ = dlg.show_modal();
                }
            },
            // ESC 会原生关闭 dialog 但不复位 show 信号，下一次任意重渲染会把弹窗重新弹开。
            // 这里拦下默认关闭，统一走 on_close。
            oncancel: move |evt| {
                evt.prevent_default();
                props.on_close.call(());
            },
            // 点击遮罩（dialog 自身）关闭；modal-box 内部会 stop_propagation
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-box overflow-x-clip {width_cls}",
                onclick: |e| e.stop_propagation(),
                button {
                    class: "btn hud-btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
                    onclick: move |_| props.on_close.call(()),
                    "✕"
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
