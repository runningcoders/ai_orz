//! tests 单元测试（拆分自 resource_resolver.rs）
//!
//! 文件瘦身：原 647 行 → 441 行，测试体 207 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

#[test]
fn skipped_notice_is_empty_without_oversized_attachments() {
    // 没有跳过项 → 正文零改动（存量路径必须逐字节不变）
    assert_eq!(skipped_attachment_notice(&[]), "");
}

#[test]
fn skipped_notice_names_file_and_both_sizes() {
    let notice = skipped_attachment_notice(&[SkippedAttachment {
        name: "季度报告.pdf".to_string(),
        size: 45 * 1024 * 1024,
        limit: 30 * 1024 * 1024,
        reason: SkippedAttachmentReason::OverLimit,
    }]);
    assert!(
        notice.starts_with("\n\n"),
        "追加在正文尾部，需自带换行: {notice}"
    );
    assert!(notice.contains("季度报告.pdf"));
    assert!(notice.contains("45.0 MB"), "要给出实际大小: {notice}");
    assert!(
        notice.contains("渠道上限 30.0 MB"),
        "要给出卡在哪个上限: {notice}"
    );
    assert!(notice.contains("未随本条消息发送"));
}

#[test]
fn skipped_notice_joins_multiple_items() {
    let notice = skipped_attachment_notice(&[
        SkippedAttachment {
            name: "a.zip".to_string(),
            size: 40 * 1024 * 1024,
            limit: 30 * 1024 * 1024,
            reason: SkippedAttachmentReason::OverLimit,
        },
        SkippedAttachment {
            name: "b.png".to_string(),
            size: 12 * 1024 * 1024,
            limit: 10 * 1024 * 1024,
            reason: SkippedAttachmentReason::OverLimit,
        },
    ]);
    assert!(notice.contains("a.zip"));
    assert!(notice.contains("b.png"));
    assert_eq!(notice.matches("；").count(), 1, "多项用分号分隔: {notice}");
}

#[test]
fn skipped_notice_distinguishes_read_failure_from_over_limit() {
    let notice = skipped_attachment_notice(&[SkippedAttachment {
        name: "丢失.pdf".to_string(),
        size: 1024,
        limit: 30 * 1024 * 1024,
        reason: SkippedAttachmentReason::ReadFailed,
    }]);
    assert!(notice.contains("读取失败"), "{notice}");
    assert!(notice.contains("丢失.pdf"), "{notice}");
    assert!(
        !notice.contains("渠道上限"),
        "读取失败与超限文案要分开: {notice}"
    );
}

#[test]
fn outbound_oversize_prefilters_by_declared_and_rechecks_actual() {
    let limit = 10;
    // 声明就超限：不读字节（actual=None）也要跳过
    assert_eq!(outbound_oversize(11, None, limit), Some(11));
    // 声明没超、实测超：以实测值提示（meta 滞后 / 文件变化）
    assert_eq!(outbound_oversize(8, Some(12), limit), Some(12));
    // 两边都合法：放行
    assert_eq!(outbound_oversize(8, Some(9), limit), None);
    assert_eq!(outbound_oversize(10, Some(10), limit), None);
    // 尚未读取且声明合法 → 继续读取
    assert_eq!(outbound_oversize(8, None, limit), None);
}

#[test]
fn human_size_switches_units_at_boundaries() {
    assert_eq!(human_size(512), "512 B");
    assert_eq!(human_size(2048), "2 KB");
    assert_eq!(human_size(10 * 1024 * 1024), "10.0 MB");
}

#[test]
fn outbound_image_refs_keep_order_and_dedupe() {
    // 逐张发送要求顺序稳定：按正文首次出现，重复只发一张；artifact 不混入
    let text = "[A](attachment:att_b) [B](attachment:att_a) [A2](attachment:att_b) \
                [R](artifact:art_1) [link](https://e.com)";
    assert_eq!(
        collect_attachment_ids_in_order(text),
        vec!["att_b".to_string(), "att_a".to_string()]
    );
    assert!(collect_attachment_ids_in_order("无附件正文").is_empty());
}

