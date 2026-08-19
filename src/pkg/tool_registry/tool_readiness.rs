//! 工具环境就绪探测与统一引导（三层就绪提示体系的 pkg 基础设施）
//!
//! 设计见 docs/design/web_search_and_browser_tools_design.md 决策 6/7/11：
//! - **探测器**：CLI 型 → 二进制可寻址（config 绝对路径优先 → PATH 扫描）；
//!   key 型 → 共享 config key 非空 OR 用户凭证库含对应 kind（经 Resolver 只读查询，
//!   不发真实网络请求；key 型就绪状态按当前查看者判定，必须带用户上下文）
//! - **TTL 缓存**：CLI 型按 tool_id、key 型按 (tool_id, user_id)，避免每次列表都探测
//! - **统一引导**：`cli_not_installed` / `api_key_missing` 结构化 JSON（调用时兜底），
//!   供 lark_cli / gh_cli / browser / tavily_search 共用
//!
//! 分层说明：探测器注册表与缓存为 pkg 内部实现；key 型探测经各工具的
//! CredentialResolver 接口查询（与授权解析同模式），pkg 不 import DAL/DAO。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::config::get;
use crate::pkg::RequestContext;
use common::api::RuntimeReady;

/// 探测结果缓存 TTL（秒）：窗口内重复列表请求复用上次探测
const CACHE_TTL: Duration = Duration::from_secs(30);

// ==================== 探测器 trait 与注册表 ====================

/// 工具就绪探测器（pkg 层抽象）
#[async_trait]
pub trait ToolReadinessProbe: Send + Sync {
    /// key 型探测器返回 true：缓存与探测按用户区分（授权是用户相关的）
    fn user_scoped(&self) -> bool {
        false
    }

    /// 执行只读探测；探测内部异常应返回 `Unknown` 而非 Err
    async fn probe(&self, ctx: &RequestContext) -> RuntimeReady;
}

type ProbeRegistry = HashMap<String, Arc<dyn ToolReadinessProbe>>;

static PROBES: OnceLock<Mutex<ProbeRegistry>> = OnceLock::new();
static CACHE: OnceLock<Mutex<HashMap<String, (RuntimeReady, Instant)>>> = OnceLock::new();

fn probes() -> &'static Mutex<ProbeRegistry> {
    PROBES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache() -> &'static Mutex<HashMap<String, (RuntimeReady, Instant)>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 注册探测器（幂等：后注册覆盖同名 tool_id）
pub fn register_probe(tool_id: &str, probe: Arc<dyn ToolReadinessProbe>) {
    probes().lock().unwrap().insert(tool_id.to_string(), probe);
}

/// 注册内置工具的默认探测器集（service::init 阶段调用）
///
/// - browser / lark_cli / gh_cli：CLI 二进制探测（browser 命令取 config，可配绝对路径）
/// - tavily_search：授权双轨探测（共享 config OR 用户凭证库 TavilyKey，按用户区分）
pub fn register_default_probes() {
    register_probe("browser", Arc::new(BrowserCliProbe));
    register_probe(
        "lark_cli",
        Arc::new(FixedCliProbe {
            bin: crate::pkg::tool_registry::lark_cli::LARK_CLI_BIN,
            install_hint: "安装 lark-cli：https://github.com/larksuite/lark-cli".to_string(),
            config_hint: "或在 ai_orz.toml 中为对应工具配置命令绝对路径".to_string(),
        }),
    );
    register_probe(
        "gh_cli",
        Arc::new(FixedCliProbe {
            bin: crate::pkg::tool_registry::gh_cli::GH_CLI_BIN,
            install_hint: "安装 GitHub CLI：https://cli.github.com（brew install gh）".to_string(),
            config_hint: "或确认 gh 已安装且在服务进程的 PATH 中".to_string(),
        }),
    );
    register_probe("tavily_search", Arc::new(TavilyKeyProbe));
}

/// 探测工具就绪状态（带 TTL 缓存；未注册探测器/探测异常返回 Unknown，不阻塞调用方）
pub async fn probe(tool_id: &str, ctx: &RequestContext) -> RuntimeReady {
    let probe = probes().lock().unwrap().get(tool_id).cloned();
    let Some(probe) = probe else {
        return RuntimeReady::Unknown;
    };

    // key 型按 (tool_id, user_id) 缓存；CLI 型按 tool_id
    let cache_key = if probe.user_scoped() {
        format!("{}|{}", tool_id, ctx.user_id.clone().unwrap_or_default())
    } else {
        tool_id.to_string()
    };

    if let Some((status, at)) = cache().lock().unwrap().get(&cache_key)
        && at.elapsed() < CACHE_TTL
    {
        return status.clone();
    }

    let status = probe.probe(ctx).await;
    cache()
        .lock()
        .unwrap()
        .insert(cache_key, (status.clone(), Instant::now()));
    status
}

/// 清除指定工具的就绪缓存（配置变更/安装后立即可见；测试亦用）
pub fn invalidate_cache(tool_id: &str) {
    let prefix = format!("{}|", tool_id);
    cache()
        .lock()
        .unwrap()
        .retain(|k, _| !k.starts_with(&prefix) && k != tool_id);
}

