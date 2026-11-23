//! 进程内缓存管理 DTO（前后端共用契约）
//!
//! 管理面只暴露「统计 + 键 + 清理」，不提供按值读取入口：
//! 缓存值可能携带敏感信息，查询走各业务自己的接口，不走管理页。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 缓存概览请求（无参数）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, Params)]
pub struct GetCachesRequest {}

/// 单个缓存的只读快照
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CacheStatsItem {
    /// 注册名（进程内唯一）
    pub name: String,
    /// TTL（毫秒）
    pub ttl_ms: u64,
    /// 当前条目数（含尚未清扫的过期条目）
    pub entries: u64,
    /// 其中已过期的条目数
    pub expired_entries: u64,
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 过期移除次数
    pub expirations: u64,
    /// 写入次数
    pub inserts: u64,
    /// 主动失效次数
    pub invalidations: u64,
    /// 清空次数
    pub clears: u64,
    /// 键列表（排序 + 截断；用于确认缓存内容范围）
    pub keys: Vec<String>,
}

/// 缓存概览响应
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GetCachesResponse {
    /// 全部已注册缓存（按名称排序）
    pub caches: Vec<CacheStatsItem>,
    /// 快照时间（毫秒）
    pub generated_at: i64,
}

/// 缓存名称路径参数
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, Params)]
pub struct CacheNamePath {
    /// 缓存注册名
    #[param(source = "path")]
    pub name: String,
}

/// 清空缓存响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ClearCacheResponse {
    /// 缓存注册名
    pub name: String,
    /// 移除的条目数
    pub removed: u64,
}

/// 清扫过期缓存响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PruneCacheResponse {
    /// 缓存注册名
    pub name: String,
    /// 移除的过期条目数
    pub removed: u64,
}
