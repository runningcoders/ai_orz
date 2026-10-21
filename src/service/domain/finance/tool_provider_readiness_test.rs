//! 内置工具就绪自愈（修复器）测试
//!
//! 覆盖：六条 AND 闸门的判定边界、单一字段红线、幂等、批量上限、留痕形态与回滚依据。
//! 真实写库/端到端路径由 `tool_provider_test.rs` 的 sqlx 用例覆盖，这里专注纯判定。

#[cfg(test)]
mod tests {
    use crate::service::domain::finance::tool_provider::*;
    use serde_json::json;

    // ---------- 闸门 4/红线：提议只取 command 单字段 ----------

    #[test]
    fn extract_single_command_accepts_only_command_field() {
        let proposal = json!({ "command": "/usr/local/bin/skillhub" });
        assert_eq!(
            extract_single_command(&proposal).as_deref(),
            Some("/usr/local/bin/skillhub")
        );
    }

    #[test]
    fn extract_single_command_drops_extra_fields_but_keeps_command() {
        // 多余的 install_hint/timeout_ms 会被丢弃并 warn，但 command 仍然生效
        let proposal = json!({
            "command": "/opt/bin/cli",
            "timeout_ms": 9999,
            "install_hint": "curl | bash"
        });
        assert_eq!(
            extract_single_command(&proposal).as_deref(),
            Some("/opt/bin/cli")
        );
    }

    #[test]
    fn extract_single_command_rejects_malformed_proposals() {
        // 非对象 / 缺 command / 非字符串 / 空白 —— 一律不放行
        assert!(extract_single_command(&json!("just-a-string")).is_none());
        assert!(extract_single_command(&json!({ "other": "x" })).is_none());
        assert!(extract_single_command(&json!({ "command": 123 })).is_none());
        assert!(extract_single_command(&json!({ "command": "   " })).is_none());
        assert!(extract_single_command(&serde_json::Value::Null).is_none());
    }

    // ---------- 单一字段写回：其余 config 键零改动 ----------

    #[test]
    fn with_healed_command_preserves_other_config_keys() {
        let config = json!({
            "command": "~/.skillhub/skillhub",
            "timeout_ms": 60000,
            "install_hint": "skillhub.cn/install"
        });
        let healed = with_healed_command(&config, "/Users/aman/.local/bin/skillhub");

        assert_eq!(healed["command"], "/Users/aman/.local/bin/skillhub");
        // 其余键原样保留（自愈不得顺手改别的配置项）
        assert_eq!(healed["timeout_ms"], 60000);
        assert_eq!(healed["install_hint"], "skillhub.cn/install");
        // 键总数不变（没有新增也没有删除）
        assert_eq!(
            healed.as_object().unwrap().len(),
            config.as_object().unwrap().len()
        );
    }

    #[test]
    fn with_healed_command_creates_object_from_empty_config() {
        // 存量 PO config 可能是 null/非对象 —— 自愈后必须是含 command 的对象
        let healed = with_healed_command(&serde_json::Value::Null, "/bin/tool");
        assert_eq!(healed["command"], "/bin/tool");
        assert_eq!(healed.as_object().unwrap().len(), 1);
    }

    // ---------- 闸门 3：只修「找不到命令」这一类未就绪 ----------

    #[test]
    fn heal_reason_gate_only_accepts_cli_not_installed() {
        assert!(is_healable_readiness_reason("cli_not_installed"));
        // 其它未就绪原因（凭证缺失等）不属于命令路径问题，必须不处理
        assert!(!is_healable_readiness_reason("api_key_missing"));
        assert!(!is_healable_readiness_reason(""));
        assert!(!is_healable_readiness_reason("some_other_reason"));
    }

    // ---------- 闸门 5：幂等 ----------

    #[test]
    fn idempotent_when_proposed_command_equals_current() {
        // 提议与现值相同 → 视为已修复，不得重复写库
        assert!(is_noop_heal(Some("/bin/tool"), "/bin/tool"));
        // 现值缺失（存量 config 无 command）→ 需要修
        assert!(!is_noop_heal(None, "/bin/tool"));
        // 现值不同 → 需要修
        assert!(!is_noop_heal(
            Some("~/.skillhub/skillhub"),
            "/Users/aman/.local/bin/skillhub"
        ));
    }

    // ---------- 闸门 6：批量上限与去重窗口 ----------

    #[test]
    fn batch_limit_is_twenty_and_gate_blocks_beyond() {
        assert_eq!(SELF_HEAL_MAX_PER_RUN, 20);
        assert!(!exceeds_self_heal_budget(0));
        assert!(!exceeds_self_heal_budget(19));
        // 已修满 20 个 → 后续一律停手
        assert!(exceeds_self_heal_budget(20));
        assert!(exceeds_self_heal_budget(25));
    }

