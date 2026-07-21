//! AopMonitor 子模块实现

use crate::pkg::aop;

use super::{AopMonitor, SystemDomainImpl};

impl AopMonitor for SystemDomainImpl {
    fn all_queue_stats(&self) -> Vec<(String, aop::queue::QueueStats)> {
        aop::registry().all_queue_stats()
    }

    fn queue_stats(&self, consumer_name: &str) -> Option<aop::queue::QueueStats> {
        aop::registry().queue_stats(consumer_name)
    }

    fn list_events(
        &self,
        consumer_name: &str,
        filter: aop::queue::EventQueryFilter,
    ) -> Option<Vec<aop::queue::EventSummary>> {
        aop::registry().query_events(consumer_name, filter)
    }

    fn get_event(&self, consumer_name: &str, event_id: &str) -> Option<aop::queue::EventDetail> {
        aop::registry().get_event(consumer_name, event_id)
    }
}
