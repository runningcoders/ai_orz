//! 联邦端点可达性解析器（P7 内外网可达性）
//!
//! 三层模型之②层：从对端自报地址候选池（③层 `link.endpoint` 归入公网组）
//! 中探测出「当前首选可达地址」。探测语义（方案 §七 拍板）：
//!
//! - **first-match 即停**：内网（private）候选按序探测，首个连通即用，
//!   不做全量扫描；内网全不通再回退公网组
//! - **TTL 缓存**：解析结果（含选定地址）缓存 TTL，期间零探测开销；
//!   「全不通」同样缓存，避免对不可达对端反复白付探测延迟
//! - **全不通 = 维持主地址**：交给出站 HTTP 自然失败（既有错误路径 /
//!   降级惯例不变），解析层不改变失败语义
//! - **快速路径**：对端无自报地址（旧版本 / 未同步）→ 原样返回主地址，
//!   零探测零缓存，行为与 P7 之前完全一致
//!
//! 无状态外置：探测结果纯内存缓存（进程内），不碰 DB；`link.endpoint`
//! 的回写（前端展示「当前在用哪条路」）留待真实需求（YAGNI）。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use common::api::organization_link::FederationAddress;
use tokio::net::TcpStream;

/// 单个地址探测超时：TCP connect 500ms（覆盖内网 + 同区域公网 RTT）
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// 解析结果缓存 TTL：60s 内不重复探测。
/// 感知延迟权衡：对端内网恢复/切换的感知延迟 ≤ TTL（方案 §七 已接受）。
const RESOLVE_TTL: Duration = Duration::from_secs(60);

/// 解析器单例（纯内存组件，无 DB 依赖，与 `http::client()` 同款惰性构建）
pub fn resolver() -> &'static ReachabilityResolver {
    static RESOLVER: OnceLock<ReachabilityResolver> = OnceLock::new();
    RESOLVER.get_or_init(ReachabilityResolver::new)
}

/// 候选地址排序（纯函数，可单测）：内网优先、公网回退，主地址为公网组首位
///
/// 返回 (内网候选, 公网候选)；组内保持自报顺序（稳定性优于智能排序，YAGNI）。
pub fn candidate_order(
    primary_endpoint: &str,
    addresses: &[FederationAddress],
) -> (Vec<String>, Vec<String>) {
    let mut private = Vec::new();
    let mut public = vec![primary_endpoint.to_string()];
    for addr in addresses {
        let url = addr.url.trim().trim_end_matches('/').to_string();
        if url.is_empty() || url == primary_endpoint {
            continue;
        }
        if addr.is_private() {
            private.push(url);
        } else {
            public.push(url);
        }
    }
    (private, public)
}

/// 从 URL 提取 (host, port)（探测用 TCP 连接目标；无 url crate 依赖，手写解析）
///
/// 支持 `http(s)://host[:port]` 与 `host[:port]`；缺省端口按 scheme 推导
/// （https=443 / 其余=80）。
pub fn parse_host_port(endpoint: &str) -> Option<(String, u16)> {
    let rest = endpoint
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(endpoint);
    let authority = rest.split(['/']).next()?;
    if authority.is_empty() {
        return None;
    }

    // IPv6 字面量 [::1]:8080
    if let Some(inner) = authority.strip_prefix('[') {
        let (host, tail) = inner.split_once(']')?;
        let port = tail
            .strip_prefix(':')
            .and_then(|p| p.parse().ok())
            .unwrap_or(80);
        return Some((host.to_string(), port));
    }

    match authority.rsplit_once(':') {
        // host:port（port 非数字视为 host 一部分，如 example.com:path 形态由上游校验拦截）
        Some((host, port)) => port.parse::<u16>().ok().map(|p| (host.to_string(), p)),
        None => Some((
            authority.to_string(),
            if endpoint.starts_with("https") {
                443
            } else {
                80
            },
        )),
    }
}

/// TCP 探测：连通 = 地址可达（应用层健康由出站 HTTP 调用本身验证）
pub async fn probe(endpoint: &str) -> bool {
    let Some((host, port)) = parse_host_port(endpoint) else {
        return false;
    };
    let target: SocketAddr = match tokio::net::lookup_host((host.as_str(), port)).await {
        // 多地址（IPv4/IPv6 双栈）任取首个即可——探测目的只是「选路」
        Ok(mut addrs) => match addrs.next() {
            Some(addr) => addr,
            None => return false,
        },
        Err(_) => return false,
    };
    tokio::time::timeout(PROBE_TIMEOUT, TcpStream::connect(target))
        .await
        .map(|inner| inner.is_ok())
        .unwrap_or(false)
}

/// 解析结果缓存条目：选定地址 + 时刻
#[derive(Clone)]
struct CacheEntry {
    chosen: String,
    resolved_at: Instant,
}

