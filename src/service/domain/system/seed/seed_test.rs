//! Seed 模块单元测试（纯函数测试，不需要 DB）

#[cfg(test)]
mod tests {
    use crate::service::domain::system::seed::defs::*;
    use crate::service::domain::system::seed::diff::*;
    use std::collections::HashMap;

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
        let result =
            resolve_password(INHERIT_CURRENT, "U1", &sensitive, Some("current_hash")).unwrap();
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

    #[tokio::test]
    async fn test_store_write_read_delete_round_trip() {
        let dir = std::env::temp_dir().join("ai_orz_seed_store_test");
        let _ = std::fs::remove_dir_all(&dir);

        let name = "test-snapshot";
        let content = r#"{"version": "1.0.0"}"#;

        let size = crate::service::domain::system::seed::store::write_file(&dir, name, content)
            .await
            .unwrap();
        assert_eq!(size, content.len() as u64);

        let resp = crate::service::domain::system::seed::store::read_file(&dir, name)
            .await
            .unwrap();
        assert_eq!(resp.content, content);
        assert_eq!(resp.name, "test-snapshot.json");

        let files = crate::service::domain::system::seed::store::list_files(&dir)
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "test-snapshot.json");

        crate::service::domain::system::seed::store::delete_file(&dir, name)
            .await
            .unwrap();
        let files = crate::service::domain::system::seed::store::list_files(&dir)
            .await
            .unwrap();
        assert_eq!(files.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_seed_filename_rejects_path_traversal() {
        assert!(
            crate::service::domain::system::seed::store::validate_seed_filename(
                "../../../etc/passwd"
            )
            .is_err()
        );
        assert!(
            crate::service::domain::system::seed::store::validate_seed_filename("a/b").is_err()
        );
        assert!(crate::service::domain::system::seed::store::validate_seed_filename("").is_err());
        assert!(
            crate::service::domain::system::seed::store::validate_seed_filename("..secret")
                .is_err()
        );
    }

    #[test]
    fn test_validate_seed_filename_appends_json_extension() {
        let name = crate::service::domain::system::seed::store::validate_seed_filename("snapshot")
            .unwrap();
        assert_eq!(name, "snapshot.json");

        let name =
            crate::service::domain::system::seed::store::validate_seed_filename("snapshot.json")
                .unwrap();
        assert_eq!(name, "snapshot.json");
    }

    #[test]
    fn test_default_snapshot_parses_successfully() {
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        assert_eq!(snapshot.version, "1.0.0");
        assert_eq!(snapshot.users.len(), 1);
        assert_eq!(snapshot.model_providers.len(), 2);
        assert_eq!(snapshot.agents.len(), 1);
        // 预置 5 个技能（4 个 neural + 1 个 project_management）
        assert_eq!(snapshot.skills.len(), 5);
        assert_eq!(
            snapshot.agents[0].model_provider_id,
            "TEMPLATE_CHAT_PROVIDER"
        );
        assert_eq!(
            snapshot.users[0].password_ref,
            super::super::defs::PENDING_INPUT
        );
    }

    #[test]
    fn test_default_snapshot_preset_skills() {
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();

        let ids: Vec<&str> = snapshot.skills.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"TEMPLATE_TOOL_MANAGEMENT"));
        assert!(ids.contains(&"TEMPLATE_SKILL_MANAGEMENT"));
        assert!(ids.contains(&"TEMPLATE_MEMORY_COGNITION"));
        assert!(ids.contains(&"TEMPLATE_COMMUNICATION"));
        assert!(ids.contains(&"TEMPLATE_PROJECT_MANAGEMENT"));

        // 前 4 个神经技能必须包含 neural tag
        let neural_ids = [
            "TEMPLATE_TOOL_MANAGEMENT",
            "TEMPLATE_SKILL_MANAGEMENT",
            "TEMPLATE_MEMORY_COGNITION",
            "TEMPLATE_COMMUNICATION",
        ];
        for skill in &snapshot.skills {
            if neural_ids.contains(&skill.id.as_str()) {
                assert!(
                    skill.tags.contains(&"neural".to_string()),
                    "神经技能 {} 必须包含 neural tag",
                    skill.id
                );
            }
            assert_eq!(skill.category, "system");
            assert_eq!(skill.status, 1); // Published
            assert_eq!(skill.author_id, "TEMPLATE_ADMIN");
            assert_eq!(skill.author_type, 0); // User
            assert!(!skill.files.is_empty(), "预置技能必须有 files");
            assert_eq!(skill.files[0].path, "skill.md");
            assert!(skill.files[0].ref_path.is_some());
        }
    }

    // ===== SkillFileDef 三来源测试（Task 1） =====

    #[test]
    fn test_skill_file_def_content_source() {
        let file = SkillFileDef {
            path: "skill.md".to_string(),
            content: Some("# 内容".to_string()),
            ref_path: None,
            url: None,
        };
        let json = serde_json::to_string(&file).unwrap();
        let de: SkillFileDef = serde_json::from_str(&json).unwrap();
        assert_eq!(de.content.as_ref().unwrap(), "# 内容");
        assert!(de.ref_path.is_none());
        assert!(de.url.is_none());
    }

    #[test]
    fn test_skill_file_def_ref_path_source() {
        let file = SkillFileDef {
            path: "skill.md".to_string(),
            content: None,
            ref_path: Some("skills/platform_guide/skill.md".to_string()),
            url: None,
        };
        let json = serde_json::to_string(&file).unwrap();
        let de: SkillFileDef = serde_json::from_str(&json).unwrap();
        assert_eq!(
            de.ref_path.as_ref().unwrap(),
            "skills/platform_guide/skill.md"
        );
    }

    #[test]
    fn test_skill_file_def_url_source() {
        let file = SkillFileDef {
            path: "skill.md".to_string(),
            content: None,
            ref_path: None,
            url: Some("https://example.com/guide.md".to_string()),
        };
        let json = serde_json::to_string(&file).unwrap();
        let de: SkillFileDef = serde_json::from_str(&json).unwrap();
        assert_eq!(de.url.as_ref().unwrap(), "https://example.com/guide.md");
    }

    #[test]
    fn test_skill_def_with_files_roundtrip() {
        let skill = SkillDef {
            id: "test_skill".to_string(),
            name: "测试技能".to_string(),
            description: "用于测试".to_string(),
            tags: vec!["neural".to_string()],
            category: "system".to_string(),
            parent_skill_id: String::new(),
            author_id: "TEMPLATE_ADMIN".to_string(),
            author_type: 0,
            status: 1,
            content_path: "skills/test_skill".to_string(),
            files: vec![SkillFileDef {
                path: "skill.md".to_string(),
                content: Some("# 测试".to_string()),
                ref_path: None,
                url: None,
            }],
        };
        let json = serde_json::to_string(&skill).unwrap();
        let de: SkillDef = serde_json::from_str(&json).unwrap();
        assert_eq!(de.files.len(), 1);
    }

    #[test]
    fn test_skill_def_backward_compat_no_files() {
        let json = r#"{
            "id": "old_skill", "name": "旧", "description": "",
            "tags": [], "category": "x", "parent_skill_id": "",
            "author_id": "u", "author_type": 0, "status": 1,
            "content_path": "skills/old"
        }"#;
        let skill: SkillDef = serde_json::from_str(json).unwrap();
        assert!(skill.files.is_empty());
    }
}
