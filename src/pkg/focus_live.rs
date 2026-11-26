//! 专注浮层实时通道（进程内广播）
//!
//! 链路：思考 / 工具 / 小脑事件 → AOP → `FocusLiveConsumer` → 本模块 broadcast
//! → SSE handler → 前端。
//!
//! 为什么不用消息库：这些是**运行时消息**（思考图的生长过程），断开即丢；
//! 历史可通过思考轨迹查询，落库只会制造一份没人回读的副本。
//!
//! ⚠️ 订阅者晚到就收不到已经发生的事件（broadcast 无回放）。前端重连后由
//! 「当前思考轨迹」兜底，符合「实时图 + 历史轨迹」两层口径。
//!
//! 通道按 `agent_id` 隔离：一个 Agent 的事件流不会挤占另一个 Agent 的缓冲，
//! SSE 端点也不必再先收全量再按字段过滤。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::sync::broadcast;

/// 广播缓冲：单 Agent 一轮思考的事件量远小于此；溢出时丢弃最旧（订阅者收到 Lagged）
const CAPACITY: usize = 512;

/// Agent id → 该 Agent 的广播通道
type Channels = Mutex<HashMap<String, broadcast::Sender<String>>>;

static CHANNELS: OnceLock<Channels> = OnceLock::new();

fn channels() -> &'static Channels {
    CHANNELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 取（或创建）某个 Agent 的广播发送端
fn sender(agent_id: &str) -> broadcast::Sender<String> {
    let mut map = channels()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    map.entry(agent_id.to_string())
        .or_insert_with(|| broadcast::channel(CAPACITY).0)
        .clone()
}

/// 发布一条已序列化的事件（无订阅者时静默丢弃，与 SSE 语义一致）
///
/// 最后一个订阅者断开后顺手释放该 Agent 的通道：广播缓冲是预分配的，
/// 长期运行时若按历史 Agent 数一直累积，会变成一份无人使用的常驻内存。
pub fn publish(agent_id: &str, json: String) {
    let mut map = channels()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let sender = map
        .entry(agent_id.to_string())
        .or_insert_with(|| broadcast::channel(CAPACITY).0);
    let _ = sender.send(json);
    if sender.receiver_count() == 0 {
        map.remove(agent_id);
    }
}

/// 订阅某个 Agent 的实时事件流
pub fn subscribe(agent_id: &str) -> broadcast::Receiver<String> {
    sender(agent_id).subscribe()
}
