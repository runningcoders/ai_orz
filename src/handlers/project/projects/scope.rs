//! 项目读侧的用户域收敛（`query_projects` / `search_projects` 共用）
//!
//! 写侧早就有归属校验（`create_task` 的 `root_user_id` 必须是当前用户），
//! 读侧却把 body 里的 `root_user_id` **原样**塞进 SQL 过滤条件：传别人的 id
//! 就能列举别人的项目，**不传**则等于不加用户域条件，直接扫全库。这与
//! `list_projects`（内部写死 `ctx.uid()`）不是同一套口径，等于把「对象级
//! 授权」留在了调用方手里。

use crate::pkg::RequestContext;
use common::error::Result;

/// 把请求里的 `root_user_id` 收敛到调用方的用户域
///
/// - 调用方没有用户绑定（系统 / A2A 内部链路）→ 原样透传：这类调用本身没有
///   可收敛的域，且不经过 Agent / 前端；
/// - 未指定或空串 → 补成 `ctx.uid()`，**不允许「不传就查全库」**；
/// - 显式指定 → 必须等于 `ctx.uid()`，否则 `InvalidRequest`。
pub(super) fn scoped_root_user_id(
    ctx: &RequestContext,
    requested: Option<String>,
) -> Result<Option<String>> {
    effective_root_user_id(&ctx.uid(), requested)
}

/// [`scoped_root_user_id`] 的纯函数内核（便于单测，不依赖 `RequestContext`）
///
/// ⚠️ 顺序不能反：先判空、再判相等。空串按「未指定」处理而不是「查空用户」，
/// 否则前端 `root_user_id: Some(String::new())` 这种历史写法会静默查不到东西。
fn effective_root_user_id(me: &str, requested: Option<String>) -> Result<Option<String>> {
    if me.is_empty() {
        return Ok(requested);
    }
    match requested.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() && v != me => Err(common::error::err!(
            InvalidRequest,
            "不能查询其他用户的项目（请求 root_user_id={}，当前用户={}）",
            v,
            me
        )),
        _ => Ok(Some(me.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::effective_root_user_id;

    #[test]
    fn unscoped_request_is_pinned_to_caller() {
        // 不传 / 空串都收敛到调用方自身，杜绝「不传即全库」
        assert_eq!(
            effective_root_user_id("u1", None).unwrap(),
            Some("u1".to_string())
        );
        assert_eq!(
            effective_root_user_id("u1", Some(String::new())).unwrap(),
            Some("u1".to_string())
        );
        assert_eq!(
            effective_root_user_id("u1", Some("   ".to_string())).unwrap(),
            Some("u1".to_string())
        );
    }

    #[test]
    fn same_user_is_accepted() {
        assert_eq!(
            effective_root_user_id("u1", Some("u1".to_string())).unwrap(),
            Some("u1".to_string())
        );
        // 前端可能带空白，trim 后相等即放行
        assert_eq!(
            effective_root_user_id("u1", Some(" u1 ".to_string())).unwrap(),
            Some("u1".to_string())
        );
    }

    #[test]
    fn other_user_is_rejected() {
        let err = effective_root_user_id("u1", Some("u2".to_string())).unwrap_err();
        assert!(
            format!("{err:?}").contains("不能查询其他用户的项目"),
            "越权查询必须显式报错，实际 {err:?}"
        );
    }

    #[test]
    fn unbound_caller_keeps_requested_filter() {
        // 系统 / A2A 内部链路没有用户域，原样透传
        assert_eq!(effective_root_user_id("", None).unwrap(), None);
        assert_eq!(
            effective_root_user_id("", Some("u9".to_string())).unwrap(),
            Some("u9".to_string())
        );
    }
}