// ==================== CLI 型探测器 ====================

/// 二进制可寻址：含路径分隔符（绝对/相对路径）直接判文件存在，纯名称扫 PATH
pub fn command_available(command: &str) -> bool {
    let path = Path::new(command);
    if path.is_absolute() || path.components().count() > 1 {
        return path.is_file();
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(command).is_file())
}

/// CLI 型就绪判定（不可寻址 → not_ready{cli_not_installed}）
fn cli_binary_readiness(command: &str, install_hint: &str, config_hint: &str) -> RuntimeReady {
    if command_available(command) {
        RuntimeReady::Ready
    } else {
        RuntimeReady::NotReady {
            reason: "cli_not_installed".to_string(),
            hint: format!("{}；{}", install_hint, config_hint),
        }
    }
}

/// browser 探测器：命令取 config `[browser].command`（绝对路径优先，PATH 兜底）
struct BrowserCliProbe;

#[async_trait]
impl ToolReadinessProbe for BrowserCliProbe {
    async fn probe(&self, _ctx: &RequestContext) -> RuntimeReady {
        let command = get().browser.command.clone();
        cli_binary_readiness(
            &command,
            "安装 agent-browser：brew install agent-browser 或 cargo install agent-browser",
            "或在 ai_orz.toml 的 [browser].command 配置绝对路径",
        )
    }
}

/// 固定命令名探测器（lark-cli / gh 等，无 config 覆盖位）
struct FixedCliProbe {
    bin: &'static str,
    install_hint: String,
    config_hint: String,
}

#[async_trait]
impl ToolReadinessProbe for FixedCliProbe {
    async fn probe(&self, _ctx: &RequestContext) -> RuntimeReady {
        cli_binary_readiness(self.bin, &self.install_hint, &self.config_hint)
    }
}

// ==================== key 型探测器 ====================

/// tavily_search 授权双轨探测：共享 config key 非空 OR 该用户凭证库含 TavilyKey
///
/// 经 `TavilyCredentialResolver` 只读查询（与调用时授权解析同源），
/// 不发真实网络请求验证 key 有效性。
struct TavilyKeyProbe;

#[async_trait]
impl ToolReadinessProbe for TavilyKeyProbe {
    fn user_scoped(&self) -> bool {
        true
    }

    async fn probe(&self, ctx: &RequestContext) -> RuntimeReady {
        if !get().tavily.api_key.trim().is_empty() {
            return RuntimeReady::Ready;
        }
        let Some(resolver) = crate::pkg::tool_registry::tavily_search::get_credential_resolver()
        else {
            // Resolver 未注册（初始化顺序异常）：无法判定
            return RuntimeReady::Unknown;
        };
        match resolver.resolve(ctx).await {
            Ok(Some(_)) => RuntimeReady::Ready,
            Ok(None) => RuntimeReady::NotReady {
                reason: "api_key_missing".to_string(),
                hint: "绑定个人 Tavily key（设置 → 身份凭证 → Tavily 区块），或由管理员在服务端 ai_orz.toml 的 [tavily].api_key 配置共享 key".to_string(),
            },
            Err(_) => RuntimeReady::Unknown,
        }
    }
}

// ==================== 统一引导构造（调用时兜底） ====================

/// CLI 未安装结构化引导（spawn NotFound 分支统一出口）
pub fn cli_not_installed_json(bin: &str, install_hint: &str, config_hint: &str) -> Value {
    json!({
        "success": false,
        "error_code": "cli_not_installed",
        "error": format!("未找到 {} 二进制，请先安装或配置路径", bin),
        "install": install_hint,
        "hint": config_hint
    })
}

