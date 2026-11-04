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
