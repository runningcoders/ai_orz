//! 联邦目录推送消费者
//!
//! 订阅 `organization.changed` 事件（Organization DAL 在组织创建/更新/删除
//! 落库后发布），把本地目录全量推给所有 Active 对端（best-effort）。
//!
//! 与 `CronTriggerConsumer` 的 `directory_reconcile` action 同源：两者最终都
//! 调 `OrganizationManage::push_directory_to_peers` / `reconcile_directories`，
//! 推送保证时效，对账保证最终一致。

use async_trait::async_trait;
use common::error::Result;

use crate::models::events::OrganizationChangedEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::domain::organization;

pub struct FederationDirectoryConsumer;

impl Default for FederationDirectoryConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl FederationDirectoryConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for FederationDirectoryConsumer {
    fn name(&self) -> &str {
        "federation_directory"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("organization.changed")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: OrganizationChangedEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!(
                "failed to deserialize OrganizationChangedEvent: {}",
                e
            ))
        })?;

        sys_debug!(
            "organization.changed received: org={} change={}",
            event.organization_id,
            event.change
        );

        // best-effort：单个对端失败已在 domain 内记 WARN（由 cron 对账补齐），
        // 这里整目录构建失败等错误也只告警，不向事件管道传播。
        if let Err(e) = organization::domain()
            .organization_manage()
            .push_directory_to_peers(ctx)
            .await
        {
            sys_warn!("目录变更推送失败(组织 {}): {}", event.organization_id, e);
        }

        Ok(())
    }
}
