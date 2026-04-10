//! Event 相关类型定义
//!
//! 所有可放入事件总线的事件基础类型定义

use std::cmp::Ordering;

/// 事件类型枚举
///
/// 预定义系统事件类型，支持扩展自定义类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    /// Agent 消息事件
    Message,
    /// 任务状态变更事件
    TaskChange,
    /// 自定义扩展事件
    Custom(u16),
}

/// 事件引用，用于堆排序和队列存储
///
/// 本体存储在全局 HashMap，堆和队列只存引用
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRef {
    /// 事件唯一 ID
    pub event_id: String,
    /// 排序分组键
    pub order_key: String,
    /// 优先级（数值越大优先级越高）
    pub priority: u8,
    /// 创建时间戳（秒）
    pub created_at: i64,
}

impl Ord for EventRef {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap 是最大堆，所以优先级高的在前，同优先级创建时间早的在前
        self.priority.cmp(&other.priority)
            .then_with(|| other.created_at.cmp(&self.created_at).reverse())
    }
}

impl PartialOrd for EventRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 事件基础能力 trait
///
/// 所有可放入事件总线的事件都需要实现此 trait
/// 需要 Clone 支持，因为 dequeue 返回克隆事件，原事件保留在队列直到 ack
pub trait Event: Send + Sync + Clone {
    /// 事件唯一 ID
    fn id(&self) -> &str;

    /// 事件类型（用于区分不同业务事件，消费者可按类型过滤）
    fn event_type(&self) -> EventType;

    /// 排序分组键 - 相同 order_key 的消息必须顺序消费
    /// 空字符串表示不需要分组，可并行消费
    fn order_key(&self) -> &str;

    /// 优先级 - 高优先级优先消费（数值越大优先级越高）
    /// 0-9，默认 5
    fn priority(&self) -> u8 {
        5
    }

    /// 创建时间戳（秒），同优先级按创建时间升序消费
    fn created_at(&self) -> i64;

    /// 生成用于堆排序和队列存储的 EventRef
    fn to_event_ref(&self) -> EventRef {
        EventRef {
            event_id: self.id().to_string(),
            order_key: self.order_key().to_string(),
            priority: self.priority(),
            created_at: self.created_at(),
        }
    }
}
