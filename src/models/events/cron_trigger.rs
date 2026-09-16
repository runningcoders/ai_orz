use crate::pkg::aop::Event;
use common::enums::EventTopic;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTriggerEvent {
    pub event_id: String,
    pub trigger_id: String,
    pub trigger_name: String,
    pub payload: String,
    pub created_at: i64,
}

impl Event for CronTriggerEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::CronTrigger
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.trigger_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
