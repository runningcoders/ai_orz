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
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;

use crate::api::finance::get_attachment;
use crate::store::directory::Directory;
use crate::utils::doc_link::DocRenderContext;
use crate::utils::mention::{AttachmentMeta, transform_mentions};
use crate::utils::message::NameMap;
use common::markdown_protocol::extract_refs;

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
/// 不带名称上下文：提及 chip 用消息文本里的快照名。
pub fn render_markdown(
    md: &str,
    attachment_meta: Option<&HashMap<String, AttachmentMeta>>,
) -> String {
    render_markdown_inner(md, None, None, attachment_meta)
}

fn render_markdown_inner(
    md: &str,
    agents: Option<&NameMap>,
    doc_ctx: Option<&DocRenderContext>,
    attachment_meta: Option<&HashMap<String, AttachmentMeta>>,
) -> String {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_TABLES);
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    let parser = pulldown_cmark::Parser::new_ext(md, options);
    let events = transform_mentions(parser, agents, attachment_meta);
    let mut html_out = String::new();
    pulldown_cmark::html::push_html(&mut html_out, events.into_iter());
    // 站内链接分发：文档上下文存在时归一化 `./` 并站内切换，否则预拼 data-repo-href
    crate::utils::doc_link::post_process_doc_links(
        &html_out,
        crate::utils::doc_link::BLOB_BASE,
        doc_ctx,
    )
}

/// `MarkdownRenderer` 每实例的渲染缓存（内容寻址）
///
/// 失效键 = `content` + 目录快照（`Option<NameMap>`，仅 `agents` 参与 HTML 计算）。
/// 附件元信息快照（`description` + `mime_type` + `original_name`，仅
/// `resolve_attachment_descriptions` 开启时参与）：任一变化即全量重解析，都未变时
/// 直接复用缓存 HTML —— chat / 长列表里父组件的重渲染只剩一次比对，不再触发 O(n) 重解析。
#[derive(Default)]
struct MdCache {
    /// 上次解析的 Markdown 源文
    content: String,
    /// 上次解析使用的目录快照（None = 无目录上下文）。仅目录失配重算时克隆一次，命中路径零克隆
    names: Option<NameMap>,
    /// 上次解析使用的文档仓库路径（None = 非文档中心场景）
    doc_repo_path: Option<String>,
    /// 上次解析使用的附件摘要快照（None = 未开启 / 尚未拉取）。description 异步到位后
    /// 与当前 signal 比对，失配即重算，让 figcaption 在拉取完成后补上。
    att_meta: Option<HashMap<String, AttachmentMeta>>,
    /// 上次解析产物
    html: String,
}

