//! 长内容折叠：正文超出可视高度时先展示缩略版，点「展开全文」再看完整内容
//!
//! 目的不是省空间，而是**保护消息流**：一条超长消息不应该把整屏消息顶走。
//! 是否折叠由**实测高度**决定（`scroll_height > client_height`），而不是字符数：
//! 同样 600 字，宽屏两行、窄屏十行，只有后者才需要折叠。
//!
//! 折叠态通过 CSS mask 做「越往下越淡」，底部浮一个展开胶囊。

use dioxus::prelude::*;

/// 短内容测量门槛（字符）：低于它的消息不可能撑破最矮的折叠高度（96px），
/// 直接判定「不溢出」，省掉每条消息一次布局读取。
const SKIP_MEASURE_CHARS: usize = 80;

#[component]
pub fn CollapsibleContent(
    /// 内容长度（字符数）：仅作为「内容变了，重新量一次」的触发信号
    len: usize,
    /// 折叠态的最大高度（px）
    #[props(default = 280)]
    max_height: u32,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let mut expanded = use_signal(|| false);
    let mut overflowing = use_signal(|| false);
    let mut measured = use_signal(|| false);
    let mut node = use_signal(|| Option::<web_sys::Element>::None);

    let mut measure = move || {
        // 展开态不做限制，量出来永远是「不溢出」，会把收起按钮吃掉
        if expanded() {
            return;
        }
        // 短内容不测：既省一次强制布局，也避免首帧把正常消息压成折叠态
        if len <= SKIP_MEASURE_CHARS {
            overflowing.set(false);
            measured.set(true);
            return;
        }
        if let Some(el) = node.peek().clone() {
            overflowing.set(el.scroll_height() > el.client_height() + 4);
            measured.set(true);
        }
    };

    // 内容变化后重新量；首次挂载由 onmounted 负责
    use_effect(use_reactive!(|len| {
        let _ = len;
        measure();
    }));

    // 图片 / Markdown 资源在挂载后才撑开高度：延迟再量一次（卸载即取消）
    use_future(move || {
        let mut measure = measure;
        async move {
            gloo_timers::future::TimeoutFuture::new(250).await;
            measure();
        }
    });

    // 测量完成前也先套上约束：避免长消息先整段铺开再突然收起的一帧跳动；
    // 短内容本来就不会溢出，不套约束（也省掉首帧的一次裁剪）
    let clamp = !expanded() && len > SKIP_MEASURE_CHARS && (overflowing() || !measured());
    // ⚠️ 展开时不能把 class / style 置空：空值不会可靠地清掉上一次的
    // `max-height` 与 mask，表现就是「按钮已经变成收起，但正文仍被裁住」。
    // 因此两个分支都给非空值，靠 `none` / `visible` 显式覆盖。
    let body_class = if clamp {
        "collapsible-fade"
    } else {
        "collapsible-expanded"
    };
    let body_style = if clamp {
        format!("max-height: {max_height}px; overflow: hidden;")
    } else {
        "max-height: none; overflow: visible;".to_string()
    };
    // 展开 / 收起共用同一枚胶囊，保证位置与视觉对称，只是箭头方向相反
    let pill_class = "rounded-full border border-base-content/15 bg-base-100/90 px-2.5 py-0.5 text-[11px] text-base-content/65 shadow-sm backdrop-blur transition-colors hover:border-primary/50 hover:text-primary";

    rsx! {
        div { class: "relative {class}",
            div {
                class: "{body_class}",
                style: "{body_style}",
                onmounted: move |evt: MountedEvent| {
                    if let Some(el) = evt.data().downcast::<web_sys::Element>() {
                        node.set(Some(el.clone()));
                        measure();
                    }
                },
                {children}
            }
            if overflowing() && !expanded() {
                // 不铺整宽渐变：气泡底色不固定（primary/neutral/system），
                // 用一枚半透明胶囊保证在任意底色上都可读。
                div { class: "pointer-events-none absolute inset-x-0 bottom-0 flex justify-center pb-1",
                    button {
                        class: "pointer-events-auto {pill_class}",
                        onclick: move |_| expanded.set(true),
                        "点击展开 ↓"
                    }
                }
            }
            if overflowing() && expanded() {
                // 与展开态同一位置：底部居中；箭头向上表示收起
                div { class: "mt-1 flex justify-center",
                    button {
                        class: "{pill_class}",
                        onclick: move |_| expanded.set(false),
                        "点击收起 ↑"
                    }
                }
            }
        }
    }
}
