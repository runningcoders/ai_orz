//! `common::id` 的回归测试。

use super::*;

#[test]
fn generate_and_parse_round_trip() {
    let id = IdPrefix::KnowledgeNode.generate();
    let s = id.to_string();

    assert!(s.starts_with("kn_"), "前缀错误: {s}");
    assert!(!s.contains('-'), "UUID 不能带横杠: {s}");

    let parsed = EntityId::parse(&s).unwrap();
    assert_eq!(parsed, id);
    assert_eq!(parsed.prefix(), &IdPrefix::KnowledgeNode);
    assert_eq!(parsed.uuid().get_version_num(), 7);
}

#[test]
fn custom_prefix_round_trip() {
    let id = EntityId::generate_custom("tenant").unwrap();
    let s = id.to_string();
    assert!(s.starts_with("tenant_"), "动态前缀错误: {s}");

    let parsed = EntityId::parse(&s).unwrap();
    assert_eq!(parsed, id);
    assert_eq!(parsed.prefix(), &IdPrefix::Custom("tenant".to_string()));

    // 自定义前缀允许包含下划线，解析按最后一个下划线切分。
    let nested = EntityId::generate_custom("my_tenant").unwrap();
    assert_eq!(EntityId::parse(&nested.to_string()).unwrap(), nested);
    assert_eq!(nested.prefix(), &IdPrefix::Custom("my_tenant".to_string()));
}

#[test]
fn generate_custom_normalizes_known_prefix() {
    let id = EntityId::generate_custom("st").unwrap();
    assert_eq!(id.prefix(), &IdPrefix::ShortTerm);
    assert_eq!(EntityId::parse(&id.to_string()).unwrap(), id);

    // 直接构造 Custom("st") 也应归一化，避免生成/解析不对称。
    let id = EntityId::generate(IdPrefix::Custom("st".to_string()));
    assert_eq!(id.prefix(), &IdPrefix::ShortTerm);
    assert_eq!(EntityId::parse(&id.to_string()).unwrap(), id);
}

#[test]
#[should_panic(expected = "ID 前缀只能包含")]
fn generate_rejects_invalid_custom_prefix() {
    let _ = EntityId::generate(IdPrefix::Custom("bad-prefix".to_string()));
}

#[test]
fn hyphenated_uuid_is_normalized_to_simple() {
    let uuid = Uuid::now_v7();
    let input = format!("kn_{uuid}"); // Uuid Display 是带横杠的
    let parsed = EntityId::parse(&input).unwrap();

    assert_eq!(parsed.uuid(), uuid);
    assert_eq!(parsed.to_string(), format!("kn_{}", uuid.simple()));
}

#[test]
fn prefix_containing_underscore_is_supported() {
    let id = IdPrefix::OntologyClass.generate();
    let s = id.to_string();
    assert!(s.starts_with("ont_cls_"), "前缀错误: {s}");

    let parsed = EntityId::parse(&s).unwrap();
    assert_eq!(parsed.prefix(), &IdPrefix::OntologyClass);
}

#[test]
fn unknown_prefix_falls_back_to_custom() {
    let parsed = EntityId::parse("tenant_0192abcd1234abcd1234abcd1234abcd").unwrap();
    assert_eq!(parsed.prefix(), &IdPrefix::Custom("tenant".to_string()));
}

#[test]
fn invalid_inputs_are_rejected() {
    assert_eq!(
        EntityId::parse("kn").unwrap_err(),
        IdParseError::MissingSeparator
    );
    assert_eq!(
        EntityId::parse("_abc").unwrap_err(),
        IdParseError::EmptyPrefix
    );
    assert!(matches!(
        EntityId::parse("kn_not-a-uuid").unwrap_err(),
        IdParseError::InvalidUuid(_)
    ));
    assert!(matches!(
        EntityId::generate_custom("bad-prefix").unwrap_err(),
        IdParseError::InvalidPrefix(_)
    ));
}

#[test]
fn prefix_registry_is_unique_and_identifier_safe() {
    let unique: std::collections::HashSet<_> = IdPrefix::ALL.iter().map(|p| p.as_str()).collect();
    assert_eq!(unique.len(), IdPrefix::ALL.len(), "ID 前缀不能重复");

    for prefix in IdPrefix::ALL {
        let raw = prefix.as_str();
        assert!(!raw.is_empty(), "ID 前缀不能为空");
        assert!(
            raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "ID 前缀只能是标识符字符: {raw}"
        );
    }
}