/// 可达性解析器（进程内单例，纯内存缓存）
pub struct ReachabilityResolver {
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl Default for ReachabilityResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ReachabilityResolver {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// 解析对端当前首选可达地址
    ///
    /// - 对端无自报地址 → 原样返回 `primary_endpoint`（快速路径）
    /// - 缓存命中（TTL 内）→ 直接返回缓存选定地址
    /// - 否则探测：内网优先 first-match → 公网回退 → 全不通维持主地址
    pub async fn resolve(&self, primary_endpoint: &str, addresses: &[FederationAddress]) -> String {
        let primary = primary_endpoint.trim().trim_end_matches('/').to_string();
        if addresses.is_empty() {
            return primary;
        }

        // TTL 缓存命中：零探测直接返回
        if let Some(entry) = self
            .cache
            .lock()
            .ok()
            .and_then(|c| c.get(&primary).cloned())
            .filter(|entry| entry.resolved_at.elapsed() < RESOLVE_TTL)
        {
            return entry.chosen;
        }

        let (private, public) = candidate_order(&primary, addresses);
        let mut chosen = primary.clone();
        for candidate in private.into_iter().chain(public) {
            if probe(&candidate).await {
                chosen = candidate;
                break;
            }
        }

        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(
                primary,
                CacheEntry {
                    chosen: chosen.clone(),
                    resolved_at: Instant::now(),
                },
            );
        }
        chosen
    }

    /// 清除指定对端的解析缓存（调用失败强制重探入口；预留，当前未接线）
    pub fn invalidate(&self, primary_endpoint: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(primary_endpoint.trim().trim_end_matches('/'));
        }
    }
}

/// 获取解析器 trait 对象包装（供 DAL 持有，测试可注入替身）
pub fn boxed() -> Arc<dyn EndpointResolver + Send + Sync> {
    Arc::new(SharedResolver)
}

/// 解析器接口（DAL 依赖此 trait 而非具体类型，测试可注入）
#[async_trait::async_trait]
pub trait EndpointResolver: Send + Sync {
    async fn resolve(&self, primary_endpoint: &str, addresses: &[FederationAddress]) -> String;
}

/// 生产实现：转发到进程内单例
struct SharedResolver;

#[async_trait::async_trait]
impl EndpointResolver for SharedResolver {
    async fn resolve(&self, primary_endpoint: &str, addresses: &[FederationAddress]) -> String {
        resolver().resolve(primary_endpoint, addresses).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(url: &str, scope: &str) -> FederationAddress {
        FederationAddress {
            url: url.to_string(),
            scope: scope.to_string(),
        }
    }

    #[test]
    fn test_candidate_order_private_first() {
        let (private, public) = candidate_order(
            "https://primary.example.com",
            &[
                addr("http://10.0.0.5:8080", "private"),
                addr("https://primary.example.com", "public"), // 与主地址重复，去重
                addr("http://192.168.1.2:9000", "private"),
                addr("https://backup.example.com", "public"),
                addr("", "private"), // 空地址过滤
            ],
        );
        assert_eq!(
            private,
            vec!["http://10.0.0.5:8080", "http://192.168.1.2:9000"]
        );
        assert_eq!(
            public,
            vec!["https://primary.example.com", "https://backup.example.com"]
        );
    }

    #[test]
    fn test_parse_host_port() {
        assert_eq!(
            parse_host_port("https://peer.example.com/api"),
            Some(("peer.example.com".into(), 443))
        );
        assert_eq!(
            parse_host_port("http://10.0.0.5:8080"),
            Some(("10.0.0.5".into(), 8080))
        );
        assert_eq!(
            parse_host_port("http://[::1]:9000/"),
            Some(("::1".into(), 9000))
        );
        assert_eq!(parse_host_port(""), None);
    }

    #[tokio::test]
    async fn test_resolve_private_first_and_cache() {
        // 起一个真实 listener 模拟可达的内网地址
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let private_port = listener.local_addr().unwrap().port();

        let primary = "http://127.0.0.1:9"; // discard 端口，必不通
        let addresses = vec![
            addr(&format!("http://127.0.0.1:{private_port}"), "private"),
            addr("http://127.0.0.1:1", "private"), // 不通的内网候选
        ];

        let r = ReachabilityResolver::new();
        let chosen = r.resolve(primary, &addresses).await;
        assert_eq!(chosen, format!("http://127.0.0.1:{private_port}"));

        // TTL 缓存：关闭 listener 后立即再解析，仍返回缓存值（不重探）
        drop(listener);
        let chosen_again = r.resolve(primary, &addresses).await;
        assert_eq!(chosen_again, chosen);
    }

    #[tokio::test]
    async fn test_resolve_all_unreachable_falls_back_to_primary() {
        let r = ReachabilityResolver::new();
        let primary = "http://127.0.0.1:9";
        let addresses = vec![
            addr("http://127.0.0.1:1", "private"),
            addr("http://127.0.0.1:2", "public"),
        ];
        // 全不通 = 维持主地址（交给出站 HTTP 自然失败）
        assert_eq!(r.resolve(primary, &addresses).await, primary);
    }

    #[tokio::test]
    async fn test_resolve_fast_path_no_addresses() {
        let r = ReachabilityResolver::new();
        // 无自报地址：原样返回，不探测（行为与 P7 之前一致）
        assert_eq!(
            r.resolve("http://primary.example.com", &[]).await,
            "http://primary.example.com"
        );
    }
}
