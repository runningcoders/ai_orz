use crate::pkg::aop::{Event, EventKind};
use serde::{Deserialize, Serialize};

/// 组织目录元信息变更事件
///
/// 由 Organization DAL 层 `create` / `update` / `delete` 在写库成功后发布。
/// 仅覆盖目录同步白名单字段的载体（组织本身的元信息），Remote/Linked 影子的
/// 写入走 link DAO（`upsert_peer_org` 等），**不发布本事件**——天然避免
/// "收对端推送 → 写影子 → 再触发推送" 的递归。
///
/// 订阅者：
/// - FederationDirectoryConsumer：异步消费，把本地目录全量推给所有 Active 对端
///   （best-effort，失败由下一次 cron 对账补齐）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationChangedEvent {
    pub event_id: String,
    pub organization_id: String,
    /// 变更类型：created / updated / deleted（目前消费者不区分，全量推送幂等收敛）
    pub change: String,
    pub created_at: i64,
}

impl OrganizationChangedEvent {
    pub fn new(organization_id: &str, change: &str) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            organization_id: organization_id.to_string(),
            change: change.to_string(),
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for OrganizationChangedEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("organization.changed")
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.organization_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
