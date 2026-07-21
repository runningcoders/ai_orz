use crate::pkg::aop;

/// AOP 监控接口
pub trait AopMonitor: Send + Sync {
    /// 获取所有队列的聚合统计
    fn all_queue_stats(&self) -> Vec<(String, aop::queue::QueueStats)>;

    /// 获取指定消费者队列统计
    fn queue_stats(&self, consumer_name: &str) -> Option<aop::queue::QueueStats>;

    /// 查询队列中的事件列表
    fn list_events(&self, consumer_name: &str, filter: aop::queue::EventQueryFilter) -> Option<Vec<aop::queue::EventSummary>>;

    /// 获取事件详情
    fn get_event(&self, consumer_name: &str, event_id: &str) -> Option<aop::queue::EventDetail>;
}

/// AOP 监控实现
pub struct AopMonitorImpl;

impl AopMonitor for AopMonitorImpl {
    fn all_queue_stats(&self) -> Vec<(String, aop::queue::QueueStats)> {
        aop::registry().all_queue_stats()
    }

    fn queue_stats(&self, consumer_name: &str) -> Option<aop::queue::QueueStats> {
        aop::registry().queue_stats(consumer_name)
    }

    fn list_events(&self, consumer_name: &str, filter: aop::queue::EventQueryFilter) -> Option<Vec<aop::queue::EventSummary>> {
        aop::registry().query_events(consumer_name, filter)
    }

    fn get_event(&self, consumer_name: &str, event_id: &str) -> Option<aop::queue::EventDetail> {
        aop::registry().get_event(consumer_name, event_id)
    }
}
