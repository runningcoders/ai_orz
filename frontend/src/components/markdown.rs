//! 通用 Markdown 渲染组件
//!
//! 使用 pulldown-cmark 将 Markdown 渲染为 HTML 后通过 `dangerous_inner_html` 注入。
//! pulldown-cmark 默认会把源文原始 HTML 透传为 Html 事件，这里通过事件映射
//! 将其全部转义为文本，保证注入是 XSS 安全的。
//!
//! 样式复用 `input.css` 中的 `.markdown-body`（引用 DaisyUI 主题变量，30+ 主题自适应）。
//!
//! Mermaid 注入式渲染：```mermaid 代码块经 pulldown-cmark 输出为
//! `pre code.language-mermaid`，组件挂载后调用 index.html 暴露的
//! `window.__renderMermaid(container)` 将其替换为 SVG。
//!
//! 用法：
//! ```rust,ignore
//! use crate::components::markdown::MarkdownRenderer;
//!
//! rsx! {
//!     MarkdownRenderer { content: plan.clone() }
//!     MarkdownRenderer { content: summary.clone(), compact: true }
//! }
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dioxus::prelude::*;

/// 容器 ID 自增序号（同页多实例互不干扰）
static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn next_container_id(prefix: &str) -> String {
    format!("{}-{}", prefix, ID_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Markdown → HTML（启用表格 / 删除线 / 任务列表扩展语法）
///
/// 源文原始 HTML（块级/内联）会被转义为纯文本，禁止透传，保证注入安全。
/// 与文档中心 docs.rs 的渲染逻辑一致，供非组件场景（如预渲染）复用。
pub fn render_markdown(md: &str) -> String {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_TABLES);
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    let parser = pulldown_cmark::Parser::new_ext(md, options);
    // 原始 HTML 事件降级为纯文本（push_html 会自动转义），避免 dangerous_inner_html 注入 XSS
    let escaped = parser.map(|event| match event {
        pulldown_cmark::Event::Html(raw) | pulldown_cmark::Event::InlineHtml(raw) => {
            pulldown_cmark::Event::Text(raw)
        }
        other => other,
    });
    let mut html_out = String::new();
    pulldown_cmark::html::push_html(&mut html_out, escaped);
    html_out
}

/// Markdown 渲染组件
///
/// - `content`: Markdown 源文本
/// - `compact`: 紧凑模式（更小字号 / 收紧上下边距），用于卡片、聊天气泡、列表展开等场景
#[component]
pub fn MarkdownRenderer(
    content: String,
    #[props(default = false)] compact: bool,
) -> Element {
    // 按 content 缓存 HTML，避免聊天等长列表场景每帧重复解析
    let html = use_memo(move || render_markdown(&content));
    let container_id = use_hook(|| next_container_id("md"));
    // 含 ```mermaid 代码块时，挂载后调用 JS 渲染层替换为 SVG
    {
        let id = container_id.clone();
        use_effect(move || {
            if html().contains("language-mermaid") {
                schedule_mermaid_scan(id.clone());
            }
        });
    }
    let class = if compact {
        "markdown-body markdown-compact max-w-none"
    } else {
        "markdown-body max-w-none"
    };
    rsx! {
        div {
            id: "{container_id}",
            class: "{class}",
            dangerous_inner_html: "{html}"
        }
    }
}

/// 独立 Mermaid 图组件（渲染裸 Mermaid 字符串，如 Project.task_graph）
///
/// 依赖 index.html 暴露的 `window.__renderMermaidCode(container, code)`。
#[component]
pub fn MermaidDiagram(code: String) -> Element {
    let container_id = use_hook(|| next_container_id("mermaid"));
    {
        let id = container_id.clone();
        let code = code.clone();
        use_effect(move || {
            let id = id.clone();
            let code = code.clone();
            spawn(async move {
                render_mermaid_code_now(&id, &code);
            });
        });
    }
    rsx! {
        div {
            id: "{container_id}",
            class: "mermaid-diagram overflow-x-auto",
        }
    }
}

/// 延迟扫描容器内的 ```mermaid 代码块（等待 DOM 挂载完成）
fn schedule_mermaid_scan(element_id: String) {
    spawn(async move {
        gloo_timers::future::sleep(Duration::from_millis(30)).await;
        render_mermaid_blocks_now(&element_id);
    });
}

/// 调用 window.__renderMermaid(container)：替换容器内 language-mermaid 代码块为 SVG
fn render_mermaid_blocks_now(element_id: &str) {
    let Some(el) = get_element_by_id(element_id) else {
        return;
    };
    call_window_fn("__renderMermaid", |window, func| {
        let _ = func.call1(window, &el);
    });
}

/// 调用 window.__renderMermaidCode(container, code)：将裸 Mermaid 字符串渲染进容器
fn render_mermaid_code_now(element_id: &str, code: &str) {
    let Some(el) = get_element_by_id(element_id) else {
        return;
    };
    let code_val = wasm_bindgen::JsValue::from_str(code);
    call_window_fn("__renderMermaidCode", |window, func| {
        let _ = func.call2(window, &el, &code_val);
    });
}

fn get_element_by_id(element_id: &str) -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|doc| doc.get_element_by_id(element_id))
}

/// 读取 window 上的全局函数并执行（函数不存在时静默跳过，如 vendor 文件缺失）
fn call_window_fn(name: &str, f: impl FnOnce(&web_sys::Window, js_sys::Function)) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(func_val) = js_sys::Reflect::get(&window, &wasm_bindgen::JsValue::from_str(name))
    else {
        return;
    };
    if !func_val.is_function() {
        return;
    }
    f(&window, js_sys::Function::from(func_val));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_heading_and_list() {
        let html = render_markdown("# 标题\n- 一项\n- 两项");
        assert!(html.contains("<h1>标题</h1>"));
        assert!(html.contains("<li>一项</li>"));
        assert!(html.contains("<li>两项</li>"));
    }

    #[test]
    fn render_table_and_strikethrough() {
        let html = render_markdown("| a | b |\n|---|---|\n| 1 | 2 |\n\n~~删除~~");
        assert!(html.contains("<table>"));
        assert!(html.contains("<del>删除</del>"));
    }

    #[test]
    fn render_tasklist() {
        let html = render_markdown("- [ ] 未完成\n- [x] 已完成");
        assert!(html.contains("checkbox"));
    }

    #[test]
    fn render_escapes_raw_html() {
        // 原始 HTML（块级 + 内联）一律转义为文本，保证 dangerous_inner_html 注入安全
        let html = render_markdown("<script>alert(1)</script>\n\nhello <b>x</b>");
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<b>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;b&gt;"));
    }

    #[test]
    fn render_mermaid_block_kept_as_code() {
        // ```mermaid 代码块保留为带语言 class 的代码，交由 JS 渲染层处理
        let html = render_markdown("```mermaid\ngraph LR\nA-->B\n```");
        assert!(html.contains("language-mermaid"));
        assert!(html.contains("graph LR"));
    }
}
