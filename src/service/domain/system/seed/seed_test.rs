//! Seed 模块单元测试（纯函数测试，不需要 DB）

#[cfg(test)]
mod tests {
    use crate::service::domain::system::seed::defs::*;
    use crate::service::domain::system::seed::diff::*;
    use common::ontology::{
        PresetOntologyClass, PresetOntologyLexicon, PresetOntologyRelationType,
        PresetOntologySynonym, TermKind,
    };
    use std::collections::{HashMap, HashSet};

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
                config: None,
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
            // 空词表：diff_ontology 两侧均空返回 None，不影响既有 diff 计数断言
            ontology: PresetOntologyLexicon::default(),
        }
    }

    /// 构造最小词表（1 节点类 + 1 关系词），供 ontology diff 测试
    fn make_test_lexicon() -> PresetOntologyLexicon {
        PresetOntologyLexicon {
            classes: vec![PresetOntologyClass {
                term_key: "concept".to_string(),
                display_name: "概念".to_string(),
                description: String::new(),
                required_fields: vec![],
            }],
            relation_types: vec![PresetOntologyRelationType {
                term_key: "related".to_string(),
                display_name: "相关".to_string(),
                description: String::new(),
                domain_classes: vec![],
                range_classes: vec![],
                weight_base: Some(1.0),
                inverse_key: None,
            }],
            synonym_mappings: vec![],
        }
    }

    /// 构造 model_provider 定义（capability: 0=对话, 1=向量）
    fn make_provider(id: &str, capability: i32, config: &str) -> ModelProviderDef {
        ModelProviderDef {
            id: id.to_string(),
            name: format!("provider-{}", id),
            provider_type: 0,
            model_name: "gpt-4o".to_string(),
            capability,
            api_key_ref: PENDING_INPUT.to_string(),
            base_url: None,
            description: None,
            config: config.to_string(),
            status: 1,
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
        assert!(
            validate_sensitive_fields(&snapshot, &sensitive, &HashSet::new(), &HashSet::new())
                .is_ok()
        );
    }

    #[test]
    fn test_validate_sensitive_fields_missing() {
        let snapshot = make_test_snapshot("name");
        let sensitive = HashMap::new();
        assert!(
            validate_sensitive_fields(&snapshot, &sensitive, &HashSet::new(), &HashSet::new())
                .is_err()
        );
    }

    #[test]
    fn test_validate_sensitive_fields_skips_existing_user() {
        // 用户已存在于目标环境 → 可留空，写入时沿用当前密码
        let snapshot = make_test_snapshot("name");
        let sensitive = HashMap::new();
        let existing: HashSet<String> = ["U1".to_string()].into_iter().collect();
        assert!(
            validate_sensitive_fields(&snapshot, &sensitive, &existing, &HashSet::new()).is_ok()
        );
    }

    #[test]
    fn test_validate_provider_context_length_rejects_chat_without_threshold() {
        let mut snapshot = make_test_snapshot("name");
        snapshot.model_providers.push(make_provider("P1", 0, "{}"));
        assert!(validate_provider_context_length(&snapshot).is_err());
    }

    #[test]
    fn test_validate_provider_context_length_accepts_chat_with_threshold() {
        let mut snapshot = make_test_snapshot("name");
        snapshot
            .model_providers
            .push(make_provider("P1", 0, "{\"max_context_length\":128000}"));
        assert!(validate_provider_context_length(&snapshot).is_ok());
    }

    #[test]
    fn test_validate_provider_context_length_exempts_embedding() {
        let mut snapshot = make_test_snapshot("name");
        snapshot.model_providers.push(make_provider("P1", 1, "{}"));
        assert!(validate_provider_context_length(&snapshot).is_ok());
    }

    #[test]
    fn test_resolve_password_pending_input() {
        let mut sensitive = HashMap::new();
        sensitive.insert("user:U1:password".to_string(), "new_hash".to_string());
        let result = resolve_password(PENDING_INPUT, "U1", &sensitive, None).unwrap();
        assert_eq!(result, "new_hash");
    }

    #[test]
    fn test_resolve_password_pending_input_falls_back_to_current() {
        // 未补填但用户已存在 → 沿用当前密码哈希
        let sensitive = HashMap::new();
        let result =
            resolve_password(PENDING_INPUT, "U1", &sensitive, Some("current_hash")).unwrap();
        assert_eq!(result, "current_hash");
    }

    #[test]
    fn test_resolve_password_pending_input_without_current_errors() {
        let sensitive = HashMap::new();
        assert!(resolve_password(PENDING_INPUT, "U1", &sensitive, None).is_err());
    }

    #[test]
    fn test_resolve_api_key_pending_input_falls_back_to_current() {
        let sensitive = HashMap::new();
        let result = resolve_api_key(PENDING_INPUT, "P1", &sensitive, Some("sk-current")).unwrap();
        assert_eq!(result, "sk-current");
    }

    #[test]
    fn test_resolve_api_key_pending_input_without_current_errors() {
        let sensitive = HashMap::new();
        assert!(resolve_api_key(PENDING_INPUT, "P1", &sensitive, None).is_err());
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
        assert_eq!(snapshot.agents.len(), 2);
        // 预置 10 个技能（6 个 neural + 1 个 project_management + 1 个 git_branch_workflow
        // + 1 个 agent_recruitment + 1 个 user_reception）
        assert_eq!(snapshot.skills.len(), 10);
        assert_eq!(
            snapshot.agents[0].model_provider_id,
            "TEMPLATE_CHAT_PROVIDER"
        );
        // 预置招聘官：角色 hr_specialist 是它的技能匹配键，勿随意改动
        let hr = snapshot
            .agents
            .iter()
            .find(|a| a.id == "TEMPLATE_HR_AGENT")
            .expect("预置招聘官 Agent 缺失");
        assert_eq!(hr.roles, vec!["hr_specialist".to_string()]);
        assert_eq!(hr.model_provider_id, "TEMPLATE_CHAT_PROVIDER");
        assert_eq!(
            snapshot.users[0].password_ref,
            super::super::defs::PENDING_INPUT
        );

        // 组织默认要求（SSOT = seed 的 organization.config）：初始化时按此写入
        // organizations.config，project_management 是「同名双重身份」包，两侧都要配
        let onboard = snapshot
            .organization
            .config
            .expect("default.json 必须带 organization.config")
            .agent_onboard;
        assert_eq!(onboard.required_tool_packs, vec!["project_management"]);
        assert_eq!(onboard.required_skill_packs, vec!["project_management"]);
    }

    #[test]
    fn test_organization_def_backward_compat_without_config() {
        // 老快照没有 organization.config → config 为 None，不得反序列化失败
        let json = r#"{
            "id": "ORG1", "name": "组织", "description": "",
            "base_url": "", "status": 1, "scope": 0
        }"#;
        let org: OrganizationDef = serde_json::from_str(json).unwrap();
        assert!(org.config.is_none());
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

        // 全部 6 个神经技能必须包含 neural tag（含自我进化 / 项目上下文两个新技能）
        let neural_ids = [
            "TEMPLATE_TOOL_MANAGEMENT",
            "TEMPLATE_SKILL_MANAGEMENT",
            "TEMPLATE_MEMORY_COGNITION",
            "TEMPLATE_COMMUNICATION",
            "TEMPLATE_SELF_EVOLUTION",
            "TEMPLATE_PROJECT_CONTEXT_COGNITION",
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
            local_path: None,
            ref_path: None,
            url: None,
        };
        let json = serde_json::to_string(&file).unwrap();
        let de: SkillFileDef = serde_json::from_str(&json).unwrap();
        assert_eq!(de.content.as_ref().unwrap(), "# 内容");
        assert!(de.local_path.is_none());
        assert!(de.ref_path.is_none());
        assert!(de.url.is_none());
    }

    #[test]
    fn test_skill_file_def_ref_path_source() {
        let file = SkillFileDef {
            path: "skill.md".to_string(),
            content: None,
            local_path: None,
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
            local_path: None,
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
                local_path: None,
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

    // ===== 本体词表段（Task P3-1）=====

    #[test]
    fn test_diff_ontology_both_empty_produces_no_entry() {
        // 两侧词表均为空（老快照互比）→ 不产生条目、不计数
        let base = make_test_snapshot("name");
        let target = base.clone();
        let diff = diff_snapshots(&base, &target);
        assert!(diff.ontology.is_none());
        assert_eq!(diff.summary.same_count, 2); // 仅 org + user
    }

    #[test]
    fn test_diff_ontology_detects_new_when_base_empty() {
        // 老快照（无词表）对比新快照（有词表）→ 整体 New
        let base = make_test_snapshot("name");
        let mut target = base.clone();
        target.ontology = make_test_lexicon();
        let diff = diff_snapshots(&base, &target);
        assert!(matches!(diff.ontology, Some(DiffEntry::New { .. })));
        assert_eq!(diff.summary.new_count, 1);
    }

    #[test]
    fn test_diff_ontology_detects_updated_on_field_change() {
        let mut base = make_test_snapshot("name");
        base.ontology = make_test_lexicon();
        let mut target = base.clone();
        target.ontology.classes[0].display_name = "改名".to_string();
        let diff = diff_snapshots(&base, &target);
        assert!(matches!(diff.ontology, Some(DiffEntry::Updated { .. })));
        assert_eq!(diff.summary.updated_count, 1);
    }

    #[test]
    fn test_diff_ontology_same_lexicons_counted_once() {
        let mut snapshot = make_test_snapshot("name");
        snapshot.ontology = make_test_lexicon();
        let target = snapshot.clone();
        let diff = diff_snapshots(&snapshot, &target);
        assert!(matches!(diff.ontology, Some(DiffEntry::Same { .. })));
        assert_eq!(diff.summary.same_count, 3); // org + user + ontology
    }

    #[test]
    fn test_seed_snapshot_backward_compat_without_ontology() {
        // 老快照无 ontology 段 → serde default 空词表，不得反序列化失败
        let json = r#"{
            "version": "1.0.0",
            "generated_at": 1000,
            "source_organization_id": "ORG1",
            "organization": {"id": "ORG1", "name": "组织", "description": "", "base_url": "", "status": 1, "scope": 0},
            "users": [], "model_providers": [], "agents": [], "skills": []
        }"#;
        let snapshot: SeedSnapshot = serde_json::from_str(json).unwrap();
        assert!(snapshot.ontology.is_empty());
    }

    #[test]
    fn test_default_snapshot_ontology_lexicon() {
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        let lex = &snapshot.ontology;
        // 4 认知类 + 8 领域类（AI Orz 自身实体建模）= 12 节点类、21 规范关系词、3 预置同义映射
        assert_eq!(lex.classes.len(), 12);
        assert_eq!(lex.relation_types.len(), 21);
        assert_eq!(lex.synonym_mappings.len(), 3);

        let class_keys: Vec<&str> = lex.classes.iter().map(|c| c.term_key.as_str()).collect();
        for key in [
            "concept",
            "event",
            "preference",
            "skill",
            "organization",
            "user",
            "agent",
            "model",
            "memory",
            "task",
            "project",
            "tool",
        ] {
            assert!(class_keys.contains(&key), "节点类 {} 缺失", key);
        }

        // 5 组 inverse 对偶必须双向互引且自洽
        let by_key: HashMap<&str, &PresetOntologyRelationType> = lex
            .relation_types
            .iter()
            .map(|r| (r.term_key.as_str(), r))
            .collect();
        assert_eq!(by_key.len(), 21);
        for (fwd_key, rev_key) in [
            ("contains", "contained_by"),
            ("depends", "depended_by"),
            ("causes", "caused_by"),
            ("owns", "owned_by"),
            ("uses", "used_by"),
        ] {
            let fwd = by_key
                .get(fwd_key)
                .unwrap_or_else(|| panic!("关系词 {} 缺失", fwd_key));
            let rev = by_key
                .get(rev_key)
                .unwrap_or_else(|| panic!("关系词 {} 缺失", rev_key));
            assert_eq!(
                fwd.inverse_key.as_deref(),
                Some(rev_key),
                "{} 的 inverse_key 应指向 {}",
                fwd_key,
                rev_key
            );
            assert_eq!(
                rev.inverse_key.as_deref(),
                Some(fwd_key),
                "{} 的 inverse_key 应指向 {}",
                rev_key,
                fwd_key
            );
        }

        // 预置同义映射：raw 指向领域类，覆盖高频口语别名（kind 与目标必须自洽）
        let by_raw: HashMap<&str, &PresetOntologySynonym> = lex
            .synonym_mappings
            .iter()
            .map(|s| (s.raw_term.as_str(), s))
            .collect();
        assert_eq!(by_raw.len(), 3);
        for (raw, kind, target) in [
            ("llm", TermKind::Class, "model"),
            ("bot", TermKind::Class, "agent"),
            ("todo", TermKind::Class, "task"),
        ] {
            let s = by_raw
                .get(raw)
                .unwrap_or_else(|| panic!("同义映射 {} 缺失", raw));
            assert_eq!(s.target_kind, kind, "{} 的 kind 应为类", raw);
            assert_eq!(s.target_key, target, "{} 应指向 {}", raw, target);
        }
        // weight_base 必须存在（1.0/1.5/2.0 分级由图谱边权消费）
        for r in &lex.relation_types {
            assert!(
                r.weight_base.is_some(),
                "关系词 {} 缺 weight_base",
                r.term_key
            );
        }
    }
}
