//! tests 单元测试（拆分自 mod.rs）
//!
//! 文件瘦身：原 648 行 → 264 行，测试体 385 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::mention::{MentionKind, MentionRef, extract_mentions_with_text, resolve_display_name};
use pulldown_cmark::LinkType;
use std::cell::RefCell;
use std::collections::HashMap;

/// 旧逻辑复刻（迁移前 frontend transform_mentions 实现口径，对拍基准）：
/// 仅 mention 拦截 + HTML 降级；命中产出统一标记事件（chip HTML 归前端层，
/// 由 frontend 既有测试与 T2 适配守护），以便与薄封装逐事件对拍。
fn legacy_transform_mentions<'a, I>(
    events: I,
    agents: Option<&HashMap<String, String>>,
) -> Vec<Event<'a>>
where
    I: Iterator<Item = Event<'a>>,
{
    let mut out: Vec<Event<'a>> = Vec::new();
    let mut pending: Option<MentionRef> = None;
    let mut name_buf = String::new();
    for event in events {
        if pending.is_some() {
            let is_end = matches!(event, Event::End(TagEnd::Link));
            match &event {
                Event::Text(t) | Event::Code(t) => name_buf.push_str(t),
                Event::SoftBreak | Event::HardBreak => name_buf.push(' '),
                _ => {}
            }
            if is_end {
                let m = pending.take().unwrap_or(MentionRef {
                    kind: MentionKind::Agent,
                    id: String::new(),
                    org: None,
                });
                let snapshot = normalize_snapshot(&name_buf, &m.id);
                let name = resolve_display_name(&m, &snapshot, agents);
                out.push(Event::InlineHtml(
                    format!("CHIP:{}|{}", m.kind.as_str(), name).into(),
                ));
            }
            continue;
        }
        match event {
            Event::Html(raw) | Event::InlineHtml(raw) => out.push(Event::Text(raw)),
            Event::Start(tag @ Tag::Link { .. }) => {
                let dest = match &tag {
                    Tag::Link { dest_url, .. } => dest_url.to_string(),
                    _ => unreachable!(),
                };
                match parse_mention_dest(&dest) {
                    Some(m) => {
                        pending = Some(m);
                        name_buf.clear();
                    }
                    None => out.push(Event::Start(tag)),
                }
            }
            other => out.push(other),
        }
    }
    if let Some(m) = pending.take() {
        let snapshot = normalize_snapshot(&name_buf, &m.id);
        let name = resolve_display_name(&m, &snapshot, agents);
        out.push(Event::InlineHtml(
            format!("CHIP:{}|{}", m.kind.as_str(), name).into(),
        ));
    }
    out
}

/// 薄封装口径：common transform + mention 适配 render（T2 前端形态预演）
fn thin_wrapper<'a>(md: &'a str, agents: Option<&HashMap<String, String>>) -> Vec<Event<'a>> {
    let reg = ProtocolRegistry::default_registry();
    let opts = TransformOptions {
        demote_raw_html: true,
    };
    transform(
        pulldown_cmark::Parser::new(md),
        &reg,
        &opts,
        |r: &ProtocolRef, snapshot: &str, _img: bool| {
            let m = r.to_mention()?;
            let name = resolve_display_name(&m, snapshot, agents);
            Some(Event::InlineHtml(
                format!("CHIP:{}|{}", m.kind.as_str(), name).into(),
            ))
        },
    )
}

/// 对拍黄金样例（T1 完成回报附带：T2 前端对拍样例输入集基础）
/// 图像内联为批1 有意扩展（旧逻辑不拦 Image），不进对拍集，单独用例守护。
#[test]
fn transform_thin_wrapper_matches_legacy_golden_cases() {
    let mut agents = HashMap::new();
    agents.insert("agt_7f3".to_string(), "李雷".to_string());
    let cases: &[(&str, bool)] = &[
        ("[@张伟](agent:agt_7f3) 看下进度", false),
        ("[@张伟](agent:agt_7f3) 你好", true),
        ("[@A](task:t1) 和 [@B](task:t2) 都阻塞了", false),
        ("[@远端助手](agent:agt_9@org-B12) 跨组织", false),
        ("[文档](https://example.com) 参考", false),
        ("[x](user:u1) 未注册 scheme 降级普通链接", false),
        ("[x](task:tsk_1@org) 非 agent 带 org 降级", false),
        (
            "<script>alert(1)</script> 与 <b>粗体</b> 源文 HTML 转义",
            false,
        ),
        ("[](agent:agt_e) 空名回退 id", false),
        ("[@a\\]b](task:tsk_1) 转义名", false),
        ("[@多\n行](agent:agt_m) 换行名", false),
        ("无任何链接的普通文本", false),
    ];
    for (md, use_agents) in cases {
        let agents_opt = if *use_agents { Some(&agents) } else { None };
        let legacy = legacy_transform_mentions(pulldown_cmark::Parser::new(md), agents_opt);
        let thin = thin_wrapper(md, agents_opt);
        assert_eq!(legacy, thin, "对拍失败：{md}");
    }
}

