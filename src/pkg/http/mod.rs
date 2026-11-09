//! HTTP 出站基建层
//!
//! 全项目出站 HTTP 请求的统一基建，与 `pkg/ws`（长连接基建）同层同级。
//!
//! # 分层职责
//!
//! - [`client`]：唯一的 `reqwest::Client` 构建入口 + 选项模型，承载「怎么连」
//!   （超时、代理、重定向、DNS pinning、User-Agent）。
//! - [`presets`]：业务侧常用预设（LLM / 一般出站 / SSRF 防护出站），
//!   业务层只声明「要哪种客户端」，不写 builder。
//! - [`ssrf`]：出站安全（SSRF 防护、响应大小限制、敏感头脱敏），
//!   从 `pkg/utils/http_security` 迁入，与客户端构建内聚。
//! - [`file`]：**入站**方向的基建——把磁盘文件转成流式 HTTP 响应（单区间 Range /
//!   安全头 / Content-Disposition 策略）。附件、产物下载等「把用户字节吐给浏览器」
//!   的场景统一走这里，不要各 handler 手搓 `Body::from(bytes)`。
//!   ⚠️ 条件请求（ETag / If-Range / If-Modified-Since）尚未实现，见该模块文档。
//!
//! # 硬约束
//!
//! - **默认必带超时**：`reqwest::Client::new()` 无超时，网络抖动时会永久挂起。
//!   本模块保证任何配置路径下超时都落在 `1ms..=MAX_TIMEOUT` 区间内。
//! - 业务层禁止再直接调用 `reqwest::Client::new()` / `reqwest::Client::builder()`，
//!   一律走本模块。
//! - 返回文件字节一律走 [`file::serve_file`]：`inline` / `attachment` 的安全判定
//!   （HTML、SVG 不得内联执行）必须由基建统一把关，不能由调用方各自决定。

pub mod client;
pub mod file;
pub mod presets;
pub mod ssrf;

pub use client::{
    DEFAULT_TIMEOUT, DEFAULT_TIMEOUT_MS, HttpClientOptions, MAX_TIMEOUT, MAX_TIMEOUT_MS,
    RedirectPolicy, USER_AGENT, build_client,
};
// 只暴露「调用方真正需要的」两个入口；Range 解析 / MIME 归一化 / Content-Disposition
// 等细节留在 `file` 模块内，避免把内部算法固化成跨模块公共 API。
pub use file::{FileServeOptions, serve_file};
pub use presets::{
    LLM_STREAM_IDLE_TIMEOUT, LLM_STREAM_TOTAL_TIMEOUT, LLM_TIMEOUT_MS, llm, outbound, ssrf_guarded,
    with_timeout, with_timeout_ms,
};
pub use ssrf::{
    DEFAULT_RESPONSE_MAX_BYTES, HARD_RESPONSE_MAX_BYTES, domain_matches, is_local_network_host,
    is_local_network_ip, is_sensitive_header, normalize_domain, read_limited_response_body,
    sanitize_response_headers, validate_target_url,
};

/// 本机回环地址的 no-proxy 例外清单（测试环境统一口径）
///
/// 取值与 `tests/e2e/playwright.config.ts` 中的 `NO_PROXY` 同源：
/// `127.0.0.1`（IPv4 回环）、`localhost`（名字解析）、`::1`（IPv6 回环）。
pub const LOOPBACK_NO_PROXY: &str = "127.0.0.1,localhost,::1";

/// 测试进程：把本机回环地址加入 `NO_PROXY` / `no_proxy` 例外（幂等）。
///
/// # 为什么测试层需要这个
///
/// 宿主开发机常开系统代理（如 macOS `HTTPEnable=1` → `127.0.0.1:7993`），
/// `reqwest` 默认会读取系统代理设置。而 macOS 的 `ExceptionsList` 习惯写成
/// `127.*` 这类通配，`reqwest` 的 no_proxy 匹配器只认 IP / CIDR / 域名后缀，
/// 识别不了 `127.*`，于是发往 `127.0.0.1:<随机端口>` 的请求被塞进代理，
/// 代理回 **502 Bad Gateway** —— 表现为「本机 mock server 明明在监听，
/// 客户端却报 502 / 初始化会话失败」。
///
/// 测试一律面向本机回环 mock 服务（stub A2A server、python MCP mock），
/// 不存在走代理的正当理由，因此在测试构建下统一声明例外。
/// 生产运行不经过本函数（`#[cfg(test)]`），代理行为零改动。
///
/// # 安全性
///
/// `std::env::set_var` 在 Rust 2024 标记 `unsafe`（跨线程读写 env 会 UB）。
/// 这里用 [`std::sync::Once`] 保证进程内只写一次，并**合并**已有取值而不是
/// 覆盖调用方的显式配置；与 `tests/common/env.rs` 的既有初始化同样处理。
#[cfg(test)]
pub(crate) fn install_loopback_no_proxy() {
    use std::collections::HashSet;
    use std::sync::Once;

    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let mut items: Vec<String> = Vec::new();
        for var in ["NO_PROXY", "no_proxy"] {
            if let Ok(value) = std::env::var(var) {
                items.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(str::to_string),
                );
            }
        }
        items.extend(LOOPBACK_NO_PROXY.split(',').map(str::to_string));

        // 去重（大小写不敏感，保留首次出现的写法）
        let mut seen: HashSet<String> = HashSet::new();
        let merged: Vec<String> = items
            .into_iter()
            .filter(|item| seen.insert(item.to_lowercase()))
            .collect();
        let merged = merged.join(",");

        // 安全性见上方文档：Once 保证单次写入，且写入发生在测试首次建链之前。
        unsafe {
            std::env::set_var("NO_PROXY", &merged);
            std::env::set_var("no_proxy", &merged);
        }
    });
}