#[test]
fn macro_generates_known_and_custom_ids() {
    let known = crate::new_id!(KnowledgeNode);
    assert!(known.starts_with("kn_"), "已知类型前缀错误: {known}");

    let custom = crate::new_id!("tenant");
    assert!(custom.starts_with("tenant_"), "动态前缀错误: {custom}");

    // 字符串命中已知前缀时归一化为已知类型。
    let known_by_string = crate::new_id!("st");
    assert!(known_by_string.starts_with("st_"));
    assert_eq!(
        EntityId::parse(&known_by_string).unwrap().prefix(),
        &IdPrefix::ShortTerm
    );

    // 动态前缀支持 String / &String，也允许尾逗号。
    let prefix = String::from("tenant");
    let from_string = crate::new_id!(&prefix);
    assert!(from_string.starts_with("tenant_"));
    let with_comma = crate::new_id!(KnowledgeNode,);
    assert!(with_comma.starts_with("kn_"));
}

#[test]
#[should_panic(expected = "ID 前缀只能包含")]
fn macro_rejects_invalid_custom_prefix() {
    let _ = crate::new_id!("bad-prefix");
}

#[test]
fn serde_uses_string_representation() {
    let id = IdPrefix::Message.generate();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{id}\""));

    let parsed: EntityId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, id);
}

#[test]
fn ordering_is_by_uuid_time_first() {
    let first = EntityId::generate(IdPrefix::Message);
    let second = EntityId::generate(IdPrefix::Message);
    assert!(first < second, "UUIDv7 应按时间递增");

    let mixed_first = EntityId::generate(IdPrefix::Task);
    let mixed_second = EntityId::generate(IdPrefix::Message);
    assert!(mixed_first < mixed_second, "混合实体也应按时间排序");
}

#[test]
fn user_id_keeps_legacy_format() {
    let id = UserId::generate();
    let s = id.to_string();

    assert_eq!(s.len(), USER_ID_LEN, "用户 ID 长度错误: {s}");
    assert!(
        s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
        "用户 ID 只能是大写字母或数字: {s}"
    );
    assert_eq!(UserId::parse(&s).unwrap(), id);
    assert_eq!(id.as_str(), s);

    let unique: std::collections::HashSet<_> = (0..1000).map(|_| UserId::generate()).collect();
    assert_eq!(unique.len(), 1000, "用户 ID 不应重复");
}

#[test]
fn user_id_rejects_legacy_or_malformed_values() {
    assert!(matches!(
        UserId::parse("abcdefghijklmnop").unwrap_err(),
        IdParseError::InvalidUserId(_)
    ));
    assert!(matches!(
        UserId::parse("short").unwrap_err(),
        IdParseError::InvalidUserId(_)
    ));
    // 历史 UUID 形式用户 ID 仍按 String 处理，不属于本类型的规范格式。
    assert!(matches!(
        UserId::parse("0192abcd-1234-7abc-8def-0123456789ab").unwrap_err(),
        IdParseError::InvalidUserId(_)
    ));
}

#[test]
fn org_id_keeps_legacy_format() {
    let id = OrgId::generate();
    let s = id.to_string();

    assert_eq!(s.len(), ORG_ID_LEN, "组织 ID 长度错误: {s}");
    assert!(
        s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
        "组织 ID 只能是大写字母或数字: {s}"
    );
    assert_eq!(OrgId::parse(&s).unwrap(), id);
    assert!(matches!(
        OrgId::parse("ABCDEFGHIJK").unwrap_err(),
        IdParseError::InvalidOrgId(_)
    ));
}

#[test]
fn uuid_ids_use_simple_format_and_normalize_hyphenated() {
    let contract = UuidId::generate();
    let s = contract.to_string();
    assert_eq!(s.len(), 32, "UuidId 应为 32 位无横杠 UUID: {s}");
    assert!(!s.contains('-'));
    assert_eq!(contract.uuid().get_version_num(), 7);
    assert_eq!(UuidId::parse(&s).unwrap(), contract);

    // 带横杠的旧值解析后统一规范化为无横杠。
    let hyphenated = contract.uuid().to_string();
    assert_eq!(UuidId::parse(&hyphenated).unwrap().to_string(), s);

    // 外部协议需要标准文本时显式取带横杠形式，内部默认仍是无横杠。
    let external = contract.to_hyphenated();
    assert_eq!(external.len(), 36);
    assert!(external.contains('-'));
    assert_eq!(UuidId::parse(&external).unwrap(), contract);
    assert!(matches!(
        UuidId::parse("not-a-uuid").unwrap_err(),
        IdParseError::InvalidUuid(_)
    ));
}

