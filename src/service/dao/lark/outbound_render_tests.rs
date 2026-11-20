//! outbound_render 测试（飞书官方 md 标签协议）

use super::*;

fn parse(v: &str) -> serde_json::Value {
    serde_json::from_str(v).expect("合法 JSON")
}

#[test]
fn plain_text_uses_post_md_tag() {
    let (kind, content) = render_outbound("今天天气不错");
    assert_eq!(kind, "post");
    let v = parse(&content);
    // 服务端 API 的 content 顶层是 zh_cn，不包 post
    assert!(v.get("post").is_none(), "不应包 post 外层");
    assert_eq!(v["zh_cn"]["content"][0][0]["tag"], "md");
    assert_eq!(v["zh_cn"]["content"][0][0]["text"], "今天天气不错");
}

#[test]
fn markdown_source_is_kept_verbatim() {
    // md 标签原生承载完整 GFM：原文不做 IR 回写，表格 / 代码块原样透传
    let src = "# 标题\n\n| a | b |\n|---|---|\n| c | d |\n\n- [x] done";
    let (kind, content) = render_outbound(src);
    assert_eq!(kind, "post");
    let v = parse(&content);
    assert_eq!(v["zh_cn"]["content"][0][0]["text"], src);
}

#[test]
fn oversize_uses_text() {
    let long = "x".repeat(POST_CONTENT_LIMIT + 1);
    let (kind, content) = render_outbound(&long);
    assert_eq!(kind, "text");
    assert_eq!(parse(&content)["text"], long);
}

#[test]
fn at_limit_uses_post() {
    let exact = "x".repeat(POST_CONTENT_LIMIT);
    assert_eq!(render_outbound(&exact).0, "post");
}

#[test]
fn empty_uses_text() {
    let (kind, content) = render_outbound("  ");
    assert_eq!(kind, "text");
    assert_eq!(parse(&content)["text"], "  ");
}
