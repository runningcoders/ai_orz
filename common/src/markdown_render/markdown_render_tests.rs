//! markdown_render 纯函数测试（前后端共用判定口径）

use super::*;
use pulldown_cmark::Parser;

fn blocks(md: &str) -> Vec<Block> {
    parse_events(Parser::new(md))
}

#[test]
fn plain_text_is_plain() {
    let b = blocks("hello\nworld");
    assert_eq!(richness(&b), Richness::Plain);
    assert_eq!(blocks_to_plain_text(&b), "hello\nworld");
}

#[test]
fn bold_link_is_inline() {
    let b = blocks("**加粗** 和 [链接](https://example.com)");
    assert_eq!(richness(&b), Richness::Inline);
    assert_eq!(blocks_to_plain_text(&b), "加粗 和 链接");
}

#[test]
fn heading_list_code_quote_rule_are_block() {
    for md in [
        "# 标题",
        "- 列表",
        "```rust\nfn main() {}\n```",
        "> 引用",
        "---",
    ] {
        assert_eq!(richness(&blocks(md)), Richness::Block, "md={md}");
    }
}

#[test]
fn heading_level_and_inline_preserved() {
    let b = blocks("### 三级 **标题**");
    assert_eq!(
        b,
        vec![Block::Heading {
            level: 3,
            inlines: vec![
                Inline::Text("三级 ".to_string()),
                Inline::Strong(vec![Inline::Text("标题".to_string())]),
            ],
        }]
    );
}

#[test]
fn nested_list_keeps_depth() {
    let b = blocks("- 一\n  - 子项\n- 二");
    let Some(Block::List { ordered, items }) = b.first() else {
        panic!("应为列表");
    };
    assert!(!ordered);
    let depths: Vec<usize> = items.iter().map(|i| i.depth).collect();
    assert_eq!(depths, vec![0, 1, 0]);
}

#[test]
fn fenced_code_captures_lang_and_body() {
    let b = blocks("```python\nprint(1)\nprint(2)\n```");
    assert_eq!(
        b,
        vec![Block::CodeBlock {
            lang: Some("python".to_string()),
            code: "print(1)\nprint(2)".to_string(),
        }]
    );
}

#[test]
fn html_is_demoted_to_text() {
    let b = blocks("<script>x</script>");
    assert!(matches!(b.first(), Some(Block::Paragraph(_))));
    // HTML 事件降级为文本，不产生结构
    assert_eq!(richness(&b), Richness::Plain);
    assert_eq!(blocks_to_plain_text(&b), "<script>x</script>");
}

#[test]
fn markdown_roundtrip_keeps_structure() {
    let src = "# 标题\n\n- 一\n- 二\n\n```rust\nlet x = 1;\n```\n\n> 引用";
    let out = blocks_to_markdown(&blocks(src));
    assert!(out.contains("# 标题"));
    assert!(out.contains("- 一"));
    assert!(out.contains("```rust"));
    assert!(out.contains("> 引用"));
}

#[test]
fn empty_input_gives_empty_ir() {
    let b = blocks("");
    assert!(b.is_empty());
    assert_eq!(richness(&b), Richness::Plain);
    assert!(blocks_to_plain_text(&b).is_empty());
}

#[test]
fn table_text_falls_back_to_paragraph() {
    // 表格无跨渠道节点：结构标记忽略、单元格文本保留为普通段落
    let b = blocks("| a | b |\n|---|---|\n| c | d |");
    let joined = blocks_to_plain_text(&b);
    assert!(joined.contains('a'));
    assert!(joined.contains('d'));
    assert_eq!(richness(&b), Richness::Plain);
}

#[test]
fn quote_with_nested_list_keeps_all_text() {
    // 回归：嵌套列表提前结算父 Item 后，父 Item 的 End(Item) 到达时栈顶是
    // Quote；旧实现无条件 pop 会吞掉 Quote 帧，导致引用首段文本丢失。
    let text = blocks_to_plain_text(&blocks("> intro\n> - parent\n>   - child"));
    assert!(text.contains("intro"), "引用首段丢失: {text:?}");
    assert!(text.contains("parent"), "父项丢失: {text:?}");
    assert!(text.contains("child"), "子项丢失: {text:?}");
}

#[test]
fn link_with_nested_style_is_single_node() {
    // 回归：样式栈在 End 时整段包裹，链接内的粗体不能被拆成两个 Link。
    let b = blocks("[**粗**链](https://e.com)");
    assert_eq!(
        b,
        vec![Block::Paragraph(vec![Inline::Link {
            text: vec![
                Inline::Strong(vec![Inline::Text("粗".into())]),
                Inline::Text("链".into()),
            ],
            dest: "https://e.com".into(),
        }])]
    );
}

#[test]
fn markdown_roundtrip_escapes_inline_code_and_link_dest() {
    // 用 IR 直接构造含反引号的代码节点（源文难以用单反引号围栏表达）
    let blocks = vec![Block::Paragraph(vec![
        Inline::Code("a`b".into()),
        Inline::Text(" 与 ".into()),
        Inline::Link {
            text: vec![Inline::Text("x".into())],
            dest: "https://e.com/a(b) c".into(),
        },
    ])];
    let out = blocks_to_markdown(&blocks);
    assert!(out.contains("``a`b``"), "行内代码围栏不足: {out}");
    assert!(
        out.contains("<https://e.com/a(b) c>"),
        "链接 dest 未用 <> 包裹: {out}"
    );
}
