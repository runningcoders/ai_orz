//! tests 单元测试（拆分自 mention.rs）
//!
//! 文件瘦身：原 743 行 → 375 行，测试体 369 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

#[test]
fn parse_mention_dest_accepts_three_kinds() {
    assert_eq!(
        parse_mention_dest("agent:agt_7f3"),
        Some(MentionRef {
            kind: MentionKind::Agent,
            id: "agt_7f3".to_string(),
            org: None
        })
    );
    assert_eq!(
        parse_mention_dest("task:tsk_a91"),
        Some(MentionRef {
            kind: MentionKind::Task,
            id: "tsk_a91".to_string(),
            org: None
        })
    );
    assert_eq!(
        parse_mention_dest("project:prj_2c8"),
        Some(MentionRef {
            kind: MentionKind::Project,
            id: "prj_2c8".to_string(),
            org: None
        })
    );
}

#[test]
fn parse_mention_dest_federated_agent() {
    assert_eq!(
        parse_mention_dest("agent:agt_9@org-B12"),
        Some(MentionRef {
            kind: MentionKind::Agent,
            id: "agt_9".to_string(),
            org: Some("org-B12".to_string())
        })
    );
    // 非法形态降级普通链接
    assert_eq!(parse_mention_dest("agent:@org-B12"), None);
    assert_eq!(parse_mention_dest("agent:agt_9@"), None);
    assert_eq!(parse_mention_dest("agent:agt_9@org@a"), None);
    // 非 Agent 类型不支持 org 后缀
    assert_eq!(parse_mention_dest("task:tsk_1@org-B12"), None);
    // org 段含空白整体不合法（外层空白检查已覆盖，这里验证组合形态）
    assert_eq!(parse_mention_dest("agent:agt_9@org B"), None);
}

#[test]
fn parse_mention_dest_rejects_non_mention() {
    // 普通链接 / 站内链接 / 邮链不得被误吞
    assert_eq!(parse_mention_dest("https://example.com/a"), None);
    assert_eq!(parse_mention_dest("docs/design/runtime.md"), None);
    assert_eq!(parse_mention_dest("mailto:a@b.com"), None);
    // 未知类型与空 id
    assert_eq!(parse_mention_dest("user:u_1"), None);
    assert_eq!(parse_mention_dest("agent:"), None);
    // 含空白（原文形如 [x](agent:a b)）
    assert_eq!(parse_mention_dest("agent:a b"), None);
}

#[test]
fn format_mention_escapes_bracket() {
    assert_eq!(
        format_mention(MentionKind::Agent, "agt_1", "张伟"),
        "[@张伟](agent:agt_1)"
    );
    // 名字里的 ] 会截断链接语法，必须转义
    assert_eq!(
        format_mention(MentionKind::Task, "tsk_1", "a]b"),
        "[@a\\]b](task:tsk_1)"
    );
}

#[test]
fn resolve_display_name_prefers_directory() {
    let mut agents = HashMap::new();
    agents.insert("agt_7f3".to_string(), "张伟（新）".to_string());

    let m = MentionRef {
        kind: MentionKind::Agent,
        id: "agt_7f3".to_string(),
        org: None,
    };
    // 命中目录：用实时名，快照名被覆盖（改名后历史消息自动同步）
    assert_eq!(
        resolve_display_name(&m, "张伟", Some(&agents)),
        "张伟（新）"
    );
    // 未命中：回退快照
    assert_eq!(resolve_display_name(&m, "张伟", None), "张伟");

    // Task 类型不走 Agent 目录
    let t = MentionRef {
        kind: MentionKind::Task,
        id: "tsk_a91".to_string(),
        org: None,
    };
    assert_eq!(
        resolve_display_name(&t, "数据清洗", Some(&agents)),
        "数据清洗"
    );
}

/// 便捷构造：`text` 里用 `|` 标记光标位置
fn detect(text: &str) -> Option<MentionQuery> {
    let caret = text.find('|').expect("测试文本需用 | 标记光标");
    let clean = text.replace('|', "");
    detect_mention_query(&clean, caret)
}

#[test]
fn detect_query_triggers_on_boundary() {
    // 行首刚打完 @：菜单应打开，关键词为空（展示全部候选）
    let q = detect("@|").expect("行首 @ 应触发");
    assert_eq!((q.start, q.caret, q.query.as_str()), (0, 1, ""));

    // 空格后 @ + 中文关键词
    let q = detect("你好 @张|").expect("空格后 @ 应触发");
    assert_eq!((q.start, q.query.as_str()), (7, "张"));

    // 开括号后同样触发（用户刚删掉一个提及再重新 @ 的常见位置）
    assert!(detect("（@|").is_some());
}

