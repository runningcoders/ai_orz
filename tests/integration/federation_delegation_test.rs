//! 跨组织 Agent 委派集成测试（跨组织业务调用方案 P4 验收）。
//!
//! 端到端验收（设计稿 §五）：A 组织用户 @ 对端 Agent（`agent:<id>@<org_id>`）
//! → B 侧 Agent 执行 → A 侧收到结果，双方日志带 org 维度（声明头）。
//!
//! 结构：
//! - e2e：A 侧 MessageConsumer 直连路由 → 联邦 A2A 出站（Bearer + 声明头）→
//!   真实 B 节点 /a2a（TestApp 真实 TCP）→ B 侧网关 Agent（Cli/cat，无 LLM）
//!   手动驱动消费回复 → A 侧轮询 tasks/get 到终态 → 回复落库给原用户。
//! - 降级：org 无 Active 连接 / 连接未开放 a2a_task → 不外呼，走既有流程
//!   （Agent 不存在时表现为 NotFound，证明未委派）。
//!
//! 双节点说明同 federation_a2a_test：共享全局 Storage 单例，
//! "对端组织" 用独立 org id 表达（逻辑隔离）。

#[path = "../common/mod.rs"]
mod common;

use ::common::api::InitializeSystemRequest;
use ::common::constants::agent_roles::ROLE_A2A_GATEWAY;
use ::common::enums::{AgentKind, AgentStatus, MessageRole, MessageStatus, MessageType};
use ai_orz::models::agent::{Agent, AgentPo, ExternalAgentConfig};
use ai_orz::models::message::Message;
use ai_orz::models::message::MessagePo;
use ai_orz::models::organization_link::OrganizationLinkPo;
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::aop::Consumer;
use sqlx::SqlitePool;
use std::time::Duration;
use uuid::Uuid;

/// Domain 层直接创建测试节点组织（返回 org_id / user_id / jwt）。
async fn create_node(app: &crate::common::TestApp, tag: &str) -> (String, String, String) {
    let ctx = RequestContext::from_storage(
        format!("fed-del-{tag}").as_str(),
        ai_orz::pkg::storage::get().clone(),
    );
    let username = format!("{tag}-admin-{}", Uuid::now_v7());
    let password = format!("{tag}-pw-{}", Uuid::now_v7());
    let (org_id, user_id) = ai_orz::service::domain::organization::domain()
        .organization_manage()
        .create_org_and_owner(
            ctx,
            InitializeSystemRequest {
                organization_name: format!("{tag}-Org-{}", Uuid::now_v7()),
                admin_username: username.clone(),
                admin_password: password.clone(),
                description: None,
                admin_display_name: None,
                admin_email: None,
                chat_model: None,
                embedding_model: None,
            },
        )
        .await
        .expect("create node org via domain should succeed");
    let jwt = crate::common::factories::login_and_get_jwt(app, &org_id, &username, &password).await;
    (org_id, user_id, jwt)
}

fn org_ctx(tag: &str) -> RequestContext {
    RequestContext::from_storage(
        format!("fed-del-{tag}").as_str(),
        ai_orz::pkg::storage::get().clone(),
    )
}

/// 播种一个 Onboarded 的 Cli Agent（cat：回显 stdin prompt，无需 LLM）。
/// 返回 agent id 供 @ 寻址。roles 由调用方指定。
///
/// 注意：同一测试进程共享全局 Storage，`ROLE_A2A_GATEWAY` 角色 Agent 应保持
/// 唯一（B 侧 tasks/send 的 `resolve_agent(by_role)` 是全局解析、AgentPo 无
/// organization_id 列），否则 tasks/send 可能命中别的用例的网关，e2e 轮询超时。
async fn seed_cli_agent(org: &str, tag: &str, roles: Vec<String>) -> String {
    let mut po = AgentPo::new(
        format!("{tag}-网关-{}", Uuid::now_v7()),
        roles,
        "联邦回显网关".to_string(),
        vec!["chat".to_string()],
        "测试灵魂".to_string(),
        String::new(), // Cli agent 无 provider
        "fed-del-seed".to_string(),
    );
    po.status = AgentStatus::Onboarded;
    po.kind = AgentKind::Cli;
    po.set_external_config(ExternalAgentConfig::Cli {
        command: "cat".to_string(),
        args: vec![],
        work_dir: "/tmp".to_string(),
        env: vec![],
        timeout_secs: 10,
        prompt_template: None,
    });
    let agent_id = po.id.clone();
    ai_orz::service::dal::agent::dal()
        .create(
            org_ctx(tag).to_builder().organization_id(org).build(),
            &Agent::from_po(po),
        )
        .await
        .expect("seed cli gateway agent failed");
    agent_id
}

