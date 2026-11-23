//! Handler: GET /api/v1/system/scene/projection - 工作台场景投影
//!
//! 本 handler 只做取数与编排：并从各域取事实，交给 `system::scene_projection`
//! 做纯推导。语义判定不在这里写，否则 v2 接入事件流时要再实现一遍。
//!
//! 数据源：
//! - 运行态：runtime 域内存态（权威实时）
//! - 展示名：hr 域 Agent 列表（失败降级为 id）
//! - 等待判定：message 域近 24h 窗口查询（失败降级为「不等待」，宁漏不刷屏）
//! - 队列积压：system 域 AOP monitor 内存态

use ai_orz_macros::generate_http_handler;
use common::api::{GetSceneProjectionRequest, SceneProjectionResponse};
use common::enums::MessageRole;
use common::error::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::pkg::RequestContext;
use crate::service::dao::message::MessageQuery;
use crate::service::domain::hr::agent_profile;
use crate::service::domain::message::domain as message_domain;
use crate::service::domain::runtime::domain as runtime_domain;
use crate::service::domain::system::domain as system_domain;
use crate::service::domain::system::scene_projection::{
    DEFAULT_SEAT_LIMIT, PresenceFacts, SceneInput, derive,
};

/// 进程内投影序号：v2 事件流用它对齐增量；重启归零，前端不以它判断新鲜度
static PROJECTION_SEQ: AtomicU64 = AtomicU64::new(1);
/// 等待判定的回看窗口
const WAITING_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
/// 单次扫描上限：只取最近几百条，够判定「谁欠回复」，不做全量统计
const MESSAGE_SCAN_LIMIT: usize = 500;

/// 单个 Agent 的等待事实
#[derive(Debug, Clone, Default)]
struct WaitingFacts {
    last_agent_message_at: Option<i64>,
    last_user_message_at: Option<i64>,
    /// 最近一条待回复消息是否在任务上下文内（无 task_id 的消息不触发 NeedsYou）
    is_task_context: bool,
}

/// 查询工作台场景投影
#[generate_http_handler]
pub async fn get_scene_projection(
    ctx: RequestContext,
    params: GetSceneProjectionRequest,
) -> Result<SceneProjectionResponse> {
    let now = common::constants::utils::current_timestamp_ms();
    let seat_limit = params.seat_limit.unwrap_or(DEFAULT_SEAT_LIMIT);

    let runtime_agents = runtime_domain().list_runtime_agents(None, None, None);

    // 展示名：复用 Agent 档案缓存；失败只影响文案，不影响判定
    let names = match agent_profile::names(&ctx).await {
        Ok(names) => names,
        Err(e) => {
            log_error!(&ctx, "get_scene_projection", "agent names degraded: {}", e);
            HashMap::new()
        }
    };

    let waiting = collect_waiting(&ctx, now).await;

    let facts = runtime_agents
        .into_iter()
        .map(|(agent_id, info)| {
            let fact = waiting.get(&agent_id);
            PresenceFacts {
                display_name: names
                    .get(&agent_id)
                    .cloned()
                    .unwrap_or_else(|| agent_id.clone()),
                agent_id,
                runtime_state: info.state,
                state_started_at: info.state_started_at,
                last_agent_message_at: fact.and_then(|f| f.last_agent_message_at),
                last_user_message_at: fact.and_then(|f| f.last_user_message_at),
                is_task_context: fact.is_some_and(|f| f.is_task_context),
            }
        })
        .collect();

    let queue_backlog = system_domain()
        .aop_monitor()
        .all_queue_stats()
        .iter()
        .fold(0u64, |acc, (_, stats)| {
            acc.saturating_add(stats.pending_count as u64)
        });

    let input = SceneInput {
        now,
        seat_limit,
        queue_backlog,
        facts,
    };
    let seq = PROJECTION_SEQ.fetch_add(1, Ordering::Relaxed);

    Ok(derive(input, seq))
}

/// 一次窗口扫描产出按 Agent 聚合的等待事实
///
/// 只做两件事：谁最近给用户发了消息、用户最近回了谁。两次查询都失败时返回空表，
/// 判定退化为「全部不等待」——宁可漏报，不可刷屏。
async fn collect_waiting(ctx: &RequestContext, now: i64) -> HashMap<String, WaitingFacts> {
    let mut map: HashMap<String, WaitingFacts> = HashMap::new();
    let window_start = now.saturating_sub(WAITING_WINDOW_MS);

    // Agent → 用户：取每个 Agent 最近一条，并记录它是否属于任务上下文
    match message_domain()
        .management()
        .query(
            ctx.clone(),
            MessageQuery {
                to_role: Some(MessageRole::User),
                created_after: Some(window_start),
                limit: Some(MESSAGE_SCAN_LIMIT),
                order_by: Some("created_at DESC".to_string()),
                ..Default::default()
            },
        )
        .await
    {
        Ok(messages) => {
            for message in messages {
                let entry = map.entry(message.po.from_id.clone()).or_default();
                if entry.last_agent_message_at.is_none() {
                    entry.last_agent_message_at = Some(message.po.created_at);
                    entry.is_task_context = message.po.task_id.is_some();
                }
            }
        }
        Err(e) => {
            log_error!(
                ctx,
                "get_scene_projection",
                "agent->user window degraded: {}",
                e
            );
        }
    }

    // 用户 → Agent：取用户最近一次回复时间
    match message_domain()
        .management()
        .query(
            ctx.clone(),
            MessageQuery {
                from_role: Some(MessageRole::User),
                created_after: Some(window_start),
                limit: Some(MESSAGE_SCAN_LIMIT),
                order_by: Some("created_at DESC".to_string()),
                ..Default::default()
            },
        )
        .await
    {
        Ok(messages) => {
            for message in messages {
                let entry = map.entry(message.po.to_id.clone()).or_default();
                if entry.last_user_message_at.is_none() {
                    entry.last_user_message_at = Some(message.po.created_at);
                }
            }
        }
        Err(e) => {
            log_error!(
                ctx,
                "get_scene_projection",
                "user->agent window degraded: {}",
                e
            );
        }
    }

    map
}
