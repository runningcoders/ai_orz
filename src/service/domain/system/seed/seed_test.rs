//! Seed 模块单元测试（纯函数测试，不需要 DB）

#[cfg(test)]
mod tests {
    use crate::service::domain::system::seed::defs::*;
    use crate::service::domain::system::seed::diff::*;
    use common::ontology::{
        PresetOntologyClass, PresetOntologyClassRelation, PresetOntologyLexicon,
        PresetOntologyRelationType, PresetOntologySynonym, TermKind,
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

    /// 构造最小词表（1 节点类 + 1 关系词 + 1 骨架三元组），供 ontology diff 测试
    fn make_test_lexicon() -> PresetOntologyLexicon {
        PresetOntologyLexicon {
            classes: vec![PresetOntologyClass {
                term_key: "concept".to_string(),
                display_name: "概念".to_string(),
                description: String::new(),
                required_fields: vec![],
                parent_term_key: None,
            }],
            relation_types: vec![PresetOntologyRelationType {
                term_key: "related".to_string(),
                display_name: "相关".to_string(),
                description: String::new(),
                weight_base: Some(1.0),
                inverse_key: None,
                direction: "undirected".to_string(),
            }],
            class_relations: vec![PresetOntologyClassRelation {
                subject_class_key: "concept".to_string(),
                relation_key: "related".to_string(),
                object_class_key: "concept".to_string(),
                cardinality: "many".to_string(),
                required: false,
            }],
            properties: vec![],
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

    /// 技能包：字节保真 + unix mode 保真（二进制附件与可执行件不能被文本通道破坏）
    #[test]
    fn skill_package_preserves_bytes_and_unix_mode() {
        use crate::service::domain::system::seed::bundle::{self, SkillFileEntry};

        let binary: Vec<u8> = vec![0x00, 0xFF, 0xFE, 0x01, 0x80];
        let entries = vec![
            SkillFileEntry {
                path: "skill.md".to_string(),
                bytes: b"# main".to_vec(),
                executable: false,
            },
            SkillFileEntry {
                path: "scripts/run.sh".to_string(),
                bytes: b"echo hi".to_vec(),
                executable: true,
            },
            SkillFileEntry {
                path: "assets/blob.bin".to_string(),
                bytes: binary.clone(),
                executable: false,
            },
        ];

        let zip_bytes = bundle::pack_skill_zip(&entries).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&zip_bytes)).unwrap();
        assert_eq!(archive.len(), 3);

        for entry in &entries {
            let mut file = archive.by_name(&entry.path).unwrap();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut buf).unwrap();
            assert_eq!(buf, entry.bytes, "{} 字节必须原样保真", entry.path);
            let exec = file.unix_mode().is_some_and(|m| m & 0o111 != 0);
            assert_eq!(exec, entry.executable, "{} 可执行位必须保真", entry.path);
        }
    }

    /// 整体压缩包：主文件 + 技能包可拆回，主文件单独可读（预览 / diff 用）
    #[test]
    fn bundle_round_trip_exposes_manifest_and_packages() {
        use crate::service::domain::system::seed::bundle;

        let snapshot = make_test_snapshot("BundleRoundTrip");
        let pkg_name = bundle::skill_package_name("SKILL1");
        assert_eq!(pkg_name, "skills/SKILL1.zip");

        let bytes = bundle::build_bundle(
            &snapshot,
            &[bundle::SkillPackage {
                name: pkg_name.clone(),
                bytes: vec![0x50, 0x4B, 0x03, 0x04, 0x00],
            }],
        )
        .unwrap();

        let (decoded, packages) = bundle::read_bundle(&bytes).unwrap();
        assert_eq!(decoded.organization.name, "BundleRoundTrip");
        assert_eq!(packages.get(&pkg_name).map(|v| v.len()), Some(5));
        assert_eq!(packages.len(), 1, "主文件不应混进技能包 map");

        let manifest = bundle::read_manifest(&bytes).unwrap();
        assert!(manifest.contains("BundleRoundTrip"));
    }

