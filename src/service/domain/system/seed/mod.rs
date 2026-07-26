//! Seed 配置迁移子模块（纯工具箱）
//!
//! 提供业务实体定义的导出/导入/diff 的数据结构和算法工具，不持有任何 DAL 引用，
//! 不调用其他 domain。实际的 DB 读写由 handler 层编排各 domain 完成。
//! 不包含运行时数据（消息、任务、stats、日志、向量索引）。

pub mod defs;
pub mod default;
pub mod diff;
pub mod store;

pub use defs::*;

#[cfg(test)]
mod seed_test;
