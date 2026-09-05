//! 联邦 WS 连接注册表
//!
//! peer_org → 活连接出站句柄。入站命令的业务 ctx 由 consumer 按事件经
//! domain 解析（接待用户 + caller_org），注册表只管连接，**帧信封内
//! 绝不携带身份字段**（P0 红线）。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};

use crate::pkg::ws::FrameTx;

/// 联邦 WS 连接注册表
pub struct FederationWsRegistry {
    sessions: RwLock<HashMap<String, Arc<dyn FrameTx>>>,
    /// 拨号防重入（后台 best-effort 拨号进行中的对端集合）
    dialing: RwLock<HashSet<String>>,
}

static REGISTRY: OnceLock<FederationWsRegistry> = OnceLock::new();

/// 全局注册表单例
pub fn registry() -> &'static FederationWsRegistry {
    REGISTRY.get_or_init(|| FederationWsRegistry {
        sessions: RwLock::new(HashMap::new()),
        dialing: RwLock::new(HashSet::new()),
    })
}

impl FederationWsRegistry {
    /// 注册/覆盖会话（同对端重拨即覆盖旧条目）
    pub fn register(&self, peer_org: &str, tx: Arc<dyn FrameTx>) {
        if let Ok(mut guard) = self.sessions.write() {
            guard.insert(peer_org.to_string(), tx);
        }
    }

    /// 查询对端出站句柄（连接已死返回 None）
    pub fn lookup(&self, peer_org: &str) -> Option<Arc<dyn FrameTx>> {
        let guard = self.sessions.read().ok()?;
        let tx = guard.get(peer_org)?;
        tx.is_alive().then(|| tx.clone())
    }

    /// 对端是否有活连接
    pub fn connected(&self, peer_org: &str) -> bool {
        self.lookup(peer_org).is_some()
    }

    /// 标记拨号进行中（防重入：已有拨号在途返回 false）
    ///
    /// 调用方负责在拨号结束（成功或失败）后 `clear_dialing`。
    pub fn try_mark_dialing(&self, peer_org: &str) -> bool {
        match self.dialing.write() {
            Ok(mut guard) => guard.insert(peer_org.to_string()),
            Err(_) => false,
        }
    }

    /// 清除拨号标记
    pub fn clear_dialing(&self, peer_org: &str) {
        if let Ok(mut guard) = self.dialing.write() {
            guard.remove(peer_org);
        }
    }

    /// 注销会话（仅当当前条目即该句柄时移除，防止误删重拨后的新条目）
    pub fn remove_if(&self, peer_org: &str, tx: &Arc<dyn FrameTx>) {
        if let Ok(mut guard) = self.sessions.write() {
            let remove = guard
                .get(peer_org)
                .map(|t| Arc::ptr_eq(t, tx))
                .unwrap_or(false);
            if remove {
                guard.remove(peer_org);
            }
        }
    }
}
