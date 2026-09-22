//! 工具授权子域测试（阶段③批次一 S1b）
//!
//! 覆盖：审批状态机流转 / 证据五要素正反例 / 同签名防重 / 撤销即时 / 消耗闭环。
//! 证据链查消息用 FakeMessageDal（不依赖 DB）；decide 红线④与 User 消息产生路径
//! 旁路 grep 复核在验收记录中单独执行。

#[cfg(test)]
mod tests {
    use crate::models::message::{Message, MessagePo};
    use crate::pkg::RequestContext;
    use crate::pkg::authorization::{AuthorizationStatus, command_signature};
    use crate::service::dal::message::MessageDal;
    use crate::service::domain::finance::tool_authorization::AuthorizationService;
    use crate::service::domain::finance::{
        AuthorizationDecisionCmd, CreateAuthorizationCmd, ToolAuthorizationManage,
    };
    use common::api::EvidenceClassDto;
    use common::enums::{MessageRole, MessageStatus};
    use common::error::Result;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    // ============ FakeMessageDal：仅实现 find_by_id，其余 unreachable ============

    #[derive(Debug, Default)]
    struct FakeMessageDal {
        messages: RwLock<HashMap<String, MessagePo>>,
        /// T5·S3：save_message 落库记录（验证双向通知落库行为）
        saved: RwLock<Vec<MessagePo>>,
    }

    impl FakeMessageDal {
        fn add_user_message(&self, id: &str, from: &str, at_ms: i64) {
            let po = MessagePo {
                id: id.to_string(),
                from_id: from.to_string(),
                to_id: "agent-b".to_string(),
                from_role: MessageRole::User,
                to_role: MessageRole::Agent,
                content: "同意".to_string(),
                status: MessageStatus::Processed,
                created_at: at_ms,
                ..Default::default()
            };
            self.messages.write().unwrap().insert(id.to_string(), po);
        }

        /// 伪装消息：from_id 冒充归属用户但角色为 Agent（专测证据五要素第③条）
        fn add_disguised_message(&self, id: &str, at_ms: i64) {
            let po = MessagePo {
                id: id.to_string(),
                from_id: "user-aman".to_string(),
                to_id: "agent-b".to_string(),
                from_role: MessageRole::Agent,
                to_role: MessageRole::Agent,
                content: "冒充用户".to_string(),
                status: MessageStatus::Processed,
                created_at: at_ms,
                ..Default::default()
            };
            self.messages.write().unwrap().insert(id.to_string(), po);
        }

        fn add_agent_message(&self, id: &str, from: &str, at_ms: i64) {
            let po = MessagePo {
                id: id.to_string(),
                from_id: from.to_string(),
                to_id: "user-aman".to_string(),
                from_role: MessageRole::Agent,
                to_role: MessageRole::User,
                content: "代为确认".to_string(),
                status: MessageStatus::Processed,
                created_at: at_ms,
                ..Default::default()
            };
            self.messages.write().unwrap().insert(id.to_string(), po);
        }
    }

    macro_rules! impl_unreachable {
        ($($name:ident($($arg:ty),*)),* $(,)?) => {};
    }
    impl_unreachable!();

    #[async_trait::async_trait]
    impl MessageDal for FakeMessageDal {
        async fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<Message>> {
            Ok(self
                .messages
                .read()
                .unwrap()
                .get(id)
                .cloned()
                .map(Message::from_po))
        }

        async fn save_message(&self, _ctx: RequestContext, m: &Message) -> Result<()> {
            self.saved.write().unwrap().push(m.po.clone());
            Ok(())
        }

        async fn query(
            &self,
            _ctx: RequestContext,
            _q: crate::service::dao::message::MessageQuery,
        ) -> Result<Vec<Message>> {
            unimplemented!("本测试不需要")
        }

        async fn list_by_task_id(
            &self,
            _ctx: RequestContext,
            _task_id: &str,
            _limit: Option<usize>,
        ) -> Result<Vec<Message>> {
            unimplemented!("本测试不需要")
        }

        async fn list_by_project_id(
            &self,
            _ctx: RequestContext,
            _project_id: &str,
            _limit: Option<usize>,
        ) -> Result<Vec<Message>> {
            unimplemented!("本测试不需要")
        }

