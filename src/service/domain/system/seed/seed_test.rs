//! Seed 模块单元测试（纯函数测试，不需要 DB）

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use crate::service::domain::system::seed::defs::*;
    use crate::service::domain::system::seed::diff::*;

    fn make_test_snapshot(name: &str) -> SeedSnapshot {
        SeedSnapshot {
            version: SeedSnapshot::CURRENT_VERSION.to_string(),
            generated_at: 1000,
            description: None,
            source_organization_id: "ORG1".to_string(),
            organization: OrganizationDef {
                id: "ORG1".to_string(),
                name: name.to_string(),
                description: String::new(),
                base_url: String::new(),
                status: 1,
                scope: 0,
            },
            users: vec![UserDef {
                id: "U1".to_string(),
                organization_id: "ORG1".to_string(),
                username: "admin".to_string(),
                display_name: "Admin".to_string(),
                email: String::new(),
                password_ref: PENDING_INPUT.to_string(),
                role: 0,
                status: 1,
            }],
            model_providers: vec![],
            agents: vec![],
            skills: vec![],
        }
    }

    #[test]
    fn test_diff_snapshots_detects_updated_field() {
        let base = make_test_snapshot("旧名称");
        let mut target = base.clone();
        target.organization.name = "新名称".to_string();

        let diff = diff_snapshots(&base, &target);
        assert_eq!(diff.summary.updated_count, 1);
        assert!(matches!(diff.organization, Some(DiffEntry::Updated { .. })));
    }

    #[test]
    fn test_diff_snapshots_detects_same() {
        let base = make_test_snapshot("name");
        let target = base.clone();
        let diff = diff_snapshots(&base, &target);
        assert_eq!(diff.summary.same_count, 2); // org + user
        assert_eq!(diff.summary.updated_count, 0);
    }

    #[test]
    fn test_diff_snapshots_detects_new_and_removed() {
        let base = make_test_snapshot("name");
        let mut target = base.clone();
        target.users.clear(); // remove user
        target.users.push(UserDef {
            id: "U2".to_string(),
            organization_id: "ORG1".to_string(),
            username: "new_user".to_string(),
            display_name: "New".to_string(),
            email: String::new(),
            password_ref: PENDING_INPUT.to_string(),
            role: 2,
            status: 1,
        });

        let diff = diff_snapshots(&base, &target);
        assert_eq!(diff.summary.new_count, 1);
        assert_eq!(diff.summary.removed_count, 1);
    }

    #[test]
    fn test_validate_sensitive_fields_success() {
        let snapshot = make_test_snapshot("name");
        let mut sensitive = HashMap::new();
        sensitive.insert("user:U1:password".to_string(), "hashed_pwd".to_string());
        assert!(validate_sensitive_fields(&snapshot, &sensitive).is_ok());
    }

    #[test]
    fn test_validate_sensitive_fields_missing() {
        let snapshot = make_test_snapshot("name");
        let sensitive = HashMap::new();
        assert!(validate_sensitive_fields(&snapshot, &sensitive).is_err());
    }

    #[test]
    fn test_resolve_password_pending_input() {
        let mut sensitive = HashMap::new();
        sensitive.insert("user:U1:password".to_string(), "new_hash".to_string());
        let result = resolve_password(PENDING_INPUT, "U1", &sensitive, None).unwrap();
        assert_eq!(result, "new_hash");
    }

    #[test]
    fn test_resolve_password_inherit_current() {
        let sensitive = HashMap::new();
        let result = resolve_password(INHERIT_CURRENT, "U1", &sensitive, Some("current_hash")).unwrap();
        assert_eq!(result, "current_hash");
    }

    #[test]
    fn test_resolve_password_inherit_current_missing_current_value() {
        let sensitive = HashMap::new();
        // INHERIT_CURRENT 但 current_password_hash 为 None → 报错
        assert!(resolve_password(INHERIT_CURRENT, "U1", &sensitive, None).is_err());
    }

    #[test]
    fn test_resolve_password_random_generate_returns_non_empty() {
        let sensitive = HashMap::new();
        let result = resolve_password(RANDOM_GENERATE, "U1", &sensitive, None).unwrap();
        assert!(!result.is_empty());
    }
}