    #[test]
    fn dedup_window_is_seven_days() {
        assert_eq!(SELF_HEAL_DEDUP_WINDOW_MS, 7 * 24 * 60 * 60 * 1000);

        // 6 天前修过 → 仍在窗口内，跳过
        let now = 1_800_000_000_000u64;
        assert!(is_within_dedup_window(now - 6 * 24 * 3600 * 1000, now));
        // 8 天前修过 → 窗口外，允许再修
        assert!(!is_within_dedup_window(now - 8 * 24 * 3600 * 1000, now));
        // 时间戳异常（未来/零）时保守按「窗口内」处理，避免抖动
        assert!(is_within_dedup_window(0, now));
    }

    // ---------- 留痕形态 ----------

    #[test]
    fn heal_trace_metadata_carries_rollback_basis() {
        let entry = build_heal_trace_entry(
            "skillhub_cli",
            "SkillHub CLI",
            Some("~/.skillhub/skillhub"),
            "/Users/aman/.local/bin/skillhub",
            "{proposal}",
        );

        assert_eq!(entry.tool_id, "skillhub_cli");
        assert_eq!(entry.metadata["heal_kind"], SELF_HEAL_KIND);
        // old_command 是一键回滚依据，必须可读
        assert_eq!(entry.metadata["old_command"], "~/.skillhub/skillhub");
        assert_eq!(
            entry.metadata["new_command"],
            "/Users/aman/.local/bin/skillhub"
        );
        assert!(
            entry.metadata["rollback"]
                .as_str()
                .unwrap()
                .contains("回滚")
        );
        assert_eq!(entry.metadata["guard"]["single_field"], "command");
        assert_eq!(entry.metadata["guard"]["max_per_run"], 20);
        // call_id 含工具 id 与时间，便于按工具追溯
        assert!(entry.call_id.starts_with("selfheal-skillhub_cli-"));
        // 留痕本身是一次成功的"调用"，不应标成 Failed
        assert_eq!(
            entry.status,
            crate::pkg::tool_tracing::entry::ToolCallStatus::Completed
        );
    }

    #[test]
    fn heal_trace_recognized_as_self_heal_and_others_are_not() {
        let heal = build_heal_trace_entry("skillhub_cli", "SkillHub CLI", None, "/bin/x", "p");
        assert!(is_self_heal_entry(&heal));

        // 普通工具调用（无 heal_kind）不得被误判成自愈记录
        let normal = crate::pkg::tool_tracing::entry::ToolCallEntry {
            tool_id: "skillhub_cli".to_string(),
            call_id: "call-1".to_string(),
            metadata: json!({ "other": true }),
            ..Default::default()
        };
        assert!(!is_self_heal_entry(&normal));
    }
}

// ==================== 端到端：真实写库 + 六条闸门生效 ====================
//
// 走真实 DAL（sqlx::test 隔离库）验证只有落库才能证明的性质：
// 「写库只改 config.command」「未就绪原因不符不动」「禁用不动」「幂等/去重」。
//
// 断言口径刻意锚在**具体工具的最终配置**而不是总修复数：`reconcile` 会遍历
// 全部已注册内置工具，总数受测试机 PATH 环境影响（某台机器上 grep 可能在
// 某个补全目录里），用计数断言会制造与环境耦合的脆弱测试。

mod e2e {
    use crate::models::tool::Tool;
    use crate::pkg::RequestContext;
    use crate::service::domain::finance;
    use common::enums::{ControlMode, ToolProtocol, ToolStatus};
    use serde_json::json;
    use sqlx::SqlitePool;

    async fn init_env(
        pool: SqlitePool,
    ) -> (std::sync::Arc<dyn finance::FinanceDomain>, RequestContext) {
        crate::pkg::request_context_test_support::init_service_for_test();
        // 内置工厂必须注册：`get_by_id` 组装 Tool 时要能按 id 找到工厂，
        // 修复器也靠 registry.list_builtin_ids() 遍历
        crate::pkg::tool_registry::builtin::register_all(crate::pkg::tool_registry::get_registry());

        let domain = finance::new(
            crate::service::dal::model_provider::dal(),
            crate::service::dal::message_channel::dal(),
            crate::service::dal::mcp_server::dal(),
            crate::service::dal::mcp_tool::dal(),
            crate::service::dal::tool::dal(),
            crate::service::dal::brain::dal(),
            crate::service::dal::attachment::dal(),
        );
        let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
        (domain, ctx)
    }

    /// 把一个已注册的内置工具改成指定状态/配置（模拟运维现场）
    async fn seed_builtin(
        domain: &std::sync::Arc<dyn finance::FinanceDomain>,
        ctx: &RequestContext,
        id: &str,
        status: ToolStatus,
        config: serde_json::Value,
    ) {
        let po = crate::models::tool::ToolPo::new(
            id.to_string(),
            format!("{} tool", id),
            "builtin for self-heal e2e".to_string(),
            ToolProtocol::Builtin,
            config,
            None,
            vec!["test".to_string()],
            Some("test-user".to_string()),
        );
        let mut tool = Tool::from_po_for_management(po);
        tool.po.status = status;
        tool.po.control_mode = ControlMode::Auto;
        domain
            .tool_provider_manage()
            .create_tool(ctx.clone(), &tool)
            .await
            .expect("seed builtin tool");
    }