/// 播种一条 Active 连接。direction=b_in：local=org_b 收，peer=org_a 发，
/// peer_token_hash = sha256(credential)（B 侧入站校验）。
/// direction=a_out：local=org_a 出站用，access_token = credential，endpoint 指向 B。
async fn seed_link(
    local_org: &str,
    peer_org: &str,
    endpoint: &str,
    access_token: &str,
    peer_token_hash: &str,
    capabilities: &str,
    tag: &str,
) {
    let mut link = OrganizationLinkPo::new(
        Uuid::now_v7().to_string(),
        local_org.to_string(),
        peer_org.to_string(),
        endpoint.to_string(),
        access_token.to_string(),
        peer_token_hash.to_string(),
        format!("fed-del-{tag}"),
    );
    link.capabilities = capabilities.to_string();
    ai_orz::service::dao::organization_link::dao()
        .insert(org_ctx(tag), &link)
        .await
        .expect("seed link failed");
}

/// 直接落库一条 user → agent 消息（不经 delivery，绕开事件发布）。
async fn save_user_message(
    org: &str,
    from_user: &str,
    to_agent: &str,
    content: &str,
    tag: &str,
) -> String {
    let po = MessagePo {
        id: Uuid::now_v7().to_string(),
        from_id: from_user.to_string(),
        to_id: to_agent.to_string(),
        from_role: MessageRole::User,
        to_role: MessageRole::Agent,
        message_type: MessageType::Text,
        status: MessageStatus::Pending,
        content: content.to_string(),
        organization_id: Some(org.to_string()),
        created_by: from_user.to_string(),
        modified_by: from_user.to_string(),
        ..Default::default()
    };
    let msg = Message::from_po(po);
    ai_orz::service::dal::message::dal()
        .save_message(org_ctx(tag), &msg)
        .await
        .expect("save message failed");
    msg.po.id.clone()
}

fn message_event_json(m: &Message) -> serde_json::Value {
    serde_json::json!({
        "message_id": m.po.id,
        "project_id": m.po.project_id,
        "task_id": m.po.task_id,
        "from_id": m.po.from_id,
        "from_role": m.po.from_role as i32,
        "to_id": m.po.to_id,
        "to_role": m.po.to_role as i32,
        "message_type": m.po.message_type as i32,
        "content": m.po.content,
        "created_at": m.po.created_at,
    })
}

/// P5 联邦 Agent 目录：聚合 Active 对端 capabilities（mention picker 数据源）。
#[sqlx::test]
async fn test_list_federation_agents_aggregates_peer_capabilities(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let base_url = app.serve_on_random_port().await;

    let (org_a, _user_a, jwt_a) = create_node(&app, "faga").await;
    let (org_b, _user_b, _jwt_b) = create_node(&app, "fagb").await;
    // 普通角色即可：capabilities 端点只过滤 Onboarded 非 Remote，
    // 且避免与 e2e 用例的 ROLE_A2A_GATEWAY Agent 产生 resolve 二义性
    let gw_id = seed_cli_agent(&org_b, "fagb-gw", vec!["tester".to_string()]).await;

    // A → B 出站连接（endpoint 指向真实 B 节点；B 侧入站校验凭证）
    let credential = "9".repeat(64);
    seed_link(
        &org_b,
        &org_a,
        "https://peer-a.example.com",
        "unused",
        &sha256::digest(credential.as_bytes()),
        r#"["a2a_task"]"#,
        "fagb-link",
    )
    .await;
    seed_link(
        &org_a,
        &org_b,
        &base_url,
        &credential,
        "unused-hash",
        r#"["a2a_task"]"#,
        "faga-link",
    )
    .await;

    let (status, body) = app
        .get_with_jwt("/api/v1/organization/links/federation-agents", &jwt_a)
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {}", body);
    let data = crate::common::assert_api_ok(status, &body);

    let groups = data
        .get("groups")
        .and_then(|v| v.as_array())
        .expect("groups array");
    let group = groups
        .iter()
        .find(|g| g.get("org_id").and_then(|v| v.as_str()) == Some(org_b.as_str()))
        .expect("org_b group should be aggregated");
    let agents = group
        .get("agents")
        .and_then(|v| v.as_array())
        .expect("agents array");
    assert!(
        agents
            .iter()
            .any(|a| a.get("id").and_then(|v| v.as_str()) == Some(gw_id.as_str())),
        "gateway agent should be listed, got: {}",
        body
    );
    let caps = group
        .get("capabilities")
        .and_then(|v| v.as_array())
        .expect("capabilities array");
    assert!(
        caps.iter().any(|c| c.as_str() == Some("a2a_task")),
        "a2a_task capability should be exposed"
    );
}