#[test]
fn outbound_image_mime_accepts_only_image_scheme() {
    // 统一判定源在 models（outbound_media 有完整四分类覆盖），
    // 这里仅回归 domain 侧依赖的口径：大小写不敏感 + 拒收平台不支持的图片格式。
    let is_image = |mime: &str| OutboundMediaKind::from_mime(mime) == OutboundMediaKind::Image;
    assert!(is_image("image/png"));
    assert!(is_image("image/webp"));
    // MIME 大小写不敏感：normalize_or_infer 对非泛型值原样返回，大写不能漏发
    assert!(is_image("IMAGE/PNG"));
    assert!(is_image("Image/Jpeg"));
    assert!(!is_image("application/pdf"));
    assert!(!is_image("application/octet-stream"));
    // 平台多半拒收的格式在入口就排除
    assert!(!is_image("image/svg+xml"));
    assert!(!is_image("image/heic"));
}

#[test]
fn collect_ids_scopes_to_resource_schemes() {
    let text = "看 [@张伟](agent:agt_1) 的 [设计稿](attachment:att_1) 与 [报告](artifact:art_2)，\
                未注册 [x](user:u1) 与普通 [链接](https://e.com) 忽略";
    let (att, art) = collect_resource_ids(text);
    assert_eq!(att, HashSet::from(["att_1".to_string()]));
    assert_eq!(art, HashSet::from(["art_2".to_string()]));
}

#[test]
fn degrade_passthrough_without_registered_refs() {
    // 幂等：无已注册协议引用 → 原文零改动
    let reg = ProtocolRegistry::default_registry();
    let src = "# 标题\n\n普通段落，含 [链接](https://e.com) 与 [x](user:u1)。\n";
    assert_eq!(degrade_content_for_external(src, &reg), src);
}

#[test]
fn degrade_replaces_resolved_and_falls_back_on_miss() {
    let mut att = HashMap::new();
    att.insert(
        "att_1".to_string(),
        ResolvedPayload {
            name: "设计稿.png".to_string(),
            summary: Some("image/png · 1".to_string()),
        },
    );
    let reg = build_resolved_registry(att, HashMap::new());
    let out = degrade_content_for_external(
        "请查收 [设计稿](attachment:att_1)，另有 [丢失](attachment:att_x)。",
        &reg,
    );
    assert!(out.contains("[附件：设计稿.png]"), "命中实名：{out}");
    assert!(out.contains("[附件：丢失]"), "未命中快照兜底：{out}");
    assert!(!out.contains("attachment:"));
}

#[test]
fn degrade_resolves_batch1_semantic_boundary_nested_emphasis() {
    // 批1 登记语义边界：结构化内层（加粗）链接——批2 组件 render 接管后消解
    let reg = ProtocolRegistry::default_registry();
    let out = degrade_content_for_external("[**张伟**](attachment:att_1) 看看", &reg);
    assert!(out.contains("[附件：张伟]"), "消解成立：{out}");
    assert!(!out.contains("**"));
}

#[test]
fn degrade_mention_renders_via_component() {
    let reg = ProtocolRegistry::default_registry();
    let out = degrade_content_for_external("[@张伟](agent:agt_1) 进度如何", &reg);
    assert!(out.contains("@张伟"));
    assert!(!out.contains("agent:"));
}

#[test]
fn artifact_summary_truncates_long_description() {
    let long = "很".repeat(100);
    let s = artifact_summary(FileType::Document, &long).unwrap();
    assert!(s.starts_with("文档 · "));
    assert!(s.ends_with('…'));
    assert!(s.chars().count() < 100, "截断生效：{s}");

    // 空 description：仅文件类型标签
    assert_eq!(artifact_summary(FileType::Image, "  ").unwrap(), "图片");
}

#[test]
fn has_resource_refs_true_when_resource_ref_present() {
    assert!(content_has_resource_refs("看 [设计稿](attachment:att_1)"));
    assert!(content_has_resource_refs("报告见 [报告](artifact:art_2)"));
    assert!(content_has_resource_refs(
        "混排 [@张伟](agent:agt_1) + [附件](attachment:a1)"
    ));
}

#[test]
fn has_resource_refs_false_for_mention_and_plain_text() {
    assert!(!content_has_resource_refs(
        "[@张伟](agent:agt_1) 请跟进 [@任务](task:t_9)"
    ));
    assert!(!content_has_resource_refs("普通文本，无任何协议引用"));
}

#[test]
fn has_resource_refs_false_for_unregistered_or_malformed() {
    assert!(!content_has_resource_refs("普通 [链接](https://e.com)"));
    assert!(!content_has_resource_refs("[x](user:u1) 未注册不算混排"));
    assert!(!content_has_resource_refs(
        "歪格式 [x](attachment) 缺 id 段"
    ));
}