        async fn list_by_from_id(
            &self,
            _ctx: RequestContext,
            _from_id: &str,
            _limit: Option<usize>,
        ) -> Result<Vec<Message>> {
            unimplemented!("本测试不需要")
        }

        async fn list_by_to_id(
            &self,
            _ctx: RequestContext,
            _to_id: &str,
            _limit: Option<usize>,
        ) -> Result<Vec<Message>> {
            unimplemented!("本测试不需要")
        }

        async fn list_by_status(
            &self,
            _ctx: RequestContext,
            _s: Vec<MessageStatus>,
            _limit: Option<usize>,
        ) -> Result<Vec<Message>> {
            unimplemented!("本测试不需要")
        }

        async fn update_status(
            &self,
            _ctx: RequestContext,
            _id: &str,
            _s: MessageStatus,
        ) -> Result<()> {
            unimplemented!("本测试不需要")
        }

        async fn count_by_task_id(&self, _ctx: RequestContext, _task_id: &str) -> Result<u64> {
            unimplemented!("本测试不需要")
        }

        async fn count(
            &self,
            _ctx: RequestContext,
            _q: crate::service::dao::message::MessageQuery,
        ) -> Result<u64> {
            unimplemented!("本测试不需要")
        }

        async fn find_id_by_external_key(
            &self,
            _ctx: RequestContext,
            _k: &str,
        ) -> Result<Option<String>> {
            unimplemented!("本测试不需要")
        }

        async fn has_pending_message_for_agent(
            &self,
            _ctx: RequestContext,
            _a: &str,
            _t: common::enums::MessageType,
        ) -> Result<bool> {
            unimplemented!("本测试不需要")
        }

        async fn delete_message(&self, _ctx: RequestContext, _id: &str) -> Result<()> {
            unimplemented!("本测试不需要")
        }

        async fn delete_by_task_id(&self, _ctx: RequestContext, _t: &str) -> Result<()> {
            unimplemented!("本测试不需要")
        }

        async fn search(
            &self,
            _ctx: RequestContext,
            _s: crate::service::dao::message::MessageSearch,
        ) -> Result<Vec<Message>> {
            unimplemented!("本测试不需要")
        }

        async fn rebuild_vectors(
            &self,
            _ctx: RequestContext,
            _p: &crate::pkg::background_task::TaskProgressCounter,
        ) -> Result<()> {
            unimplemented!("本测试不需要")
        }
    }

    // ============ 测试辅助 ============

    const SIGNATURE: &str = "docker push registry.local/app:1.0";

    fn user_ctx(pool: &sqlx::SqlitePool) -> RequestContext {
        let storage = crate::pkg::storage::test_support::create_test_storage(pool.clone());
        RequestContext::builder()
            .user_id("user-aman")
            .storage(storage)
            .build()
    }

    fn agent_ctx(pool: &sqlx::SqlitePool) -> RequestContext {
        let storage = crate::pkg::storage::test_support::create_test_storage(pool.clone());
        RequestContext::builder()
            .user_id("user-aman")
            .agent_id("agent-a")
            .storage(storage)
            .build()
    }

    fn service_with_evidence() -> Arc<AuthorizationService> {
        // 证据时间戳取「真实当前时间 + 1 天」：授权单建单时刻为测试运行当下，
        // 证据必须晚于建单（五要素第④条），偏移量保证测试执行窗口内恒成立。
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            + 86_400_000;
        let dal = Arc::new(FakeMessageDal::default());
        dal.add_user_message("ev-1", "user-aman", base);
        dal.add_agent_message("ev-agent", "agent-a", base);
        dal.add_disguised_message("ev-disguised", base);
        Arc::new(AuthorizationService::new().with_message_dal(dal))
    }

    async fn make_pending(svc: &AuthorizationService, pool: &sqlx::SqlitePool) -> String {
        let pending = svc
            .create_pending_authorization(
                user_ctx(pool),
                CreateAuthorizationCmd {
                    agent_id: "agent-a".to_string(),
                    tool_id: "shell_exec".to_string(),
                    command_signature: command_signature(SIGNATURE),
                    blocking_rule: "git_dangerous_subcommand".to_string(),
                    rule_idempotent: false,
                    reason: Some("测试".to_string()),
                },
            )
            .await
            .unwrap();
        pending.authorization_id
    }

