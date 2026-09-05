//! 联邦 WS 请求-响应配对表（correlation_id → oneshot）
//!
//! 发起侧注册 pending → 出站 push → 响应帧由 session 截获唤醒；
//! 等待侧带超时兜底（事件/连接丢失时不永久悬挂）。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;

use common::error::{Result, err};

/// 默认响应超时：send_task 是异步提交（返回 task 创建结果），30s 覆盖公网 RTT
/// pending 表
pub struct PendingTable {
    entries: Mutex<HashMap<String, oneshot::Sender<Result<Value>>>>,
    /// 默认响应超时（秒）
    pub default_timeout_secs: u64,
}

static TABLE: OnceLock<PendingTable> = OnceLock::new();

/// 全局 pending 表单例
pub fn pending() -> &'static PendingTable {
    TABLE.get_or_init(|| PendingTable {
        entries: Mutex::new(HashMap::new()),
        default_timeout_secs: 30,
    })
}

impl PendingTable {
    /// 注册等待项（correlation_id 已由调用方生成）
    pub fn register(&self, correlation_id: &str) -> oneshot::Receiver<Result<Value>> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut guard) = self.entries.lock() {
            guard.insert(correlation_id.to_string(), tx);
        }
        rx
    }

    /// 唤醒等待方（响应帧到达时由 session 调用）
    ///
    /// 返回是否成功唤醒（false = 无此 pending，可能是超时后迟到的响应）。
    pub fn resolve(&self, correlation_id: &str, result: Result<Value>) -> bool {
        let sender = {
            let Ok(mut guard) = self.entries.lock() else {
                return false;
            };
            guard.remove(correlation_id)
        };
        match sender {
            Some(tx) => tx.send(result).is_ok(),
            None => false,
        }
    }

    /// 清理等待项（超时/出站失败时调用，防泄漏）
    pub fn remove(&self, correlation_id: &str) {
        if let Ok(mut guard) = self.entries.lock() {
            guard.remove(correlation_id);
        }
    }

    /// 等待响应（带超时兜底；超时/失败自动清理 pending）
    pub async fn wait(
        &self,
        correlation_id: &str,
        rx: oneshot::Receiver<Result<Value>>,
        timeout: Duration,
    ) -> Result<Value> {
        let result = tokio::time::timeout(timeout, rx).await;
        // 无论超时还是正常唤醒，都确保表内无残留
        self.remove(correlation_id);
        match result {
            Ok(Ok(Ok(payload))) => Ok(payload),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(err!(Internal, "federation pending dropped")),
            Err(_) => Err(err!(
                ThirdPartyError,
                "federation peer response timeout ({}s): correlation_id={}",
                timeout.as_secs(),
                correlation_id
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 正常配对：注册 → resolve 唤醒 → wait 返回 payload，表内无残留
    #[tokio::test]
    async fn register_resolve_wait_roundtrip() {
        let table = PendingTable {
            entries: Mutex::new(HashMap::new()),
            default_timeout_secs: 1,
        };
        let rx = table.register("c1");
        assert!(table.resolve("c1", Ok(json!({"ok": true}))));
        let payload = table.wait("c1", rx, Duration::from_secs(1)).await.unwrap();
        assert_eq!(payload, json!({"ok": true}));
    }

    /// 迟到响应：无 pending 时 resolve 返回 false（不 panic）
    #[tokio::test]
    async fn late_response_is_ignored() {
        let table = PendingTable {
            entries: Mutex::new(HashMap::new()),
            default_timeout_secs: 1,
        };
        assert!(!table.resolve("ghost", Ok(json!(null))));
    }

    /// 超时兜底：wait 超时报错并自动清理 pending，之后迟到 resolve 返回 false
    #[tokio::test]
    async fn wait_timeout_cleans_up() {
        let table = PendingTable {
            entries: Mutex::new(HashMap::new()),
            default_timeout_secs: 1,
        };
        let rx = table.register("c2");
        let result = table.wait("c2", rx, Duration::from_millis(50)).await;
        assert!(result.is_err());
        // 超时后 pending 已清理：迟到响应无人接收
        assert!(!table.resolve("c2", Ok(json!(null))));
    }
}