/// Markdown 渲染组件
///
/// - `content`: Markdown 源文本
/// - `compact`: 紧凑模式（更小字号 / 收紧上下边距），用于卡片、聊天气泡、列表展开等场景
/// - `resolve_attachment_descriptions`: 为 true 时，自动解析正文内 `attachment:<id>` 引用，
///   异步拉取各附件的 `description` / `mime_type` / `original_name`：位图在下方渲染
///   `<figcaption>` 文字总结，PDF / 文档 / 音视频则渲染为可下载的文件 chip（不会裂图）。
///   聊天场景开启；文档中心等无附件图注需求的页面保持默认 false 零开销。
///
/// 内容寻缓存：`content` / 目录任一变化必重算（修复知识图谱详情「内容」栏切换不更新），
/// 都未变化则直接复用缓存（chat / 长列表高频父渲染下不丢性能）。
#[component]
pub fn MarkdownRenderer(
    content: String,
    #[props(default = false)] compact: bool,
    #[props(default)] doc_ctx: Option<DocRenderContext>,
    #[props(default = false)] resolve_attachment_descriptions: bool,
) -> Element {
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
    // 附件元信息异步拉取：正文内 `attachment:<id>` 引用 → 批量 get_attachment 取
    // description / mime_type / original_name，存入 signal；拉取完成（signal 变化）
    // 触发下方缓存失效重算，按 MIME 分派出图或下载 chip。
    // 无条件调用 hook（保持 hook 顺序稳定），仅在 `resolve_attachment_descriptions` 开启时真正拉取。
    let att_meta = use_signal(HashMap::<String, AttachmentMeta>::new);
    {
        let enabled = resolve_attachment_descriptions;
        // `att_meta`（Signal，Copy）被下方 move 闭包隐式拷贝进 effect，原绑定仍可在
        // 下方缓存块中 `att_meta.read()` 使用，无需额外 rebinding（避免 redundant_locals）。
        use_effect(use_reactive(&content, move |content: String| {
            if !enabled {
                return;
            }
            let mut att_meta = att_meta;
            spawn(async move {
                let known = att_meta.read();
                // 增量拉取：只请求尚未拉取过的 id。流式场景 content 每个 token 都会触发
                // 一次 effect，若每次全量重拉，N 个 token 会给后端放大 N 倍请求；同时
                // 下方用「读快照 + 合并 + 回写」而非整体替换，避免并发 task 相互覆盖。
                // 空描述 / 请求失败的 id 也写入（空串）以标记「已尝试」，既杜绝重试风暴，
                // 又让渲染侧自然跳过 figcaption（见 render_attachment_image 空串分支）。
                let todo: Vec<String> = extract_refs(&content)
                    .into_iter()
                    .filter(|(r, _)| r.scheme == "attachment")
                    .map(|(r, _)| r.id.clone())
                    .filter(|id| !known.contains_key(id))
                    .collect();
                // 读守卫在 await 之前释放：跨 await 持有 Signal guard 会与后续写入冲突
                drop(known);
                if todo.is_empty() {
                    return;
                }
                let mut fetched = HashMap::new();
                for id in todo {
                    // 一次请求拿齐摘要 / MIME / 文件名：MIME 决定 `<img>` 还是下载 chip，
                    // 别把 PDF 字节硬塞进 `<img>`。失败写 `default()`（mime 为空）既标记
                    // 「已尝试」防重试风暴，又让渲染侧落到文件 chip 而不是裂图。
                    let meta = match get_attachment(&id).await {
                        Ok(detail) => AttachmentMeta {
                            description: detail.description.trim().to_string(),
                            mime_type: detail.mime_type,
                            original_name: detail.original_name,
                        },
                        Err(_) => AttachmentMeta::default(),
                    };
                    fetched.insert(id, meta);
                }
                let mut merged = att_meta.read().clone();
                merged.extend(fetched);
                att_meta.set(merged);
            });
        }));
    }
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
        // 文档切换时原文位置变化 → 相对链接归一化的基准也变，必须重算
        let doc_path = doc_ctx.as_ref().map(|ctx| ctx.doc_repo_path.as_str());
        let doc_match = c.doc_repo_path.as_deref() == doc_path;
        // 附件元信息快照：仅开启拉取时取 signal 当前值（克隆一次），否则恒为 None ——
        // 关闭状态下 meta 与缓存快照（同为 None）必然匹配，不会空 map 反复重算。
        let meta_snapshot = if resolve_attachment_descriptions {
            Some(att_meta.read().clone())
        } else {
            None
        };
        let meta_match = c.att_meta == meta_snapshot;
        if c.content != content || !names_match || !doc_match || !meta_match {
            // 目录快照仅在失配时重新克隆：流式场景 content 每 token 失效一次，
            // 目录未变时不应付出整表克隆的代价。
            if !names_match {
                c.names = directory.as_ref().map(|sig| sig.read().agents.clone());
            }
            if !doc_match {
                c.doc_repo_path = doc_path.map(str::to_string);
            }
            if !meta_match {
                c.att_meta = meta_snapshot.clone();
            }
            // 文档上下文优先：全局 Directory provider 常驻（main.rs 注册），若先命中
            // directory 分支会把文档中心的站内链接分发整个跳过，导致链接全部退化 GitHub。
            c.html = match doc_ctx.as_ref() {
                Some(ctx) => {
                    render_markdown_inner(&content, None, Some(ctx), meta_snapshot.as_ref())
                }
                None => match directory.as_ref() {
                    Some(sig) => {
                        let d = sig.read();
                        render_markdown_inner(
                            &content,
                            Some(&d.agents),
                            None,
                            meta_snapshot.as_ref(),
                        )
                    }
                    None => render_markdown(&content, meta_snapshot.as_ref()),
                },
            };
            c.content = content.clone();
        }
        c.html.clone()
    };
    let container_id = use_hook(|| next_container_id("md"));
    // 含 ```mermaid 代码块时，挂载后（use_effect 已在 DOM 挂载后运行）直接调用 JS 渲染层替换为 SVG。
    // use_effect 只订阅 signal、无法跟踪普通 prop，这里用 use_reactive 把 html 转成
    // 反应式依赖：内容变化（含文档切换、流式追加）才重跑，未变化时零额外调用。
    // 不再用 spawn + 30ms sleep 延迟：该 future 会在所属组件卸载时被 drop，
    // 而 TimeoutFuture 底层 Closure::once 在「已入队未触发」时卸载会触发
    // "closure invoked recursively or after being dropped"（工作台 SSE 流式气泡高频复现）。
    {
        let id = container_id.clone();
        use_effect(dioxus::hooks::use_reactive(&html, move |html: String| {
            if html.contains("language-mermaid") {
                render_mermaid_blocks_now(&id);
            }
        }));
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
        // code 是普通 prop，use_effect 无法感知其变化（任务依赖图刷新后旧图不更新）；
        // use_reactive 把 code 转成反应式依赖，对内容变化重新渲染。
        use_effect(dioxus::hooks::use_reactive(&code, move |code: String| {
            let id = id.clone();
            spawn(async move {
                render_mermaid_code_now(&id, &code);
            });
        }));
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
        let html = render_markdown("# 标题\n- 一项\n- 两项", None);
        assert!(html.contains("<h1>标题</h1>"));
        assert!(html.contains("<li>一项</li>"));
        assert!(html.contains("<li>两项</li>"));
    }

    #[test]
    fn render_table_and_strikethrough() {
        let html = render_markdown("| a | b |\n|---|---|\n| 1 | 2 |\n\n~~删除~~", None);
        assert!(html.contains("<table>"));
        assert!(html.contains("<del>删除</del>"));
    }

    #[test]
    fn render_tasklist() {
        let html = render_markdown("- [ ] 未完成\n- [x] 已完成", None);
        assert!(html.contains("checkbox"));
    }

    #[test]
    fn render_escapes_raw_html() {
        // 原始 HTML（块级 + 内联）一律转义为文本，保证 dangerous_inner_html 注入安全
        let html = render_markdown("<script>alert(1)</script>\n\nhello <b>x</b>", None);
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<b>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;b&gt;"));
    }

    #[test]
    fn render_mermaid_block_kept_as_code() {
        // ```mermaid 代码块保留为带语言 class 的代码，交由 JS 渲染层处理
        let html = render_markdown("```mermaid\ngraph LR\nA-->B\n```", None);
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
            code.matches("if c.content != content || !names_match || !doc_match || !meta_match {")
                .count(),
            1
        );
        assert_eq!(code.matches("c.html.clone()").count(), 1);
        // mermaid 效果用 use_reactive 订阅 html（禁止回到渲染期写 signal 的旧实现）
        assert_eq!(
            code.matches("use_reactive(&html").count(),
            1,
            "mermaid 效果必须以 html 为反应式依赖"
        );
        assert_eq!(
            code.matches("html_signal").count(),
            0,
            "禁止渲染期写 signal 驱动 mermaid 效果"
        );
        assert_eq!(
            code.matches("use_reactive(&code").count(),
            1,
            "MermaidDiagram 必须以 code 为反应式依赖"
        );
        assert_eq!(code.matches("dangerous_inner_html: \"{html}\"").count(), 1);
    }
}
