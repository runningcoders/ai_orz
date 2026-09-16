use common::enums::EventTopic;
use serde::{Serialize, de::DeserializeOwned};

pub trait Event: Send + Sync + Clone + Serialize + DeserializeOwned + 'static {
    /// 事件主题（`common::enums::EventTopic`）—— 路由与生产者归属反查的唯一依据
    fn kind(&self) -> EventTopic;
    fn id(&self) -> &str;
    fn order_key(&self) -> &str {
        ""
    }
    fn priority(&self) -> u8 {
        0
    }
    fn created_at(&self) -> i64 {
        0
    }
}