#[test]
fn time_id_starts_with_local_time_and_is_indexable() {
    let id = TimeId::generate();
    let s = id.to_string();

    assert_eq!(s.len(), 32, "TimeId 长度应为 32: {s}");
    assert!(s.bytes().all(|b| b.is_ascii_alphanumeric()));
    assert!(!s.contains('-'), "TimeId 不能带分隔符: {s}");

    let time = id.time_part();
    assert_eq!(time.len(), 14);
    assert!(time.bytes().all(|b| b.is_ascii_digit()));

    let dt = id.local_datetime().expect("时间前缀应可解析");
    assert_eq!(dt.format("%Y%m%d%H%M%S").to_string(), time);
    assert_eq!(id.date_part(), &time[..8]);

    assert_eq!(TimeId::parse(&s).unwrap(), id);
}

#[test]
fn time_id_rejects_invalid_values() {
    // 13 月：时间前缀非法。
    assert!(matches!(
        TimeId::parse("20261301000000abcdef0123456789ab").unwrap_err(),
        IdParseError::InvalidTimeId(_)
    ));
    // 长度不足。
    assert!(matches!(
        TimeId::parse("2026111715304").unwrap_err(),
        IdParseError::InvalidTimeId(_)
    ));
    // 后缀含大写。
    assert!(matches!(
        TimeId::parse("20261117153045ABCDEF0123456789AB").unwrap_err(),
        IdParseError::InvalidTimeId(_)
    ));
    // 后缀含非十六进制字符。
    assert!(matches!(
        TimeId::parse("20261117153045abcdefgh0123456789").unwrap_err(),
        IdParseError::InvalidTimeId(_)
    ));
    // 非 ASCII 输入不能触发按字节切分的 panic（é 跨第 14 字节）。
    let non_ascii = format!("2026111715304é{}", "a".repeat(17));
    assert!(matches!(
        TimeId::parse(&non_ascii).unwrap_err(),
        IdParseError::InvalidTimeId(_)
    ));
}

#[test]
fn time_ids_are_unique_within_same_second() {
    let unique: std::collections::HashSet<_> = (0..1000).map(|_| TimeId::generate()).collect();
    assert_eq!(unique.len(), 1000, "同一秒内 TimeId 不应重复");
}

#[test]
fn a2a_task_id_generates_uuid_but_accepts_opaque_remote_ids() {
    let local = A2aTaskId::generate();
    assert_eq!(local.as_str().len(), 32);
    assert!(!local.as_str().contains('-'));

    let remote = A2aTaskId::parse("remote-task-1").unwrap();
    assert_eq!(remote.as_str(), "remote-task-1");
    assert_eq!(remote.to_string(), "remote-task-1");

    assert_eq!(A2aTaskId::parse("   ").unwrap_err(), IdParseError::EmptyId);
}

#[test]
fn external_id_preserves_raw_value() {
    let raw = "ou_abcDEF123_456";
    let id = ExternalId::parse(raw).unwrap();

    assert_eq!(id.as_str(), raw);
    assert_eq!(id.to_string(), raw);
    assert_eq!(ExternalId::parse("").unwrap_err(), IdParseError::EmptyId);
    assert_eq!(ExternalId::parse(" \t").unwrap_err(), IdParseError::EmptyId);
}

#[test]
fn all_id_types_implement_id_value() {
    fn render<T: IdValue>(value: T) -> String {
        value.to_string()
    }

    assert!(render(IdPrefix::Message.generate()).starts_with("msg_"));
    assert_eq!(render(UserId::generate()).len(), USER_ID_LEN);
    assert_eq!(render(OrgId::generate()).len(), ORG_ID_LEN);
    assert_eq!(render(UuidId::generate()).len(), 32);
    assert_eq!(render(TimeId::generate()).len(), 32);
    assert_eq!(render(A2aTaskId::generate()).len(), 32);
    assert_eq!(render(ExternalId::parse("open-id").unwrap()), "open-id");
}

#[test]
fn new_id_types_serde_uses_string_representation() {
    let id = OrgId::generate();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{id}\""));

    let parsed: OrgId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, id);

    let invalid: Result<UserId, _> = serde_json::from_str("\"lowercase-value\"");
    assert!(invalid.is_err(), "非法用户 ID 不能在反序列化时通过");
}