    async fn read_tool(
        domain: &std::sync::Arc<dyn finance::FinanceDomain>,
        ctx: &RequestContext,
        id: &str,
    ) -> Tool {
        domain
            .tool_provider_manage()
            .get_tool(ctx.clone(), id)
            .await
            .expect("get_tool ok")
            .expect("seeded tool must be readable")
    }

    #[sqlx::test]
    async fn disabled_cli_tool_is_never_healed(pool: SqlitePool) {
        let (domain, ctx) = init_env(pool).await;
        let original = json!({ "command": "~/.no-such-dir/no-such-cli" });
        seed_builtin(
            &domain,
            &ctx,
            "skillhub_cli",
            ToolStatus::Disabled,
            original.clone(),
        )
        .await;

        domain
            .tool_provider_manage()
            .reconcile_builtin_tool_readiness(ctx.clone())
            .await
            .expect("reconcile must not fail");

        // 闸门 2：禁用工具配置一字不动
        let stored = read_tool(&domain, &ctx, "skillhub_cli").await;
        assert_eq!(stored.po.config, original);
        assert_eq!(stored.po.status, ToolStatus::Disabled);
    }

    #[sqlx::test]
    async fn ready_cli_tool_is_never_healed(pool: SqlitePool) {
        let (domain, ctx) = init_env(pool).await;
        // `ls` 必在 PATH 内 → probe_readiness 为 Ready → 闸门 3 拦下
        let original = json!({ "command": "ls" });
        seed_builtin(
            &domain,
            &ctx,
            "skillhub_cli",
            ToolStatus::Enabled,
            original.clone(),
        )
        .await;

        domain
            .tool_provider_manage()
            .reconcile_builtin_tool_readiness(ctx.clone())
            .await
            .expect("reconcile must not fail");

        let stored = read_tool(&domain, &ctx, "skillhub_cli").await;
        assert_eq!(stored.po.config, original, "已就绪工具不得被改写");
    }

    #[sqlx::test]
    async fn non_cli_builtin_config_is_untouched(pool: SqlitePool) {
        let (domain, ctx) = init_env(pool).await;
        // 非 CLI 型（http_fetch 无 command）→ 默认 probe_readiness=Ready → 不动
        let original = json!({ "timeout_ms": 30000 });
        seed_builtin(
            &domain,
            &ctx,
            "http_fetch",
            ToolStatus::Enabled,
            original.clone(),
        )
        .await;

        domain
            .tool_provider_manage()
            .reconcile_builtin_tool_readiness(ctx.clone())
            .await
            .expect("reconcile must not fail");

        let stored = read_tool(&domain, &ctx, "http_fetch").await;
        assert_eq!(stored.po.config, original, "非 CLI 型工具配置不得被改写");
        assert!(stored.po.cli_command().is_none());
    }

    #[sqlx::test]
    async fn reconcile_never_mutates_non_command_fields(pool: SqlitePool) {
        let (domain, ctx) = init_env(pool).await;
        seed_builtin(
            &domain,
            &ctx,
            "skillhub_cli",
            ToolStatus::Enabled,
            json!({
                "command": "/no/such/binary-xyz",
                "timeout_ms": 12345,
                "install_hint": "keep me"
            }),
        )
        .await;

        domain
            .tool_provider_manage()
            .reconcile_builtin_tool_readiness(ctx.clone())
            .await
            .expect("reconcile must not fail");

        let stored = read_tool(&domain, &ctx, "skillhub_cli").await;
        // 红线：无论是否发生自愈，除 command 外的键都必须原样保留
        assert_eq!(stored.po.config["timeout_ms"], 12345);
        assert_eq!(stored.po.config["install_hint"], "keep me");
        assert!(stored.po.config.get("command").is_some());
    }

    #[sqlx::test]
    async fn reconcile_is_stable_across_repeated_runs(pool: SqlitePool) {
        let (domain, ctx) = init_env(pool).await;
        seed_builtin(
            &domain,
            &ctx,
            "skillhub_cli",
            ToolStatus::Enabled,
            json!({ "command": "/no/such/binary-xyz" }),
        )
        .await;

        domain
            .tool_provider_manage()
            .reconcile_builtin_tool_readiness(ctx.clone())
            .await
            .expect("first reconcile");
        let after_first = read_tool(&domain, &ctx, "skillhub_cli").await.po.config;

        domain
            .tool_provider_manage()
            .reconcile_builtin_tool_readiness(ctx.clone())
            .await
            .expect("second reconcile");
        let after_second = read_tool(&domain, &ctx, "skillhub_cli").await.po.config;

        // 幂等 + 7 天去重：第二轮不得再改动配置
        assert_eq!(after_first, after_second, "重复执行必须收敛（幂等/去重）");
    }
}