/// 端到端：A 用户 @ 对端 Agent → B 网关（cat）执行 → A 收到回复。
#[sqlx::test]
async fn test_federated_delegation_end_to_end(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let base_url = app.serve_on_random_port().await;

    let (org_a, user_a, _jwt_a) = create_node(&app, "dela").await;
    let (org_b, _user_b, _jwt_b) = create_node(&app, "delb").await;
    let gw_id = seed_cli_agent(&org_b, "delb-gw", vec![ROLE_A2A_GATEWAY.to_string()]).await;
    let gw_id_for_b_loop = gw_id.clone();

    let credential = "e".repeat(64);
    // B 侧：入站校验 A 的出站凭证
    seed_link(
        &org_b,
        &org_a,
        "https://peer-a.example.com",
        "unused-b-outbound",
        &sha256::digest(credential.as_bytes()),
        r#"["a2a_task"]"#,
        "delb-link",
    )
    .await;
    // A 侧：出站路由（endpoint = B 节点 /a2a，Bearer = B 所发凭证）
    seed_link(
        &org_a,
        &org_b,
        &format!("{base_url}/a2a"),
        &credential,
        "unused-hash",
        r#"["a2a_task"]"#,
        "dela-link",
    )
    .await;

    // A 侧本地 Agent（消息目标；委派命中后不会被唤醒）
    let local_agent_id = {
        let mut po = AgentPo::new(
            format!("dela-前台-{}", Uuid::now_v7()),
            vec!["reception".to_string()],
            "A 前台".to_string(),
            vec!["chat".to_string()],
            "灵魂".to_string(),
            String::new(),
            "fed-del-seed".to_string(),
        );
        po.status = AgentStatus::Onboarded;
        let id = po.id.clone();
        ai_orz::service::dal::agent::dal()
            .create(
                org_ctx("dela-agent")
                    .to_builder()
                    .organization_id(&org_a)
                    .build(),
                &Agent::from_po(po),
            )
            .await
            .expect("seed local agent failed");
        id
    };

    let prompt = format!("请 [@远端助手](agent:{gw_id}@{org_b}) 帮我回显这句话");
    let a_msg_id = save_user_message(&org_a, &user_a, &local_agent_id, &prompt, "dela-msg").await;

    // A 侧 consumer：直连路由 → 联邦委派（阻塞到轮询到终态）
    let event = {
        let ctx = org_ctx("dela-event");
        let msg = ai_orz::service::dal::message::dal()
            .find_by_id(ctx, &a_msg_id)
            .await
            .expect("find a msg")
            .expect("a msg exists");
        message_event_json(&msg)
    };
    let a_handle = tokio::spawn(async move {
        let consumer = ai_orz::consumer::message::MessageConsumer::new();
        consumer.on_event(RequestContext::new_system(), event).await
    });

    // B 侧 consumer 模拟：轮询发现发给网关的消息并驱动消费（真实 awaken 链路）
    let b_loop = tokio::spawn(async move {
        let ctx = org_ctx("delb-consumer");
        let consumer = ai_orz::consumer::message::MessageConsumer::new();
        for _ in 0..600 {
            let msgs = ai_orz::service::dal::message::dal()
                .list_by_to_id(ctx.clone(), &gw_id_for_b_loop, Some(10))
                .await
                .unwrap_or_default();
            for m in msgs {
                if m.po.status != MessageStatus::Pending {
                    continue;
                }
                let event = message_event_json(&m);
                let _ = consumer.on_event(ctx.clone(), event).await;
                let _ = ai_orz::service::dal::message::dal()
                    .update_status(ctx.clone(), &m.po.id, MessageStatus::Processed)
                    .await;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    a_handle
        .await
        .expect("a consumer task should not panic")
        .expect("federated delegation should succeed");
    b_loop.abort();

    // 断言 1：A 侧收到对端回复（from = 对端网关 Agent，to = 原用户，reply 关联原消息）
    let ctx = org_ctx("assert-a");
    let replies = ai_orz::service::dal::message::dal()
        .list_by_from_id(ctx.clone(), &gw_id, Some(10))
        .await
        .expect("list replies");
    assert!(!replies.is_empty(), "A 侧应收到对端 Agent 的回复消息");
    let reply = replies
        .iter()
        .find(|m| m.po.to_id == user_a && m.po.reply_to_id.as_deref() == Some(a_msg_id.as_str()))
        .expect("reply should target the original user with reply_to_id");
    // cat 网关回显收到的完整 prompt：原文出现在回显内容中（PromptBuilder 组装
    // 后的 prompt 含【消息内容】段，联邦提及也被注入【提及上下文】区块）
    assert!(
        reply.po.content.contains(&prompt),
        "cat gateway echo should contain the original prompt"
    );

    // 断言 2：B 侧 A2A 一次性 project 已自动 complete（否则 tasks/get 永远 working）
    let b_ctx = org_ctx("assert-b");
    let inbox = ai_orz::service::dal::message::dal()
        .list_by_to_id(b_ctx.clone(), &gw_id, Some(10))
        .await
        .expect("list b inbox");
    let inbound = inbox
        .iter()
        .find(|m| m.po.from_role == MessageRole::User)
        .expect("B 侧应有 tasks/send 创建的入站消息");
    let project_id = inbound
        .po
        .project_id
        .clone()
        .expect("inbound carries project_id");
    let project = ai_orz::service::dao::project::dao()
        .find_by_id(b_ctx, &project_id)
        .await
        .expect("query project")
        .expect("project exists");
    assert!(
        matches!(project.status, ::common::enums::ProjectStatus::Completed),
        "A2A one-shot project should auto-complete, got {:?}",
        project.status
    );
}

/// 降级：@ 的 org 无 Active 连接 → 不外呼，走既有流程（目标 Agent 不存在 → NotFound）。
#[sqlx::test]
async fn test_delegation_degrades_when_no_active_link(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, user_a, _jwt_a) = create_node(&app, "dega").await;

    // @ 随机 org id：共享 Storage 下其它用例可能播种任意 Local org 间的连接，
    // 随机 id 保证不存在任何巧合的 Active link，降级路径稳定
    let unknown_org = format!(
        "NOORG{}",
        Uuid::now_v7().simple().to_string()[..8].to_uppercase()
    );
    let msg_id = save_user_message(
        &org_a,
        &user_a,
        "nonexistent-agent",
        &format!("请 [@远端助手](agent:some-agent@{unknown_org}) 帮忙"),
        "dega-msg",
    )
    .await;
    let ctx = org_ctx("dega-event");
    let msg = ai_orz::service::dal::message::dal()
        .find_by_id(ctx, &msg_id)
        .await
        .expect("find msg")
        .expect("msg exists");

    let consumer = ai_orz::consumer::message::MessageConsumer::new();
    let result = consumer
        .on_event(RequestContext::new_system(), message_event_json(&msg))
        .await;
    // 降级后走既有流程：Agent 不存在 → NotFound（证明未委派、未外呼）
    let err = result.expect_err("should fall through to local flow and fail on missing agent");
    assert!(
        err.to_string().contains("not found"),
        "expected NotFound from local flow, got: {}",
        err
    );
}

/// 降级：连接存在但未开放 a2a_task 能力 → 不外呼。
#[sqlx::test]
async fn test_delegation_degrades_when_capability_missing(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;

    let (org_a, user_a, _jwt_a) = create_node(&app, "capa").await;
    let (org_b, _user_b, _jwt_b) = create_node(&app, "capb").await;
    seed_link(
        &org_a,
        &org_b,
        "https://peer-b.example.com/a2a",
        &"f".repeat(64),
        "unused-hash",
        r#"["other_cap"]"#, // 连接有效但不开放 a2a_task
        "capa-link",
    )
    .await;

    let msg_id = save_user_message(
        &org_a,
        &user_a,
        "nonexistent-agent",
        &format!("请 [@远端助手](agent:some-agent@{org_b}) 帮忙"),
        "capa-msg",
    )
    .await;
    let ctx = org_ctx("capa-event");
    let msg = ai_orz::service::dal::message::dal()
        .find_by_id(ctx, &msg_id)
        .await
        .expect("find msg")
        .expect("msg exists");

    let consumer = ai_orz::consumer::message::MessageConsumer::new();
    let result = consumer
        .on_event(RequestContext::new_system(), message_event_json(&msg))
        .await;
    let err = result.expect_err("capability missing → degrade → missing agent NotFound");
    assert!(
        err.to_string().contains("not found"),
        "expected NotFound from local flow, got: {}",
        err
    );
}