#[test]
fn detect_query_ignores_left_context_entirely() {
    // 放开边界后：`@` 前是什么字符都触发。邮箱 / URL 本地部分也照弹
    // （用户继续输入时关键词无命中，前端会自动收起菜单）
    assert!(detect("a@b.com|").is_some());
    assert!(detect("联系zhang@|").is_some());
    assert!(detect("https://x.com/@u|").is_some());
    // 仍然挡住的：@ 与光标之间有空白 → 光标已不在查询里
    assert!(detect("@张伟 |").is_none());
    // 纯文本无 @
    assert!(detect("你好世界|").is_none());
}

#[test]
fn detect_query_triggers_after_cjk_and_punctuation() {
    // 回归：中文写作不补空格，@ 直接紧跟上一个汉字也必须触发
    // （早期白名单实现下这里一律返回 None，等于「只有行首能用」）
    let q = detect("帮我看下@张|").expect("汉字后 @ 应触发");
    assert_eq!((q.start, q.query.as_str()), ("帮我看下".len(), "张"));

    // 中英文标点后触发
    assert!(detect("你好，@|").is_some());
    assert!(detect("进度：@张|").is_some());
    assert!(detect("先做完这个。@|").is_some());
    // 闭合括号 / 全角空格（中文输入法常见）也算边界
    assert!(detect("（已完成）@|").is_some());
    assert!(detect("你好　@张|").is_some());
}

#[test]
fn detect_query_triggers_after_ascii_word() {
    // 放开边界：ASCII 单词紧贴的 @ 也触发（`hi@张` 这类不补空格的写法必须能用）
    assert!(detect("hi@|").is_some());
    assert!(detect("user_1@|").is_some());
    // 邮箱中段同样触发：query 取 @ 之后的内容，由前端空结果自动收起
    let q = detect("zhang@qq|").expect("邮箱中段也触发");
    assert_eq!(q.query, "qq");
    // 但 @ 之后一旦出现空白，就不再算「正在输入查询」
    assert!(detect("zhang@qq |").is_none());
}

#[test]
fn detect_query_rejects_after_inserted_mention() {
    // 关键回归：刚插入的提及语法里含 @，光标停在末尾时不得再次弹菜单
    assert!(detect("[@张伟](agent:agt_1)|").is_none());
    // 提及后再输入普通文字，同样不该弹
    assert!(detect("[@张伟](agent:agt_1) 你好|").is_none());
    // 但空格后重新打 @ 应当正常触发
    let q = detect("[@张伟](agent:agt_1) @李|").expect("新 @ 应触发");
    assert_eq!(q.query, "李");
}

#[test]
fn apply_pick_replaces_query_range() {
    let q = detect("看下 @张| 的进度").expect("应检测到查询");
    let token = format_mention(MentionKind::Agent, "agt_7f3", "张伟");
    let (text, caret) = apply_mention_pick("看下 @张 的进度", &q, &token);

    // 替换区间是 @ 起至光标位，原文里被 @ 查询占掉的「张」一并吃掉
    assert_eq!(text, "看下 [@张伟](agent:agt_7f3)  的进度");
    // 光标停在插入内容之后（含补的空格）
    assert_eq!(caret, q.start + token.len() + 1);
    assert_eq!(&text[..caret], "看下 [@张伟](agent:agt_7f3) ");
}

#[test]
fn apply_pick_at_text_end() {
    let q = detect("你好 @|").expect("应检测到查询");
    let token = format_mention(MentionKind::Task, "tsk_a91", "数据清洗");
    let (text, caret) = apply_mention_pick("你好 @", &q, &token);
    assert_eq!(text, "你好 [@数据清洗](task:tsk_a91) ");
    assert_eq!(caret, text.len());
}

#[test]
fn remove_token_strips_one_space() {
    let token = format_mention(MentionKind::Agent, "agt_1", "张伟");
    let text = format!("请 {} 跟进一下", token);
    assert_eq!(remove_mention_token(&text, &token), "请 跟进一下");
    // token 不存在时原样返回，不破坏用户已编辑的内容
    assert_eq!(remove_mention_token("随便写的", &token), "随便写的");
    // 尾部提及（后面没有空格）也能干净摘除
    let tail = format!("你好 {}", token);
    assert_eq!(remove_mention_token(&tail, &token), "你好 ");
}