#[test]
fn transform_intercepts_registered_image() {
    let reg = ProtocolRegistry::default_registry();
    let called: RefCell<Vec<(String, String, String, bool)>> = RefCell::new(Vec::new());
    let md = "![预览](attachment:att_1)";
    let out = transform(
        pulldown_cmark::Parser::new(md),
        &reg,
        &TransformOptions {
            demote_raw_html: true,
        },
        |r: &ProtocolRef, snapshot: &str, is_image: bool| {
            called.borrow_mut().push((
                r.scheme.clone(),
                r.id.clone(),
                snapshot.to_string(),
                is_image,
            ));
            Some(Event::Text(format!("[IMG:{}|{snapshot}]", r.scheme).into()))
        },
    );
    assert_eq!(
        called.into_inner(),
        vec![(
            "attachment".to_string(),
            "att_1".to_string(),
            "预览".to_string(),
            true
        )]
    );
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out.into_iter());
    assert!(html.contains("[IMG:attachment|预览]"));
}

#[test]
fn transform_passes_unregistered_image_through() {
    let reg = ProtocolRegistry::default_registry();
    let md = "![alt](https://example.com/a.png)";
    let out = transform(
        pulldown_cmark::Parser::new(md),
        &reg,
        &TransformOptions {
            demote_raw_html: true,
        },
        |_, _, _| Some(Event::Text("RENDERED".into())),
    );
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out.into_iter());
    assert!(html.contains("<img"));
    assert!(!html.contains("RENDERED"));
}

#[test]
fn transform_demote_raw_html_switch() {
    let reg = ProtocolRegistry::default_registry();
    let md = "<b>x</b>";
    let no_render = |_: &ProtocolRef, _: &str, _: bool| -> Option<Event<'static>> { None };
    let demoted = transform(
        pulldown_cmark::Parser::new(md),
        &reg,
        &TransformOptions {
            demote_raw_html: true,
        },
        no_render,
    );
    assert!(
        demoted
            .iter()
            .all(|e| !matches!(e, Event::Html(_) | Event::InlineHtml(_)))
    );
    let kept = transform(
        pulldown_cmark::Parser::new(md),
        &reg,
        &TransformOptions {
            demote_raw_html: false,
        },
        no_render,
    );
    assert!(
        kept.iter()
            .any(|e| matches!(e, Event::Html(_) | Event::InlineHtml(_)))
    );
}

#[test]
fn transform_unclosed_link_fallback_emits_collected() {
    // 手工构造未闭合事件流（理论上 parser 不产生），兜底吐已收集内容
    let reg = ProtocolRegistry::default_registry();
    let events = vec![
        Event::Start(Tag::Link {
            link_type: LinkType::Inline,
            dest_url: "agent:agt_1".into(),
            title: "".into(),
            id: "".into(),
        }),
        Event::Text("x".into()),
    ];
    let out = transform(
        events.into_iter(),
        &reg,
        &TransformOptions {
            demote_raw_html: true,
        },
        |r: &ProtocolRef, s: &str, _| Some(Event::Text(format!("{}:{s}", r.id).into())),
    );
    assert_eq!(out, vec![Event::Text("agt_1:x".into())]);
}

#[test]
fn transform_extension_demo_new_scheme_by_registry_only() {
    // 「一组件 + 一行注册」演示：test scheme 注册后 transform 立即拦截，解析器零改动
    struct TestComponent;
    impl ProtocolComponent for TestComponent {
        fn scheme(&self) -> &'static str {
            "test"
        }
        fn participates_in_notify(&self) -> bool {
            false
        }
        fn render_text(&self, _r: &ProtocolRef, snapshot: &str) -> String {
            format!("[测试：{snapshot}]")
        }
    }
    let mut reg = ProtocolRegistry::default_registry();
    reg.register(std::sync::Arc::new(TestComponent));
    let md = "[x](test:1)";
    let out = transform(
        pulldown_cmark::Parser::new(md),
        &reg,
        &TransformOptions {
            demote_raw_html: true,
        },
        |r: &ProtocolRef, s: &str, _| Some(Event::Text(format!("{}:{s}", r.scheme).into())),
    );
    // 拦截成功：完整 parser 输出含段落包装，断言命中产出存在且原链接不外泄
    assert!(
        out.iter()
            .any(|e| matches!(e, Event::Text(t) if t.as_ref() == "test:x"))
    );
    assert!(
        !out.iter()
            .any(|e| matches!(e, Event::Start(Tag::Link { .. })))
    );
}