    /// 产物是压缩包：写入 .zip，read_file 取回的是包内 seed.json
    #[tokio::test]
    async fn test_store_bundle_round_trip() {
        use crate::service::domain::system::seed::bundle;

        let dir = std::env::temp_dir().join("ai_orz_seed_store_bundle_test");
        let _ = std::fs::remove_dir_all(&dir);

        let name = "test-snapshot";
        let snapshot = make_test_snapshot("BundleRoundTrip");
        let bundle_bytes = bundle::build_bundle(&snapshot, &[]).unwrap();

        let size =
            crate::service::domain::system::seed::store::write_file(&dir, name, &bundle_bytes)
                .await
                .unwrap();
        assert_eq!(size, bundle_bytes.len() as u64);

        // read_file 返回包内主文件文本，且能反序列化回同一份快照
        let resp = crate::service::domain::system::seed::store::read_file(&dir, name)
            .await
            .unwrap();
        assert_eq!(resp.name, "test-snapshot.zip");
        let decoded: SeedSnapshot = serde_json::from_str(&resp.content).unwrap();
        assert_eq!(decoded.organization.name, "BundleRoundTrip");

        let files = crate::service::domain::system::seed::store::list_files(&dir)
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "test-snapshot.zip");

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
    fn test_validate_seed_filename_appends_zip_extension() {
        // 新产物是压缩包：无名后缀默认补 .zip
        let name = crate::service::domain::system::seed::store::validate_seed_filename("snapshot")
            .unwrap();
        assert_eq!(name, "snapshot.zip");

        // 显式 .zip 后缀原样保留
        let name =
            crate::service::domain::system::seed::store::validate_seed_filename("snapshot.zip")
                .unwrap();
        assert_eq!(name, "snapshot.zip");
    }

    #[test]
    fn test_default_snapshot_parses_successfully() {
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        assert_eq!(snapshot.version, "1.0.0");
        assert_eq!(snapshot.users.len(), 1);
        assert_eq!(snapshot.model_providers.len(), 2);
        // 预置 3 个基础员工：前台接待 / 招聘官 / 图谱专家
        assert_eq!(snapshot.agents.len(), 3);
        // 预置 12 个技能（7 个 neural + 1 个 project_management + 1 个 git_branch_workflow
        // + 1 个 agent_recruitment + 1 个 user_reception + 1 个 thinking_method）
        assert_eq!(snapshot.skills.len(), 12);
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
        // organizations.config，project_management 是「同名双重身份」包，两侧都要配。
        // 注意：附件（attachment）走 BASE_AGENT_PACKS 出生自带，与组织/岗位无关，不在这里。
        let onboard = snapshot
            .organization
            .config
            .expect("default.json 必须带 organization.config")
            .agent_onboard;
        assert_eq!(onboard.required_tool_packs, vec!["project_management"]);
        assert_eq!(onboard.required_skill_packs, vec!["project_management"]);
    }

