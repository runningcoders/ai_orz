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
//! @ 提及渲染：`[@名](agent:id)` 形态的链接（合法 CommonMark，未识别时降级为普通链接）
//! 在事件流里被替换为 chip，详见 `utils::mention`。
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

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;

use crate::store::directory::Directory;
use crate::utils::mention::transform_mentions;
use crate::utils::message::NameMap;

/// 容器 ID 自增序号（同页多实例互不干扰）
static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn next_container_id(prefix: &str) -> String {
    format!("{}-{}", prefix, ID_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Markdown → HTML（启用表格 / 删除线 / 任务列表扩展语法）
///
/// 源文原始 HTML（块级/内联）会被转义为纯文本，禁止透传，保证注入安全。
/// 本函数是全站唯一渲染事实源，文档中心（docs.rs）等非组件场景直接复用。
///
/// 不带名称上下文：提及 chip 用消息文本里的快照名，见 [`render_markdown_with_names`]。
pub fn render_markdown(md: &str) -> String {
    render_markdown_with_names(md, None)
}

/// Markdown → HTML，带 Agent 名称上下文
///
/// `agents` 用于把 Agent 提及的展示名升级为目录里的实时名 —— 实体改名后，
/// 历史消息里的 chip 会自动跟着变，不会停留在发送当时的快照。
/// 传 `None`（文档中心等非聊天场景）时回退到消息文本里的快照名。
///
/// 事件流统一交给 [`transform_mentions`]：它同时承担源文 HTML 转义
/// 与 `[@名](agent:id)` 提及链接 → chip 的替换，调用方无需再单独做转义映射。
pub fn render_markdown_with_names(md: &str, agents: Option<&NameMap>) -> String {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_TABLES);
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    let parser = pulldown_cmark::Parser::new_ext(md, options);
    let events = transform_mentions(parser, agents);
    let mut html_out = String::new();
    pulldown_cmark::html::push_html(&mut html_out, events.into_iter());
    // 站内链接预拼 data-repo-href（index.html 全局点击拦截 JS 桥用，AGENTS §2.1.2）
    crate::utils::doc_link::post_process_doc_links(&html_out, crate::utils::doc_link::BLOB_BASE)
}

/// `MarkdownRenderer` 每实例的渲染缓存（内容寻址）
///
/// 失效键 = `content` + 目录快照（`Option<NameMap>`，仅 `agents` 参与 HTML 计算）：
/// 任一变化即全量重解析；都未变时直接复用缓存 HTML —— chat / 长列表里父组件
/// 的重渲染只剩一次字符串对比，不再触发 O(n) 重解析。
#[derive(Default)]
struct MdCache {
    /// 上次解析的 Markdown 源文
    content: String,
    /// 上次解析使用的目录快照（None = 无目录上下文）。仅目录失配重算时克隆一次，命中路径零克隆
    names: Option<NameMap>,
    /// 上次解析产物
    html: String,
}

/// Markdown 渲染组件
///
/// - `content`: Markdown 源文本
/// - `compact`: 紧凑模式（更小字号 / 收紧上下边距），用于卡片、聊天气泡、列表展开等场景
///
/// 内容寻缓存：`content` / 目录任一变化必重算（修复知识图谱详情「内容」栏切换不更新），
/// 都未变化则直接复用缓存（chat / 长列表高频父渲染下不丢性能）。
#[component]
pub fn MarkdownRenderer(content: String, #[props(default = false)] compact: bool) -> Element {
    // 全局名称目录：命中时 Agent 提及用实时名渲染。
    // 文档中心等未提供 provider 的页面 try_consume_context 返回 None，自动回退到快照名。
    let directory = try_consume_context::<Signal<Directory>>();
    // 内容寻缓存兼顾两个目标：
    // 1) content 变化必重算（修复：知识图谱详情「内容」栏切换不更新）——
    //    病灶是 use_memo 闭包只捕获普通 String prop、零 signal 订阅永不重算，
    //    组件实例被位置 diff 复用时「内容」恒显示首个渲染节点的固化值；
    //    现以 content 为失效键在渲染期对比，变化必重算，语义等价于此前
    //    「每渲染直接计算」的修复。
    // 2) content 未变化不重解析 —— 父组件任何重渲染只付 O(len) 字符串对比
    //    + 一次 String 克隆，不再 O(n) 解析。
    //
    // use_hook 每次渲染返回存储值的 clone（State: Clone），裸 RefCell 会被克隆走、
    // 写入不落回组件状态，故包一层 Rc（clone 只拷指针，cell 本体跨渲染共享）。
    let cache = use_hook(|| Rc::new(RefCell::new(MdCache::default())));
    let html = {
        let mut c = cache.borrow_mut();
        // 目录指纹取舍：Directory 非 Arc 包装、HashMap 也没有廉价的稳定指针指纹，
        // 故缓存里存快照用 PartialEq 精确对比（组织内 ≤ 500 条纯字符串比较，
        // 远低于一次 markdown 解析）；比弱指纹（len+首键，会漏掉中间实体改名）可靠。
        // Some 分支每次渲染无条件 read()：维持对目录 signal 的订阅，Agent 名预载
        // 完成后已在屏的历史气泡 chip 自动失效重算为实时名。
        let names_match = match &directory {
            Some(sig) => {
                let d = sig.read();
                c.names.as_ref() == Some(&d.agents)
            }
            None => c.names.is_none(),
        };
        if c.content != content || !names_match {
            // 目录快照仅在失配时重新克隆：流式场景 content 每 token 失效一次，
            // 目录未变时不应付出整表克隆的代价。
            if !names_match {
                c.names = directory.as_ref().map(|sig| sig.read().agents.clone());
            }
            c.html = match directory.as_ref() {
                Some(sig) => {
                    let d = sig.read();
                    render_markdown_with_names(&content, Some(&d.agents))
                }
                None => render_markdown(&content),
            };
            c.content = content.clone();
        }
        c.html.clone()
    };
    // use_effect 只订阅 signal、无法跟踪普通值：渲染期把最新 html 同步进 signal，
    // 供下方 mermaid 效果在内容变化后重新触发。signal 初值即首帧计算结果
    // （use_signal 初始化闭包），消除「空 → 首值」多出的一次重渲染；
    // 后续帧渲染期仍无条件 set 同步，值相等时由 vdom diff 保证无 DOM 变更。
    let mut html_signal = use_signal(|| html.clone());
    html_signal.set(html.clone());
    let container_id = use_hook(|| next_container_id("md"));
    // 含 ```mermaid 代码块时，挂载后（use_effect 已在 DOM 挂载后运行）直接调用 JS 渲染层替换为 SVG。
    // 不再用 spawn + 30ms sleep 延迟：该 future 会在所属组件卸载时被 drop，
    // 而 TimeoutFuture 底层 Closure::once 在「已入队未触发」时卸载会触发
    // "closure invoked recursively or after being dropped"（工作台 SSE 流式气泡高频复现）。
    {
        let id = container_id.clone();
        use_effect(move || {
            if html_signal.read().contains("language-mermaid") {
                render_mermaid_blocks_now(&id);
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
    let Ok(func_val) = js_sys::Reflect::get(&window, &wasm_bindgen::JsValue::from_str(name)) else {
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

    // 问题 3 回归守卫：MarkdownRenderer 禁止回到 use_memo 固化实现，也禁止丢掉内容寻缓存。
    // 病灶：use_memo 闭包只捕获普通 String prop、零 signal 订阅 → Memo 永不重算，
    // 位置 diff 复用组件实例时「内容」栏恒显示首个渲染节点的固化值。
    // 结构断言（include_str! + 按 #[cfg(test)] 切分代码区防自匹配）：
    // 组件区必须保留「内容寻缓存失效键 + signal 初值即首帧结果 + 渲染期同步 signal +
    // 效果订阅 signal」锚点。
    #[test]
    fn markdown_renderer_content_switch_fix_anchors_present() {
        let src = include_str!("markdown.rs");
        let code = src.split("#[cfg(test)]").next().expect("code region");
        assert!(
            !code.contains("use_memo("),
            "组件区禁止 use_memo（固化病灶回退即失败）"
        );
        // 内容寻缓存：缓存容器 + 失效键对比 + 命中复用
        assert_eq!(
            code.matches("use_hook(|| Rc::new(RefCell::new(MdCache::default())))")
                .count(),
            1
        );
        assert_eq!(
            code.matches("if c.content != content || !names_match {")
                .count(),
            1
        );
        assert_eq!(code.matches("c.html.clone()").count(), 1);
        // signal 初值即首帧计算结果（无「空 → 首值」多渲染），渲染期维持同步
        assert_eq!(code.matches("use_signal(|| html.clone())").count(), 1);
        assert_eq!(code.matches("html_signal.set(html.clone());").count(), 1);
        assert_eq!(
            code.matches("html_signal.read().contains(\"language-mermaid\")")
                .count(),
            1
        );
        assert_eq!(code.matches("dangerous_inner_html: \"{html}\"").count(), 1);
    }
}
