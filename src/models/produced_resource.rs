//! 本轮工具产出的资源引用（跨层共享的纯数据）
//!
//! 「这一轮产出了什么」由执行层判定（`runtime::collected::collect_produced`），
//! 「怎么拼进 markdown 正文」由消息域负责（`message::resource_refs`）。两边都
//! 需要同一个值对象，因此它必须待在依赖链最底端：
//!
//! ```text
//! 执行层（runtime）产出 ──┐
//!                        ├── ProducedResource（models）
//! 消息域（message）消费 ──┘
//! ```
//!
//! ⚠️ 不要再把它挂回任一域：挂回 runtime 会让 `message → runtime` 成立（域间调用），
//! 挂回 message 会让 `runtime → message` 成立（执行层反向依赖消息协议）。

/// 本轮工具产出的一条资源：scheme + id + 展示名
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducedResource {
    /// 协议 scheme：`attachment` / `artifact`
    pub scheme: &'static str,
    /// 资源 id（工具返回）
    pub id: String,
    /// 展示名（工具返回的 original_name / name）
    pub name: String,
}