    /// 预置 Agent 的 `runtime_config` 必须是「可直接落库」的完整配置
    ///
    /// 背景（回归护栏）：`apply_single_preset_agent` 的 Overwrite 策略会**整行覆写**
    /// `runtime_config`（只保留本地 model_provider_id 与生命周期状态）。若 seed 里写空壳
    /// （如 `installed_tags: []`），用户点一次「同步预置 Agent」就会把在用 Agent 的工具包
    /// / 技能包清空 ⇒ 该 Agent 立刻失去全部非神经工具。seed 与在用配置必须同口径。
    #[test]
    fn test_preset_agents_runtime_config_is_complete() {
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        for agent in &snapshot.agents {
            let cfg: serde_json::Value = serde_json::from_str(&agent.runtime_config)
                .unwrap_or_else(|e| {
                    panic!("预置 Agent {} 的 runtime_config 非法 JSON: {e}", agent.id)
                });
            let tags = cfg
                .get("installed_tags")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| panic!("预置 Agent {} 缺少 installed_tags", agent.id));
            assert!(
                tags.iter().any(|t| t.as_str() == Some("neural")),
                "预置 Agent {} 的 installed_tags 缺少 neural（出生自带包，缺失即残废）",
                agent.id
            );
            assert!(
                !agent.soul.trim().is_empty() && !agent.capabilities.is_empty(),
                "预置 Agent {} 的人设或能力关键词为空（覆写会打空在用 Agent）",
                agent.id
            );
            // 基础员工一律显式持有 `ontology` 消费侧包（查词表 / 写记忆 / 检索 / 看图谱）。
            // 注意它**不是可达性前提**：消费侧 13 个工具恒带 neural（出生自带），
            // 见 `ontology_consumer_tools_are_neural_reachable`。显式标注是为了语义自洽
            //（基础员工 = 认知基建全量）与未来 proof（若将来出现非 neural 的消费侧工具，
            // 已标注的员工不会突然失能）。
            assert!(
                tags.iter().any(|t| t.as_str() == Some("ontology")),
                "预置基础员工 {} 缺少 ontology 消费侧工具包",
                agent.id
            );
            // 工具包与技能包必须**成套**：本体这条线上，工具侧是消费侧 13 个（恒 neural），
            // 技能侧是 `TEMPLATE_ONTOLOGY_COGNITION`（tags 含 ontology）。只装一半的表现是
            // 「模型有工具没人教」或「被教了却调不到工具」——两侧都不报错，靠人肉才看得出来。
            let packs = cfg
                .get("installed_skill_packs")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| panic!("预置 Agent {} 缺少 installed_skill_packs", agent.id));
            assert!(
                packs.iter().any(|t| t.as_str() == Some("ontology")),
                "预置基础员工 {} 的 installed_skill_packs 缺少 ontology（工具包与技能包必须成套）",
                agent.id
            );
        }
    }

    /// 预置 Agent 的 id 必须是 `TEMPLATE_*` 规范 id。
    ///
    /// **本护栏防的回归**：seed 与运行库的匹配键是 **id**（见 `sync_preset_agents`
    /// 「匹配键是 Agent ID（`TEMPLATE_*`），不是名称」）。若预置条目用运行时生成的裸 id
    /// （曾经这么干过），则：① 新装实例按 seed id 建一条；② 旧库里那条同名人仍在
    /// ⇒ 两张「图谱专家」重复造人。裸 id 也无法一眼区分「预置」与「用户自建」。
    #[test]
    fn test_preset_agent_ids_use_template_prefix() {
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        assert!(!snapshot.agents.is_empty(), "预置 Agent 列表为空");
        for agent in &snapshot.agents {
            assert!(
                agent.id.starts_with("TEMPLATE_"),
                "预置 Agent {} 的 id 不是 TEMPLATE_* 规范 id（按 id 匹配 ⇒ 会重复造人）",
                agent.id
            );
        }
    }

    /// 图谱专家是第三个基础员工
    #[test]
    fn test_preset_graph_specialist_agent() {
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        let g = snapshot
            .agents
            .iter()
            .find(|a| a.roles.iter().any(|r| r == "knowledge_graph_specialist"))
            .expect("预置图谱专家 Agent 缺失");
        assert_eq!(g.name, "图谱专家");
        assert_eq!(g.id, "TEMPLATE_KNOWLEDGE_GRAPH_AGENT");
        let cfg: serde_json::Value = serde_json::from_str(&g.runtime_config).unwrap();
        let tags: Vec<&str> = cfg["installed_tags"]
            .as_array()
            .expect("installed_tags 缺失")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            tags.contains(&"ontology"),
            "图谱专家需持有 ontology 消费侧工具包"
        );
        assert!(
            tags.contains(&"ontology_management"),
            "图谱专家需持有 ontology_management 治理侧工具包"
        );
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
        assert!(ids.contains(&"TEMPLATE_ONTOLOGY_COGNITION"));
        // 思维方法元技能（category=thinking_method，检索侧按该 tag 召回，设计 §3.6）
        assert!(ids.contains(&"TEMPLATE_PROBLEM_SOLVING"));

        // 全部 7 个神经技能必须包含 neural tag（含自我进化 / 项目上下文 / 本体认知三个新技能）
        //
        // ⚠️ 技能侧 neural 的真实语义是「**出生自带**」：`BASE_AGENT_PACKS` 含 neural ⇒
        // create_agent / train_agent 阶段 1 会装 neural 技能包 ⇒ 装上所有 tags 含 neural
        // 的已发布技能。摘掉本体认知的 neural ⇒ 新 Agent 出生不再自动持有本体方法论副本，
        // 只有显式装了 `ontology` 技能包的才有。
        //
        // 这与工具侧护栏 `ontology_consumer_tools_are_neural_reachable` 是**成套**的两侧：
        // - 工具侧 neural = 可达性（人人能写记忆 / 查词表）；
        // - 技能侧 neural = 出生自带（人人生而懂怎么给知识分类）。
        // 只留一侧都会退化：有工具没方法论 ⇒ 凭直觉造词污染蜂巢共享图谱（无租户）；
        // 有方法论没工具 ⇒ 技能里的引导全是空话。两侧都不报错，故各锁一条。
        let neural_ids = [
            "TEMPLATE_TOOL_MANAGEMENT",
            "TEMPLATE_SKILL_MANAGEMENT",
            "TEMPLATE_MEMORY_COGNITION",
            "TEMPLATE_COMMUNICATION",
            "TEMPLATE_SELF_EVOLUTION",
            "TEMPLATE_PROJECT_CONTEXT_COGNITION",
            "TEMPLATE_ONTOLOGY_COGNITION",
        ];
        for skill in &snapshot.skills {
            if neural_ids.contains(&skill.id.as_str()) {
                assert!(
                    skill.tags.contains(&"neural".to_string()),
                    "神经技能 {} 必须包含 neural tag",
                    skill.id
                );
            }
            // category 白名单：常规预置技能为 system；思维方法元技能为 thinking_method
            // （思维方法 = category=thinking_method 的技能，设计 §3.6）
            assert!(
                skill.category == "system" || skill.category == "thinking_method",
                "预置技能 category 只允许 system / thinking_method，实际: {}",
                skill.category
            );
            assert_eq!(skill.status, 1); // Published
            assert_eq!(skill.author_id, "TEMPLATE_ADMIN");
            assert_eq!(skill.author_type, 0); // User
            // 纯元信息：正文不进主文件，只登记技能包名（由编译期内嵌正文现打）
            assert_eq!(
                skill.package,
                crate::service::domain::system::seed::bundle::skill_package_name(&skill.id),
                "预置技能 {} 的 package 名必须由 skill_id 推导",
                skill.id
            );
        }
    }

    /// 默认快照登记的每个技能包都必须能从编译期内嵌正文打包出来，且正文逐字节一致。
    ///
    /// 双向护栏：
    /// - 正向：default.json 写了 `package` 但 embedded.rs 没注册正文 ⇒ 初始化会直接
    ///   报「缺少技能包」被打断；
    /// - 反向：embedded.rs 注册了正文但 default.json 没登记 ⇒ 该技能正文永远进不了库，
    ///   装出来的是空壳。
    #[test]
    fn test_default_snapshot_packages_resolve_from_embedded_files() {
        use crate::service::domain::system::seed::embedded;
        use std::io::{Cursor, Read as _};

        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        let packages =
            crate::service::domain::system::seed::default::embedded_default_skill_packages();
        assert!(!snapshot.skills.is_empty(), "默认快照技能列表为空");

        let declared: std::collections::HashSet<&str> =
            snapshot.skills.iter().map(|s| s.id.as_str()).collect();

        for skill in &snapshot.skills {
            let name = skill.package.as_str();
            let bytes = packages
                .get(name)
                .unwrap_or_else(|| panic!("预置技能 {} 的技能包 {} 未打包", skill.id, name));

            // 包内根引导文件必须与编译期内嵌正文一致
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
                .unwrap_or_else(|e| panic!("技能包 {} 解包失败: {e}", name));
            let mut file = archive
                .by_name(crate::models::skill::SKILL_MAIN_FILE)
                .unwrap_or_else(|_| panic!("技能包 {} 内缺少根引导文件", name));
            let mut content = String::new();
            file.read_to_string(&mut content).unwrap();
            let expected = {
                let path = format!(
                    "{}{}/{}",
                    embedded::SKILL_REF_PREFIX,
                    skill.id,
                    crate::models::skill::SKILL_MAIN_FILE
                );
                embedded::embedded_skill_files()
                    .iter()
                    .find(|(p, _)| *p == path)
                    .map(|(_, c)| *c)
                    .unwrap_or_else(|| panic!("预置技能 {} 正文未内嵌", skill.id))
            };
            assert_eq!(
                content, expected,
                "预置技能 {} 包内正文与内嵌正文不一致",
                skill.id
            );
        }

        // 反向：内嵌正文里的每个技能目录都必须在主文件里登记
        for (path, _) in embedded::embedded_skill_files() {
            let rest = path
                .strip_prefix(embedded::SKILL_REF_PREFIX)
                .expect("内嵌路径必须以 skills/ 开头");
            let skill_id = rest.split('/').next().unwrap_or(rest);
            assert!(
                declared.contains(skill_id),
                "内嵌技能正文 {} 未在 default.json 登记（正文进不了库）",
                path
            );
        }
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