#[test]
fn extract_finds_all_kinds_in_order() {
    let text = "请 [@张伟](agent:agt_1) 跟进 [@A](task:t1) 和 [@B](task:t2)，\
                背景见 [@平台](project:p1)";
    assert_eq!(
        extract_mentions(text),
        vec![
            MentionRef {
                kind: MentionKind::Agent,
                id: "agt_1".into(),
                org: None
            },
            MentionRef {
                kind: MentionKind::Task,
                id: "t1".into(),
                org: None
            },
            MentionRef {
                kind: MentionKind::Task,
                id: "t2".into(),
                org: None
            },
            MentionRef {
                kind: MentionKind::Project,
                id: "p1".into(),
                org: None
            },
        ]
    );
}

#[test]
fn extract_ignores_normal_links_and_emails() {
    let text = "[文档](https://example.com) 与 [站内](docs/a.md) 参考，\
                邮箱 a@b.com 不含链接语法；[@张伟](agent:agt_1) 是唯一提及";
    assert_eq!(
        extract_mentions(text),
        vec![MentionRef {
            kind: MentionKind::Agent,
            id: "agt_1".into(),
            org: None
        }]
    );
}

#[test]
fn extract_federated_agent_mention() {
    let text = "请 [@远端助手](agent:agt_9@org-B12) 帮忙翻译";
    let got = extract_mentions_with_text(text);
    assert_eq!(
        got,
        vec![(
            MentionRef {
                kind: MentionKind::Agent,
                id: "agt_9".into(),
                org: Some("org-B12".into())
            },
            "远端助手".to_string()
        )]
    );
}

#[test]
fn format_mention_ref_federated_roundtrip() {
    let m = parse_mention_dest("agent:agt_9@org-B12").unwrap();
    let token = format_mention_ref(&m, "远端助手");
    assert_eq!(token, "[@远端助手](agent:agt_9@org-B12)");
    // 序列化往返：旧数据（无 org 字段）反序列化为 None
    let legacy: MentionRef = serde_json::from_str(r#"{"kind":"Agent","id":"agt_1"}"#).unwrap();
    assert_eq!(legacy.org, None);
    assert_eq!(
        serde_json::to_value(&m).unwrap(),
        serde_json::json!({"kind":"Agent","id":"agt_9","org":"org-B12"})
    );
}

#[test]
fn extract_skips_unclosed_and_whitespace_dest() {
    // 未闭合链接、dest 含空白都不是提及
    assert!(extract_mentions("[@x](agent:agt_1").is_empty());
    assert!(extract_mentions("[@x](agent:agt_1 more)").is_empty());
    assert!(extract_mentions("没有链接的普通文本").is_empty());
}

#[test]
fn extract_survives_escaped_bracket_name() {
    // format_mention 转义过的名字（含 \]）仍能正常提取
    let token = format_mention(MentionKind::Agent, "agt_1", "a]b");
    assert_eq!(
        extract_mentions(&format!("看 {} ", token)),
        vec![MentionRef {
            kind: MentionKind::Agent,
            id: "agt_1".into(),
            org: None
        }]
    );
}

#[test]
fn extract_with_text_captures_snapshot_name() {
    let text = "请 [@张伟](agent:agt_1) 看 [@数据清洗](task:tsk_a) 与 [@平台](project:prj_1)";
    let got = extract_mentions_with_text(text);
    assert_eq!(
        got,
        vec![
            (
                MentionRef {
                    kind: MentionKind::Agent,
                    id: "agt_1".into(),
                    org: None
                },
                "张伟".to_string()
            ),
            (
                MentionRef {
                    kind: MentionKind::Task,
                    id: "tsk_a".into(),
                    org: None
                },
                "数据清洗".to_string()
            ),
            (
                MentionRef {
                    kind: MentionKind::Project,
                    id: "prj_1".into(),
                    org: None
                },
                "平台".to_string()
            ),
        ]
    );
}

#[test]
fn resolved_mention_kind_label() {
    assert_eq!(
        ResolvedMention {
            kind: MentionKind::Agent,
            id: "x".into(),
            name: "张".into(),
            summary: None
        }
        .kind_label(),
        "Agent"
    );
    assert_eq!(
        ResolvedMention {
            kind: MentionKind::Task,
            id: "x".into(),
            name: "t".into(),
            summary: None
        }
        .kind_label(),
        "任务"
    );
}
