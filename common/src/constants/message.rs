//! 消息相关常量
//!
//! 目前只有一件事：给「默认会话」一个可寻址的虚拟 project id。

/// 默认会话（虚拟群组）的哨兵 project id
///
/// ## 为什么需要它
///
/// 消息表里「默认会话」（不挂在任何项目下的对话）是用 `project_id = NULL` 表达的。
/// 但 `MessageQuery::project_id` / `ListMessagesRequest::project_id` 的 `None` 已经被
/// 占用为「**不限制该条件**」，语义上是「返回全部消息」。一个字段两义，读侧就永远
/// 无法精确表达「我只要默认会话」——只能退化成「拉全组织消息、前端再过滤」，
/// 于是出现两个后果：
///
/// 1. 分页失真：一页原始消息可能被滤到一条不剩，`total` / `has_more` 都不再可信；
/// 2. 翻不到头：前端连翻 N 页仍被滤空后，游标停在原地，用户再怎么上拉也拿不到
///    更早的默认会话消息（列表没变化 → 不再产生滚动事件 → 死局）。
///
/// 哨兵值把「默认会话」从 `None` 里拆出来，让它在读侧成为一个真正可寻址的取值：
///
/// | 传入值                    | 语义                                  |
/// |---------------------------|---------------------------------------|
/// | `None`                    | 不过滤，返回全部                        |
/// | `Some(DEFAULT_CONVERSATION_PROJECT_ID)` | 只要 `project_id IS NULL` 的默认会话 |
/// | `Some(真实 project id)`   | 该项目的会话                            |
///
/// ## 关键约束：只存在于查询侧，永不落库
///
/// 它是**查询期别名**，不是真的数据。写路径（`send_message*`）必须先经
/// [`normalize_project_id`] 折叠回 `None`，落到库里仍然是 `NULL`：
///
/// - 库里若真存了 `'__default__'`，所有 `project_id.is_none()` 的既有判定
///   （收件人路由、项目查询、prompt 组装）都会失效；
/// - 反之，只要读侧翻译、写侧折叠，就**零迁移零回填**，历史数据一行都不用动。
///
/// ## 为什么不会和真实 id 撞车
///
/// 真实 project id 是 UUIDv7（只含 `[0-9a-f-]`）。本常量含下划线，构造上不可能相等，
/// 因此无需任何校验即可保证唯一性。
pub const DEFAULT_CONVERSATION_PROJECT_ID: &str = "__default__";

/// 判断给定 project id 是否是「默认会话」哨兵值
///
/// `None`（= 不过滤）不是默认会话，返回 `false`。
pub fn is_default_conversation(project_id: Option<&str>) -> bool {
    matches!(project_id, Some(id) if id == DEFAULT_CONVERSATION_PROJECT_ID)
}

/// 写路径归一化：哨兵值折叠为 `None`（落库为 `NULL`），其余原样返回
///
/// 见 [`DEFAULT_CONVERSATION_PROJECT_ID`] 的「只存在于查询侧」一节：任何要把
/// `project_id` 写进库、或拿它去查真实项目的路径，都必须先过这里。
pub fn normalize_project_id(project_id: Option<&str>) -> Option<&str> {
    match project_id {
        Some(id) if id == DEFAULT_CONVERSATION_PROJECT_ID => None,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_conversation_detection() {
        assert!(is_default_conversation(Some(
            DEFAULT_CONVERSATION_PROJECT_ID
        )));
        // None = 不过滤，不是默认会话
        assert!(!is_default_conversation(None));
        // 真实 UUIDv7 不是默认会话
        assert!(!is_default_conversation(Some(
            "0192f3a1-7c4e-7000-8a2b-0d1e2f3a4b5c"
        )));
    }

    #[test]
    fn sentinel_can_never_collide_with_uuid() {
        // UUID 字符集不含下划线 → 构造上不可能相等
        assert!(
            DEFAULT_CONVERSATION_PROJECT_ID
                .chars()
                .any(|c| !c.is_ascii_hexdigit() && c != '-')
        );
    }

    #[test]
    fn normalize_folds_sentinel_to_none() {
        assert_eq!(
            normalize_project_id(Some(DEFAULT_CONVERSATION_PROJECT_ID)),
            None
        );
        assert_eq!(normalize_project_id(None), None);
        assert_eq!(
            normalize_project_id(Some("0192f3a1-7c4e-7000-8a2b-0d1e2f3a4b5c")),
            Some("0192f3a1-7c4e-7000-8a2b-0d1e2f3a4b5c")
        );
    }
}
