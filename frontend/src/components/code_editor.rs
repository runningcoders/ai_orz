//! 轻量代码编辑器组件：textarea + 等宽字体 + 行号显示
//!
//! 不引入 Monaco/CodeMirror 等重量级编辑器，使用纯 textarea + 行号同步滚动
//! 适用于 Skill 文件内容编辑、Artifact 内容编辑等场景

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct CodeEditorProps {
    /// 当前内容
    value: String,
    /// 内容变更回调
    on_input: EventHandler<String>,
    /// 语言（用于占位符显示，不实际做语法高亮）
    #[props(default = "text".to_string())]
    language: String,
    /// 是否只读
    #[props(default = false)]
    read_only: bool,
    /// 最小行数（控制高度）
    #[props(default = 16)]
    min_lines: u32,
}

#[component]
pub fn CodeEditor(props: CodeEditorProps) -> Element {
    let line_count = props.value.lines().count().max(props.min_lines as usize);
    let line_numbers = (1..=line_count)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    rsx! {
        div { class: "code-editor-container",
            style: "display: flex; border: 1px solid var(--color-border, #e5e7eb); border-radius: var(--radius-md, 6px); overflow: hidden; background: var(--color-mistral-black, #1e1e1e);",
            // 行号栏
            pre {
                class: "code-editor-line-numbers",
                style: "margin: 0; padding: 12px 8px; min-width: 48px; text-align: right; color: #6b7280; font-family: ui-monospace, 'SF Mono', Menlo, monospace; font-size: 13px; line-height: 1.6; user-select: none; overflow: hidden; white-space: pre;",
                "{line_numbers}"
            }
            // 编辑区
            textarea {
                class: "code-editor-textarea",
                style: "flex: 1; min-height: {props.min_lines * 24}px; padding: 12px; border: none; outline: none; resize: vertical; background: transparent; color: var(--color-text-on-dark, #e5e7eb); font-family: ui-monospace, 'SF Mono', Menlo, monospace; font-size: 13px; line-height: 1.6; white-space: pre; overflow: auto;",
                value: "{props.value}",
                readonly: props.read_only,
                placeholder: "请输入 {props.language} 内容...",
                oninput: move |e| props.on_input.call(e.value()),
                spellcheck: "false",
                autocomplete: "off",
                autocapitalize: "off",
                autocorrect: "off",
            }
        }
    }
}
