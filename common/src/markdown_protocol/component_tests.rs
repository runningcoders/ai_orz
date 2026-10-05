//! tests 单元测试（拆分自 component.rs）
//!
//! 文件瘦身：原 495 行 → 276 行，测试体 220 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

#[test]
fn protocol_ref_mention_roundtrip() {
    // agent / task / project 保形互转（含 org 段）
    for dest in ["agent:agt_9@org-B12", "task:tsk_a91", "project:prj_2c8"] {
        let m = crate::mention::parse_mention_dest(dest).expect("合法提及 dest");
        let r = ProtocolRef::from(&m);
        assert_eq!(r.to_mention(), Some(m));
    }
}

#[test]
fn protocol_ref_non_mention_scheme_to_mention_none() {
    let r = ProtocolRef {
        scheme: "attachment".into(),
        id: "att_1".into(),
        org: None,
    };
    assert_eq!(r.to_mention(), None);
}

#[test]
fn mention_components_delegate_fact_source() {
    let reg = ProtocolRegistry::default_registry();
    for (scheme, kind) in [
        ("agent", MentionKind::Agent),
        ("task", MentionKind::Task),
        ("project", MentionKind::Project),
    ] {
        let c = reg.get(scheme).expect("mention 三组件已注册");
        assert_eq!(c.scheme(), scheme);
        assert!(c.participates_in_notify());
        let m = MentionRef {
            kind,
            id: "x1".into(),
            org: None,
        };
        assert_eq!(c.render_text(&ProtocolRef::from(&m), "张伟"), "@张伟");
    }
}

#[test]
fn resource_components_are_notify_silent() {
    let reg = ProtocolRegistry::default_registry();
    for scheme in ["attachment", "artifact"] {
        let c = reg.get(scheme).expect("资源占位组件已注册");
        assert!(!c.participates_in_notify());
        // extract 批1 快照通道默认 None（批2 DAL resolver 注入）
        let r = ProtocolRef {
            scheme: scheme.into(),
            id: "r1".into(),
            org: None,
        };
        assert_eq!(c.extract(&r), None);
    }
    assert_eq!(
        reg.get("attachment").unwrap().render_text(
            &ProtocolRef {
                scheme: "attachment".into(),
                id: "att_1".into(),
                org: None
            },
            "设计稿.png"
        ),
        "[附件：设计稿.png]"
    );
    assert_eq!(
        reg.get("artifact").unwrap().render_text(
            &ProtocolRef {
                scheme: "artifact".into(),
                id: "art_1".into(),
                org: None
            },
            "调研报告"
        ),
        "[产物：调研报告]"
    );
}

#[test]
fn registry_extension_demo_one_component_one_line() {
    // 「一组件 + 一行注册」演示：新 scheme 注册后立即具备拦截资格
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
    assert!(!reg.contains("test"));
    reg.register(Arc::new(TestComponent));
    assert!(reg.contains("test"));
    let r = ProtocolRef {
        scheme: "test".into(),
        id: "1".into(),
        org: None,
    };
    assert_eq!(reg.get("test").unwrap().render_text(&r, "x"), "[测试：x]");
}

#[test]
fn builder_chain_registers() {
    struct Another;
    impl ProtocolComponent for Another {
        fn scheme(&self) -> &'static str {
            "another"
        }
        fn participates_in_notify(&self) -> bool {
            false
        }
        fn render_text(&self, _r: &ProtocolRef, s: &str) -> String {
            s.to_string()
        }
    }
    let reg = ProtocolRegistry::builder()
        .register(Arc::new(MentionComponent {
            kind: MentionKind::Agent,
        }))
        .register(Arc::new(Another))
        .build();
    assert!(reg.contains("another"));
    assert!(reg.contains("agent"));
}

#[test]
fn resolved_attachment_component_extracts_and_renders() {
    // 批2 数据化：命中实名载荷 / 未命中快照兜底 / 参与通知
    let mut map = HashMap::new();
    map.insert(
        "att_1".to_string(),
        ResolvedPayload {
            name: "设计稿.png".to_string(),
            summary: Some("image/png · 125952".to_string()),
        },
    );
    let c = AttachmentComponent::with_resolved(map);
    let r = ProtocolRef {
        scheme: "attachment".into(),
        id: "att_1".into(),
        org: None,
    };
    assert_eq!(
        c.extract(&r),
        Some(ResolvedPayload {
            name: "设计稿.png".into(),
            summary: Some("image/png · 125952".into()),
        })
    );
    assert_eq!(c.render_text(&r, "快照名"), "[附件：设计稿.png]");
    assert!(c.participates_in_notify());

    // 未命中 id：快照兜底防悬空
    let miss = ProtocolRef {
        scheme: "attachment".into(),
        id: "att_x".into(),
        org: None,
    };
    assert_eq!(c.extract(&miss), None);
    assert_eq!(c.render_text(&miss, "快照名"), "[附件：快照名]");
}

#[test]
fn default_resource_components_stay_placeholder_compatible() {
    // 批1 占位行为零回归：Default 构造 extract=None / 不参与通知 / 快照 render
    let c = AttachmentComponent::default();
    let r = ProtocolRef {
        scheme: "attachment".into(),
        id: "att_1".into(),
        org: None,
    };
    assert_eq!(c.extract(&r), None);
    assert!(!c.participates_in_notify());
    assert_eq!(c.render_text(&r, "设计稿"), "[附件：设计稿]");

    let a = ArtifactComponent::default();
    let ar = ProtocolRef {
        scheme: "artifact".into(),
        id: "art_1".into(),
        org: None,
    };
    assert_eq!(a.extract(&ar), None);
    assert!(!a.participates_in_notify());
    assert_eq!(a.render_text(&ar, "报告"), "[产物：报告]");
}

#[test]
fn late_registration_overrides_placeholder() {
    // 「同 scheme 后注册者覆盖」：数据化实例覆盖 default_registry 占位
    let mut map = HashMap::new();
    map.insert(
        "att_2".to_string(),
        ResolvedPayload {
            name: "doc.pdf".to_string(),
            summary: None,
        },
    );
    let mut reg = ProtocolRegistry::default_registry();
    assert!(!reg.get("attachment").unwrap().participates_in_notify());
    reg.register(Arc::new(AttachmentComponent::with_resolved(map)));
    let c = reg.get("attachment").unwrap();
    assert!(c.participates_in_notify());
    assert_eq!(
        c.extract(&ProtocolRef {
            scheme: "attachment".into(),
            id: "att_2".into(),
            org: None,
        }),
        Some(ResolvedPayload {
            name: "doc.pdf".into(),
            summary: None,
        })
    );
}
