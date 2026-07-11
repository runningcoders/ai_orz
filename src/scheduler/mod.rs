//! Cron 调度器模块
//!
//! CronScheduler 是一个后台任务，每分钟扫描一次数据库，
//! 找到所有到期的触发器（next_run_at <= now AND is_enabled = 1），
//! 然后将这些触发器投递到事件队列中，由消费者处理。

use crate::models::cron_trigger::CronTriggerPo;
use crate::models::event::{Event, EventTopic};
use crate::pkg::RequestContext;
use common::constants::utils;
use std::sync::{Arc, OnceLock};

// ==================== CronTriggerEvent 事件类型 ====================

/// Cron 触发器事件
///
/// 当 CronScheduler 扫描到到期触发器时，会创建此事件并推入事件队列，
/// 由消费者负责实际执行触发器的业务逻辑。
#[derive(Debug, Clone)]
pub struct CronTriggerEvent {
    /// 事件唯一 ID（使用 trigger_id + 时间戳确保唯一）
    event_id: String,
    /// 触发器 ID
    pub trigger_id: String,
    /// 触发器名称
    pub trigger_name: String,
    /// 触发器类型
    pub trigger_type: common::enums::TriggerType,
    /// 触发器 payload（JSON 字符串，包含 action 等配置）
    pub payload: String,
    /// 本次执行时间（秒级时间戳）
    pub fired_at: i64,
    /// 创建时间
    created_at: i64,
}

impl CronTriggerEvent {
    /// 从 CronTriggerPo 创建事件
    pub fn from_trigger(trigger: &CronTriggerPo, fired_at: i64) -> Self {
        let event_id = format!("{}-{}", trigger.id, fired_at);
        Self {
            event_id,
            trigger_id: trigger.id.clone(),
            trigger_name: trigger.name.clone(),
            trigger_type: trigger.trigger_type,
            payload: trigger.payload.clone(),
            fired_at,
            created_at: utils::current_timestamp(),
        }
    }
}

impl Event for CronTriggerEvent {
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn topic(&self) -> EventTopic {
        EventTopic::CronTrigger
    }

    fn order_key(&self) -> &str {
        // 按触发器 ID 分组，同一触发器的事件保证顺序消费
        &self.trigger_id
    }

    fn priority(&self) -> u8 {
        5
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}

// ==================== CronScheduler 配置 ====================

/// CronScheduler 配置
#[derive(Debug, Clone)]
pub struct CronSchedulerConfig {
    /// 扫描间隔（秒），默认 60 秒
    pub scan_interval_secs: u64,
    /// 每次扫描最多获取的触发器数量，默认 100
    pub scan_limit: i32,
}

impl Default for CronSchedulerConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: 60,
            scan_limit: 100,
        }
    }
}

// ==================== CronScheduler 实现 ====================

/// Cron 调度器
///
/// 后台运行，定时扫描数据库中的到期触发器，
/// 将其推入事件队列等待消费者处理。
#[derive(Debug)]
pub struct CronScheduler {
    config: CronSchedulerConfig,
}

impl CronScheduler {
    /// 创建新的 CronScheduler
    pub fn new(config: CronSchedulerConfig) -> Self {
        Self { config }
    }

    /// 启动调度器（永久运行在后台任务中）
    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            self.run_loop().await;
        });
    }

    /// 主循环
    async fn run_loop(&self) {
        log_info!("cron scheduler started, interval: {}s", self.config.scan_interval_secs);

        loop {
            if let Err(e) = self.scan_once().await {
                log_error!("cron scheduler scan error: {}", e);
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(self.config.scan_interval_secs)).await;
        }
    }

    /// 执行一次扫描
    async fn scan_once(&self) -> common::error::Result<()> {
        let ctx = RequestContext::new(None, None);
        let now = utils::current_timestamp();

        // 获取到期触发器
        let triggers = crate::service::domain::system::domain()
            .cron_manager()
            .list_due_triggers(ctx.clone(), now, self.config.scan_limit)
            .await?;

        if triggers.is_empty() {
            return Ok(());
        }

        log_debug!("cron scheduler found {} due triggers", triggers.len());

        let event_queue = crate::service::dao::event_queue::cron_trigger_dao();
        let mut events = Vec::with_capacity(triggers.len());

        for trigger in &triggers {
            // 创建事件
            let event = CronTriggerEvent::from_trigger(trigger, now);
            events.push(Box::new(event));

            // 更新触发器下次执行时间
            crate::service::domain::system::domain()
                .cron_manager()
                .mark_trigger_executed(ctx.clone(), &trigger.id, now)
                .await?;
        }

        // 批量入队
        event_queue.enqueue_batch(ctx, events)?;

        log_info!("cron scheduler enqueued {} trigger events", triggers.len());

        Ok(())
    }
}

// ==================== 单例 ====================

static CRON_SCHEDULER: OnceLock<Arc<CronScheduler>> = OnceLock::new();

/// 获取 CronScheduler 单例
pub fn scheduler() -> Arc<CronScheduler> {
    CRON_SCHEDULER.get().cloned().unwrap()
}

/// 初始化并启动 CronScheduler
pub fn init(config: Option<CronSchedulerConfig>) {
    let config = config.unwrap_or_default();
    let scheduler = Arc::new(CronScheduler::new(config));
    let _ = CRON_SCHEDULER.set(scheduler.clone());
    scheduler.start();
}
