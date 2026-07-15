//! SSE 推送 DAO
//!
//! SSE 连接管理 + 消息推送

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use tokio::sync::{RwLock, broadcast};
use async_trait::async_trait;
use common::error::Result;
use crate::pkg::RequestContext;

#[derive(Debug, Clone)]
pub struct SsePushResult {
    pub success: bool,
    pub delivered_count: usize,
    pub error: Option<String>,
}

#[async_trait]
pub trait SsePushDao: Send + Sync {
    async fn push(
        &self,
        ctx: RequestContext,
        user_id: &str,
        payload: &str,
    ) -> Result<SsePushResult>;

    async fn register(
        &self,
        ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> broadcast::Receiver<String>;

    async fn unregister(&self, ctx: RequestContext, connection_id: &str);

    async fn connection_count(&self, ctx: RequestContext, user_id: &str) -> usize;
}

pub struct SsePushDaoImpl {
    connections: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
    user_connections: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl SsePushDaoImpl {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            user_connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for SsePushDaoImpl {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl SsePushDao for SsePushDaoImpl {
    async fn push(
        &self,
        _ctx: RequestContext,
        user_id: &str,
        payload: &str,
    ) -> Result<SsePushResult> {
        let user_connections = self.user_connections.read().await;
        let connection_ids = user_connections.get(user_id).cloned().unwrap_or_default();
        let connections = self.connections.read().await;

        let mut success_count = 0;
        for conn_id in connection_ids {
            if let Some(tx) = connections.get(&conn_id) {
                if tx.send(payload.to_string()).is_ok() {
                    success_count += 1;
                }
            }
        }

        Ok(SsePushResult {
            success: success_count > 0,
            delivered_count: success_count,
            error: None,
        })
    }

    async fn register(
        &self,
        _ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> broadcast::Receiver<String> {
        let (tx, rx) = broadcast::channel(100);
        self.connections.write().await.insert(connection_id.to_string(), tx);
        self.user_connections
            .write()
            .await
            .entry(user_id.to_string())
            .or_insert_with(HashSet::new)
            .insert(connection_id.to_string());
        rx
    }

    async fn unregister(&self, _ctx: RequestContext, connection_id: &str) {
        if let Some(_) = self.connections.write().await.remove(connection_id) {
            let mut user_connections = self.user_connections.write().await;
            for (_, conn_set) in user_connections.iter_mut() {
                conn_set.remove(connection_id);
            }
        }
    }

    async fn connection_count(&self, _ctx: RequestContext, user_id: &str) -> usize {
        self.user_connections
            .read()
            .await
            .get(user_id)
            .map(|s| s.len())
            .unwrap_or(0)
    }
}

pub fn dao() -> Arc<dyn SsePushDao> {
    static DAO: OnceLock<Arc<dyn SsePushDao>> = OnceLock::new();
    DAO.get_or_init(|| Arc::new(SsePushDaoImpl::new())).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn init_test_storage() {
        crate::pkg::storage::test_support::init_for_test().await;
    }

    fn new_ctx() -> RequestContext {
        RequestContext::builder().build()
    }

    #[tokio::test]
    async fn test_register_and_unregister() {
        init_test_storage().await;
        let dao = SsePushDaoImpl::new();
        let ctx = new_ctx();
        let _rx = dao.register(ctx.clone(), "user_1", "conn_1").await;
        assert_eq!(dao.connection_count(ctx.clone(), "user_1").await, 1);
        dao.unregister(ctx.clone(), "conn_1").await;
        assert_eq!(dao.connection_count(ctx.clone(), "user_1").await, 0);
    }

    #[tokio::test]
    async fn test_push() {
        init_test_storage().await;
        let dao = SsePushDaoImpl::new();
        let ctx = new_ctx();
        let mut rx = dao.register(ctx.clone(), "user_1", "conn_1").await;
        let result = dao.push(ctx.clone(), "user_1", "hello").await.unwrap();
        assert_eq!(result.success, true);
        assert_eq!(result.delivered_count, 1);
        assert_eq!(rx.try_recv().unwrap(), "hello");
    }
}
