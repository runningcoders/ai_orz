//! Handler: 进程内缓存管理（概览 / 清空 / 清扫）
//!
//! 数据全部来自 `pkg::cache` 的进程级注册表，不触碰任何业务库；
//! 管理面只暴露统计与键，不提供按值读取。

use ai_orz_macros::generate_http_handler;
use common::api::{
    CacheNamePath, CacheStatsItem, ClearCacheResponse, GetCachesRequest, GetCachesResponse,
    PruneCacheResponse,
};
use common::error::{Error, Result};

use crate::pkg::RequestContext;
use crate::pkg::cache;

/// 全量缓存概览
#[generate_http_handler]
pub async fn get_caches(
    _ctx: RequestContext,
    _params: GetCachesRequest,
) -> Result<GetCachesResponse> {
    let caches = cache::all_stats().into_iter().map(to_item).collect();
    Ok(GetCachesResponse {
        caches,
        generated_at: common::constants::utils::current_timestamp_ms(),
    })
}

/// 清空指定缓存
#[generate_http_handler]
pub async fn clear_cache(
    _ctx: RequestContext,
    params: CacheNamePath,
) -> Result<ClearCacheResponse> {
    let removed = cache::clear(&params.name)
        .ok_or_else(|| Error::not_found(format!("cache not found: {}", params.name)))?;
    Ok(ClearCacheResponse {
        name: params.name,
        removed: removed as u64,
    })
}

/// 清扫指定缓存的过期条目
#[generate_http_handler]
pub async fn prune_cache(
    _ctx: RequestContext,
    params: CacheNamePath,
) -> Result<PruneCacheResponse> {
    let removed = cache::prune(&params.name)
        .ok_or_else(|| Error::not_found(format!("cache not found: {}", params.name)))?;
    Ok(PruneCacheResponse {
        name: params.name,
        removed: removed as u64,
    })
}

fn to_item(stats: cache::CacheStats) -> CacheStatsItem {
    let counters = stats.counters;
    CacheStatsItem {
        name: stats.name.to_string(),
        ttl_ms: stats.ttl.as_millis() as u64,
        entries: stats.entries as u64,
        expired_entries: stats.expired_entries as u64,
        hits: counters.hits,
        misses: counters.misses,
        expirations: counters.expirations,
        inserts: counters.inserts,
        invalidations: counters.invalidations,
        clears: counters.clears,
        keys: cache::keys(stats.name).unwrap_or_default(),
    }
}