    fn approve_cmd(auth_id: &str) -> AuthorizationDecisionCmd {
        AuthorizationDecisionCmd {
            authorization_id: auth_id.to_string(),
            approve: true,
            evidence_class: Some(EvidenceClassDto::ChatMediated),
            evidence_message_id: Some("ev-1".to_string()),
            mediator_agent_id: Some("agent-b".to_string()),
            ..Default::default()
        }
    }

    // ============ 状态机全流转 ============

    #[sqlx::test]
    async fn pending_to_active_via_mediated_approval(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let out = svc
            .decide_authorization(user_ctx(&pool), approve_cmd(&auth_id))
            .await
            .unwrap();
        assert_eq!(out.status, AuthorizationStatus::Active);
        assert!(out.grant_id.is_some());
    }

    #[sqlx::test]
    async fn pending_to_rejected(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let cmd = AuthorizationDecisionCmd {
            authorization_id: auth_id.clone(),
            approve: false,
            ..Default::default()
        };
        let out = svc
            .decide_authorization(user_ctx(&pool), cmd)
            .await
            .unwrap();
        assert_eq!(out.status, AuthorizationStatus::Rejected);
        assert!(out.grant_id.is_none());
    }

    #[sqlx::test]
    async fn active_to_revoked_immediately(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        svc.decide_authorization(user_ctx(&pool), approve_cmd(&auth_id))
            .await
            .unwrap();
        let out = svc
            .revoke_authorization(user_ctx(&pool), &auth_id, Some("撤销".to_string()))
            .await
            .unwrap();
        assert_eq!(out.status, AuthorizationStatus::Revoked);
        // 撤销后 active_grants_for 即时为空
        let grants = svc
            .active_grants_for(user_ctx(&pool), "agent-a", "shell_exec")
            .await
            .unwrap();
        assert!(grants.is_empty());
    }

    #[sqlx::test]
    async fn active_to_consumed_via_uses_exhaustion(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        svc.decide_authorization(user_ctx(&pool), approve_cmd(&auth_id))
            .await
            .unwrap();
        // 非幂等默认 max_uses=1：第一次消耗放行，第二次闭环 Consumed
        let first = svc.consume_grant(user_ctx(&pool), &auth_id).await.unwrap();
        assert!(first.is_some());
        assert_eq!(first.unwrap().uses, 1);
        let second = svc.consume_grant(user_ctx(&pool), &auth_id).await.unwrap();
        assert!(second.is_none(), "非幂等 1 次后应闭环不可再消耗");
    }

    // ============ 红线④：decide 强制 user ctx ============