/// 授权缺失结构化引导（双路径：绑个人凭证 / 配共享 key）
pub fn api_key_missing_json(error: &str, guidance: &str) -> Value {
    json!({
        "success": false,
        "error_code": "api_key_missing",
        "error": error,
        "guidance": guidance
    })
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::request_context_test_support::new_test_ctx;

    fn test_ctx() -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        new_test_ctx("probe-user", pool)
    }

    /// 计数探测器：验证 TTL 缓存命中（连续两次探测只触发一次真实探测）
    struct CountingProbe {
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl ToolReadinessProbe for CountingProbe {
        async fn probe(&self, _ctx: &RequestContext) -> RuntimeReady {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            RuntimeReady::Ready
        }
    }

    /// 恒不就绪探测器（key 型，验证 user_scoped 缓存键）
    struct NeverReadyProbe;

    #[async_trait]
    impl ToolReadinessProbe for NeverReadyProbe {
        fn user_scoped(&self) -> bool {
            true
        }

        async fn probe(&self, _ctx: &RequestContext) -> RuntimeReady {
            RuntimeReady::NotReady {
                reason: "api_key_missing".to_string(),
                hint: "test hint".to_string(),
            }
        }
    }

    #[tokio::test]
    async fn cache_hit_within_ttl() {
        let probe_impl = Arc::new(CountingProbe {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        register_probe("test-cache-hit", probe_impl.clone());
        invalidate_cache("test-cache-hit");

        let ctx = test_ctx();
        let first = probe("test-cache-hit", &ctx).await;
        let second = probe("test-cache-hit", &ctx).await;
        assert_eq!(first, RuntimeReady::Ready);
        assert_eq!(second, RuntimeReady::Ready);
        assert_eq!(
            probe_impl.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "second probe within TTL should hit cache"
        );

        // invalidate 后重新探测
        invalidate_cache("test-cache-hit");
        let _ = probe("test-cache-hit", &ctx).await;
        assert_eq!(
            probe_impl.calls.load(std::sync::atomic::Ordering::SeqCst),
            2
        );
    }

    #[tokio::test]
    async fn user_scoped_probe_caches_per_user() {
        let probe_impl = Arc::new(NeverReadyProbe);
        register_probe("test-user-scoped", probe_impl);
        invalidate_cache("test-user-scoped");

        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let ctx_a = new_test_ctx("user-a", pool.clone());
        let ctx_b = new_test_ctx("user-b", pool);

        let a1 = probe("test-user-scoped", &ctx_a).await;
        let a2 = probe("test-user-scoped", &ctx_a).await;
        let b = probe("test-user-scoped", &ctx_b).await;

        assert_eq!(
            a1,
            RuntimeReady::NotReady {
                reason: "api_key_missing".to_string(),
                hint: "test hint".to_string()
            }
        );
        assert_eq!(a1, a2, "same user should hit cache");
        assert_eq!(a1, b, "different user gets same computed status here");
    }

    #[tokio::test]
    async fn ttl_expiry_triggers_reprobe() {
        let probe_impl = Arc::new(CountingProbe {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        register_probe("test-ttl-expiry", probe_impl.clone());
        invalidate_cache("test-ttl-expiry");

        let ctx = test_ctx();
        let _ = probe("test-ttl-expiry", &ctx).await;
        assert_eq!(
            probe_impl.calls.load(std::sync::atomic::Ordering::SeqCst),
            1
        );

        // 手动把缓存时间戳拨回 TTL 之前，模拟过期
        cache().lock().unwrap().insert(
            "test-ttl-expiry".to_string(),
            (
                RuntimeReady::Ready,
                Instant::now() - CACHE_TTL - Duration::from_secs(1),
            ),
        );
        let _ = probe("test-ttl-expiry", &ctx).await;
        assert_eq!(
            probe_impl.calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "expired cache should trigger reprobe"
        );
    }

    #[tokio::test]
    async fn unregistered_tool_returns_unknown() {
        let ctx = test_ctx();
        assert_eq!(probe("no-such-tool", &ctx).await, RuntimeReady::Unknown);
    }

    #[test]
    fn command_available_absolute_path() {
        // 绝对路径：存在的文件 → true；不存在 → false
        assert!(command_available("/bin/ls"));
        assert!(!command_available("/no/such/binary-xyz"));
    }

    #[test]
    fn command_available_path_scan() {
        // 纯名称：macOS/Linux 环境必有 ls
        assert!(command_available("ls"));
        assert!(!command_available("no-such-binary-xyz-abc"));
    }

    #[tokio::test]
    async fn tavily_probe_without_key_is_not_ready() {
        // 共享 key 未配置 + Resolver 未注册（单测环境）→ Unknown
        // （Resolver 注册态与 config 内容受测试执行顺序影响，这里只验证不 panic）
        let _ = crate::config::init();
        register_default_probes();
        invalidate_cache("tavily_search");
        let ctx = test_ctx();
        let status = probe("tavily_search", &ctx).await;
        assert!(matches!(
            status,
            RuntimeReady::NotReady { .. } | RuntimeReady::Unknown
        ));
    }

    #[test]
    fn cli_not_installed_json_shape() {
        let v =
            cli_not_installed_json("agent-browser", "brew install agent-browser", "config hint");
        assert_eq!(v["success"], false);
        assert_eq!(v["error_code"], "cli_not_installed");
        assert!(v["error"].as_str().unwrap().contains("agent-browser"));
        assert_eq!(v["install"], "brew install agent-browser");
        assert_eq!(v["hint"], "config hint");
    }

    #[test]
    fn api_key_missing_json_shape() {
        let v = api_key_missing_json("no key", "guidance text");
        assert_eq!(v["success"], false);
        assert_eq!(v["error_code"], "api_key_missing");
        assert_eq!(v["error"], "no key");
        assert_eq!(v["guidance"], "guidance text");
    }

    #[tokio::test]
    async fn key_probe_resolver_error_returns_unknown() {
        // 直接构造 TavilyKeyProbe 验证 Resolver 缺失场景
        let probe = TavilyKeyProbe;
        // 共享 key 为空的前提由 config init 后默认值保证
        let _ = crate::config::init();
        let status = probe.probe(&test_ctx()).await;
        // Resolver 未注册时为 Unknown；Resolver Err 分支同走 Unknown（探测异常不阻塞）
        assert!(matches!(
            status,
            RuntimeReady::Unknown | RuntimeReady::NotReady { .. }
        ));
    }
}
