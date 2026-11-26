// Common types shared between backend and frontend
// All DTOs (Data Transfer Objects) for API requests and responses are defined here
// This ensures type safety and consistency between backend and frontend

#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Common shared types for ai_orz backend and frontend

pub mod api;
pub mod config;
pub mod constants;
pub mod doc_link;
pub mod enums;
pub mod error;
pub mod id;
pub mod llm_schema;
pub mod markdown_protocol;
pub mod markdown_render;
pub mod mention;
pub mod mime;
pub mod models;
pub mod ontology;
pub mod ontology_property;
pub mod redaction;
/// 前后端共用的时间算法：cron 解析、日期词折算、规则在任意区间的实例展开。
pub mod schedule;
/// 跨 JSON 边界保真的三态字段反序列化（更新类 DTO 的「保持/清空/覆盖」）
pub mod serde_ext;

#[cfg(test)]
mod error_test;