    #[sqlx::test]
    async fn agent_ctx_decide_without_mediation_is_structurally_rejected(pool: sqlx::SqlitePool) {
        // 红线④细化（§14.3/§15.1）：Agent ctx 仅放行聊天代呈（ChatMediated），
        // 无代呈形态的 decide 仍结构性拒绝
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        // 无代呈形态（UI 直批形态）的 Agent ctx decide：结构性拒绝
        let plain_cmd = AuthorizationDecisionCmd {
            authorization_id: auth_id.clone(),
            approve: true,
            ..Default::default()
        };
        let err = svc
            .decide_authorization(agent_ctx(&pool), plain_cmd)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("Agent 不得自我审批"));
        // 授权单状态不受影响
        let items = svc
            .list_authorizations(user_ctx(&pool), Default::default())
            .await
            .unwrap();
        assert_eq!(
            items[0].status,
            common::api::AuthorizationStatusDto::Pending
        );
    }

    #[sqlx::test]
    async fn agent_ctx_chat_mediated_mediator_forced_from_ctx(pool: sqlx::SqlitePool) {
        // Agent ctx 代呈：mediator 强制取 ctx.agent_id（覆盖客户端传值）——
        // 客户端伪造 mediator=申请人（agent-a）不影响裁决，实际代呈者=agent-b
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let ctx_b = RequestContext::builder()
            .user_id("user-aman")
            .agent_id("agent-b")
            .storage(crate::pkg::storage::test_support::create_test_storage(
                pool.clone(),
            ))
            .build();
        let mut cmd = approve_cmd(&auth_id);
        cmd.mediator_agent_id = Some("agent-a".to_string());
        let out = svc.decide_authorization(ctx_b, cmd).await.unwrap();
        assert_eq!(out.status, AuthorizationStatus::Active);
        assert!(out.grant_id.is_some());
    }

    // ============ 证据五要素正反例 ============

    #[sqlx::test]
    async fn chat_mediated_without_evidence_is_rejected(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let cmd = AuthorizationDecisionCmd {
            authorization_id: auth_id.clone(),
            approve: true,
            evidence_class: Some(EvidenceClassDto::ChatMediated),
            evidence_message_id: None,
            mediator_agent_id: Some("agent-b".to_string()),
            ..Default::default()
        };
        let err = svc
            .decide_authorization(user_ctx(&pool), cmd)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("证据消息 ID"));
    }

    #[sqlx::test]
    async fn self_mediation_is_rejected(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let mut cmd = approve_cmd(&auth_id);
        cmd.mediator_agent_id = Some("agent-a".to_string());
        let err = svc
            .decide_authorization(user_ctx(&pool), cmd)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("申请人不得自代呈"));
    }

    #[sqlx::test]
    async fn agent_evidence_is_rejected(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let mut cmd = approve_cmd(&auth_id);
        cmd.evidence_message_id = Some("ev-disguised".to_string());
        let err = svc
            .decide_authorization(user_ctx(&pool), cmd)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("角色非用户"));
    }

    #[sqlx::test]
    async fn evidence_replay_is_rejected(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        // 第一单批准消费 ev-1
        let a1 = make_pending(&svc, &pool).await;
        svc.decide_authorization(user_ctx(&pool), approve_cmd(&a1))
            .await
            .unwrap();
        // 第二单复用同一证据 → 防重放拒绝
        let a2 = make_pending(&svc, &pool).await;
        let err = svc
            .decide_authorization(user_ctx(&pool), approve_cmd(&a2))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("防重放"));
    }

    #[sqlx::test]
    async fn missing_message_is_rejected(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let mut cmd = approve_cmd(&auth_id);
        cmd.evidence_message_id = Some("ev-missing".to_string());
        let err = svc
            .decide_authorization(user_ctx(&pool), cmd)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("证据消息不存在"));
    }

    #[sqlx::test]
    async fn ui_direct_approval_without_evidence_passes(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let cmd = AuthorizationDecisionCmd {
            authorization_id: auth_id.clone(),
            approve: true,
            ..Default::default()
        };
        let out = svc
            .decide_authorization(user_ctx(&pool), cmd)
            .await
            .unwrap();
        assert_eq!(out.status, AuthorizationStatus::Active);
    }

    // ============ 同签名防重 / 查询 / ttl ============

    #[sqlx::test]
    async fn duplicate_pending_same_signature_is_rejected(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        make_pending(&svc, &pool).await;
        let err = svc
            .create_pending_authorization(
                user_ctx(&pool),
                CreateAuthorizationCmd {
                    agent_id: "agent-a".to_string(),
                    tool_id: "shell_exec".to_string(),
                    command_signature: command_signature(SIGNATURE),
                    blocking_rule: "git_dangerous_subcommand".to_string(),
                    rule_idempotent: false,
                    reason: None,
                },
            )
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("同签名"));
    }

    #[sqlx::test]
    async fn list_filters_by_status_and_agent(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        make_pending(&svc, &pool).await;
        let all = svc
            .list_authorizations(user_ctx(&pool), Default::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 1);
        let q = common::api::AuthorizationQueryRequest {
            agent_id: Some("agent-other".to_string()),
            ..Default::default()
        };
        let none = svc.list_authorizations(user_ctx(&pool), q).await.unwrap();
        assert!(none.is_empty());
    }

    #[sqlx::test]
    async fn scope_ttl_clamped_and_prefix_grant_active(pool: sqlx::SqlitePool) {
        let svc = service_with_evidence();
        let auth_id = make_pending(&svc, &pool).await;
        let mut cmd = approve_cmd(&auth_id);
        cmd.ttl_secs = Some(1);
        cmd.prefix_match = true;
        cmd.scope_command_signature = Some("docker".to_string());
        let out = svc
            .decide_authorization(user_ctx(&pool), cmd)
            .await
            .unwrap();
        assert_eq!(out.status, AuthorizationStatus::Active);
        let grants = svc
            .active_grants_for(user_ctx(&pool), "agent-a", "shell_exec")
            .await
            .unwrap();
        assert_eq!(grants.len(), 1);
        assert!(grants[0].prefix_match);
        assert_eq!(grants[0].command_signature, "docker");
    }
}

