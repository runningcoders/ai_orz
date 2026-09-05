//! 通用工具模块
//!
//! 提供 Graph 渲染、远程内容抓取等通用能力。
//! HTTP 出站相关能力（客户端构建 / SSRF 防护）统一在 `pkg::http`。

pub mod fetch_remote_content;
pub mod graph;