#[test]
fn parse_protocol_emits_ref_image_and_plain() {
    let reg = ProtocolRegistry::default_registry();
    let evs = parse_protocol(
        "看 [@张伟](agent:agt_1) 和 ![预览](attachment:att_1) 与 [文档](https://e.com)",
        &reg,
    );
    let mut refs = 0;
    let mut images = 0;
    let mut saw_plain_link = false;
    for e in &evs {
        match e {
            ProtocolEvent::Ref(r, s) => {
                refs += 1;
                assert_eq!(r.scheme, "agent");
                assert_eq!(r.id, "agt_1");
                assert_eq!(s, "张伟");
            }
            ProtocolEvent::Image(r, s) => {
                images += 1;
                assert_eq!(r.scheme, "attachment");
                assert_eq!(r.id, "att_1");
                assert_eq!(s, "预览");
            }
            ProtocolEvent::Plain(Event::Start(Tag::Link { dest_url, .. })) => {
                saw_plain_link = true;
                assert_eq!(dest_url.as_ref(), "https://e.com");
            }
            _ => {}
        }
    }
    assert_eq!((refs, images), (1, 1));
    assert!(saw_plain_link);
}

#[test]
fn extract_refs_agrees_with_extract_mentions_on_mention_forms() {
    let text = "请 [@张伟](agent:agt_1) 跟进 [@A](task:t1)，背景见 [@平台](project:p1)；\
                [文档](https://example.com) 与邮箱 a@b.com 不算；[@远端](agent:agt_9@org-B12)";
    let got: Vec<(MentionRef, String)> = extract_refs(text)
        .into_iter()
        .filter_map(|(r, s)| r.to_mention().map(|m| (m, s)))
        .collect();
    assert_eq!(got, extract_mentions_with_text(text));

    // 转义名形态同源（format_mention 产物可提取）
    let token = crate::mention::format_mention(MentionKind::Task, "tsk_1", "a]b");
    let got: Vec<(MentionRef, String)> = extract_refs(&token)
        .into_iter()
        .filter_map(|(r, s)| r.to_mention().map(|m| (m, s)))
        .collect();
    assert_eq!(got, extract_mentions_with_text(&token));
}

#[test]
fn extract_refs_generalized_schemes_and_rejects() {
    // 泛化 scheme（语法合法即收录，消费方按 registry 过滤）
    let got = extract_refs("文件 [设计稿.png](attachment:att_1) 与 [产物](artifact:art_9)");
    assert_eq!(
        got,
        vec![
            (
                ProtocolRef {
                    scheme: "attachment".into(),
                    id: "att_1".into(),
                    org: None
                },
                "设计稿.png".to_string()
            ),
            (
                ProtocolRef {
                    scheme: "artifact".into(),
                    id: "art_9".into(),
                    org: None
                },
                "产物".to_string()
            ),
        ]
    );
    // 非法形态：dest 含空白 / 未闭合 / 非 agent 带 @ / 空段
    assert!(extract_refs("[x](attachment:a b)").is_empty());
    assert!(extract_refs("[x](attachment:att_1").is_empty());
    assert!(extract_refs("[x](task:t1@org)").is_empty());
    assert!(extract_refs("[x](attachment:)").is_empty());
    // 未转义 ] 截断：]（ 之前的最近 [ 为空文本
    assert!(extract_refs("没有链接的普通文本").is_empty());
}

#[test]
fn format_ref_roundtrip_mention_and_generic() {
    // mention 形态与 format_mention 逐字一致（单一事实源复用）
    let m = parse_mention_dest("agent:agt_1").expect("合法提及");
    let r = ProtocolRef::from(&m);
    assert_eq!(format_ref(&r, "张伟"), "[@张伟](agent:agt_1)");
    assert_eq!(format_ref(&r, "a]b"), "[@a\\]b](agent:agt_1)");
    let federated = ProtocolRef::from(&parse_mention_dest("agent:agt_9@org-B12").unwrap());
    assert_eq!(format_ref(&federated, "远"), "[@远](agent:agt_9@org-B12)");
    // 泛化形态 + extract 往返
    let g = ProtocolRef {
        scheme: "attachment".into(),
        id: "att_1".into(),
        org: None,
    };
    let text = format_ref(&g, "设计稿.png");
    assert_eq!(text, "[设计稿.png](attachment:att_1)");
    assert_eq!(extract_refs(&text), vec![(g, "设计稿.png".to_string())]);
}

/// 批1 验收 #5：Mixed=13 仅枚举预留——From(13) 可还原、未知值回退不变、
/// 无任何消费点（生产写入路径归批3，grep 证明见 T1 回报）。
#[test]
fn mixed_enum_reserved_without_behavior() {
    use crate::enums::MessageType;
    assert_eq!(MessageType::from(13), MessageType::Mixed);
    assert_eq!(MessageType::Mixed.to_i32(), 13);
    assert_eq!(MessageType::from(999), MessageType::Text);
}