// ============ T5·S3 双向通知（落库即通知） ============

/// 构造带 FakeMessageDal 的服务（同时返回 Fake 引用供断言）；证据时间戳同 service_with_evidence
fn notifier_service() -> (Arc<AuthorizationService>, Arc<FakeMessageDal>) {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        + 86_400_000;
    let dal = Arc::new(FakeMessageDal::default());
    dal.add_user_message("ev-1", "user-aman", base);
    let svc = Arc::new(AuthorizationService::new().with_message_dal(dal.clone()));
    (svc, dal)
}

fn mk_cmd() -> CreateAuthorizationCmd {
    CreateAuthorizationCmd {
        agent_id: "agent-a".to_string(),
        tool_id: "shell_exec".to_string(),
        command_signature: command_signature(SIGNATURE),
        blocking_rule: "git_dangerous_subcommand".to_string(),
        rule_idempotent: false,
        reason: Some("测试".to_string()),
    }
}

#[sqlx::test]
async fn pending_and_decided_notifications_persisted(pool: sqlx::SqlitePool) {
    let (svc, dal) = notifier_service();
    let auth_id = make_pending(&svc, &pool).await;

    // 建单推送恰好 1 条：System → 归属用户，携带授权单 ID
    {
        let saved = dal.saved.read().unwrap();
        assert_eq!(saved.len(), 1, "建单后应恰有 1 条建单推送");
        let po = &saved[0];
        assert_eq!(po.from_role, MessageRole::System);
        assert_eq!(po.to_role, MessageRole::User);
        assert_eq!(po.to_id, "user-aman");
        assert!(
            po.content.contains(&auth_id),
            "建单推送应携带 authorization_id，实际: {}",
            po.content
        );
    }

    // 审批批准 → 决策回推恰好新增 1 条：System → 申请人 Agent，携带授权单 ID
    svc.decide_authorization(user_ctx(&pool), approve_cmd(&auth_id))
        .await
        .unwrap();
    let saved = dal.saved.read().unwrap();
    assert_eq!(saved.len(), 2, "决策后应新增 1 条决策回推");
    let po = &saved[1];
    assert_eq!(po.from_role, MessageRole::System);
    assert_eq!(po.to_role, MessageRole::Agent);
    assert_eq!(po.to_id, "agent-a");
    assert!(
        po.content.contains(&auth_id),
        "决策回推应携带 authorization_id，实际: {}",
        po.content
    );
}

#[sqlx::test]
async fn duplicate_signature_reuse_notifies_once(pool: sqlx::SqlitePool) {
    let (svc, dal) = notifier_service();
    svc.create_pending_authorization(user_ctx(&pool), mk_cmd())
        .await
        .unwrap();
    let again = svc
        .create_pending_authorization(user_ctx(&pool), mk_cmd())
        .await;
    assert!(again.is_err(), "同签名 Pending 复用应拒绝重复建单");
    assert_eq!(
        dal.saved.read().unwrap().len(),
        1,
        "同签名 Pending 复用不重复通知：建单推送仍仅 1 条"
    );
}

#[sqlx::test]
async fn notification_degraded_without_message_dal(pool: sqlx::SqlitePool) {
    // 无消息 DAL：通知降级为 log_warn 留痕，建单与决策主流程不受阻断。
    // 决策用 UI 直批形态（免证据链，不依赖消息 DAL）——批准分支的通知
    // 降级与决策主流程解耦正是本用例要验证的行为。
    let svc = AuthorizationService::new();
    let auth_id = make_pending(&svc, &pool).await;
    assert!(!auth_id.is_empty(), "DAL 未接线时建单仍应成功");
    let ui_approve = AuthorizationDecisionCmd {
        authorization_id: auth_id,
        approve: true,
        ..Default::default()
    };
    svc.decide_authorization(user_ctx(&pool), ui_approve)
        .await
        .unwrap_or_else(|e| panic!("DAL 未接线时决策不应被通知阻断: {e}"));
}
