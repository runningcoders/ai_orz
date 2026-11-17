//! 按天 JSONL 读写基建。
//!
//! 这里只放**零业务依赖**的通用机械层：
//! - [`writer`]：按天分片的 JSONL 追加写 + 按坐标读回（全局 `APPEND_LOCK` 保证行原子）；
//! - [`sidecar`]：旁车索引底层——watermark 校验、增量补齐、坏行重建、按坐标读回。
//!
//! 三类 trace（工具调用 / 记忆 / 思考）各自的过滤字段、匹配与合并语义在**各自业务模块**
//! 的适配层里，通过泛型 `F` + 解析闭包注入本模块。适配层不放在这里——它们依赖业务类型
//! （memory 适配层甚至依赖 `service` 层），放进基建模块会造成底层反向依赖上层。

pub mod sidecar;
pub mod writer;

#[cfg(test)]
mod writer_test;
