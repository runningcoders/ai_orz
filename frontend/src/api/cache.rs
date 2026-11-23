//! 缓存管理 API - 进程内 TTL 缓存的概览 / 清空 / 清扫

use common::api::{ClearCacheResponse, GetCachesResponse, PruneCacheResponse};

use super::{ApiError, api_get, api_post};

/// 全部已注册缓存的概览
pub async fn get_caches() -> Result<GetCachesResponse, ApiError> {
    api_get("/api/v1/system/cache").await
}

/// 清空指定缓存（下一次请求会重新取数）
pub async fn clear_cache(name: &str) -> Result<ClearCacheResponse, ApiError> {
    api_post(&format!("/api/v1/system/cache/{name}/clear"), &()).await
}

/// 清扫指定缓存的过期条目
pub async fn prune_cache(name: &str) -> Result<PruneCacheResponse, ApiError> {
    api_post(&format!("/api/v1/system/cache/{name}/prune"), &()).await
}
