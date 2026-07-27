//! Diff 算法 + 敏感字段解析（纯函数）
//!
//! 这些函数只接收数据返回结果，不调用任何 DAL 或 domain。
//! 跨 domain 的 DB 读取由 handler 完成，handler 把当前 DB 值作为参数传入。

use super::defs::*;
use std::collections::HashMap;

/// 对比两个快照（纯函数）
pub fn diff_snapshots(base: &SeedSnapshot, target: &SeedSnapshot) -> SeedDiff {
    let mut summary = DiffSummary::default();
    let org_diff = diff_organization(&base.organization, &target.organization, &mut summary);

    let users = diff_vec(&base.users, &target.users, &mut summary, |u| u.id.clone());
    let model_providers = diff_vec(
        &base.model_providers,
        &target.model_providers,
        &mut summary,
        |p| p.id.clone(),
    );
    let agents = diff_vec(&base.agents, &target.agents, &mut summary, |a| a.id.clone());
    let skills = diff_vec(&base.skills, &target.skills, &mut summary, |s| s.id.clone());

    SeedDiff {
        meta: DiffMeta {
            kind: DiffKind::FileVsFile,
            base_source: base.source_organization_id.clone(),
            target_source: target.source_organization_id.clone(),
            compared_at: common::constants::utils::current_timestamp(),
        },
        summary,
        organization: org_diff,
        users,
        model_providers,
        agents,
        skills,
    }
}

fn diff_organization(
    base: &OrganizationDef,
    target: &OrganizationDef,
    summary: &mut DiffSummary,
) -> Option<DiffEntry<OrganizationDef>> {
    let changes = collect_changes(base, target);
    if changes.is_empty() {
        summary.same_count += 1;
        Some(DiffEntry::Same {
            id: base.id.clone(),
            current: base.clone(),
        })
    } else {
        summary.updated_count += 1;
        Some(DiffEntry::Updated {
            id: base.id.clone(),
            current: base.clone(),
            snapshot: target.clone(),
            changes,
        })
    }
}

fn diff_vec<T, F>(
    base: &[T],
    target: &[T],
    summary: &mut DiffSummary,
    id_fn: F,
) -> Vec<DiffEntry<T>>
where
    T: Clone + serde::Serialize,
    F: Fn(&T) -> String,
{
    let mut result = Vec::new();
    let mut base_ids = std::collections::HashSet::new();

    for b in base {
        let id = id_fn(b);
        base_ids.insert(id.clone());
        if let Some(t) = target.iter().find(|t| id_fn(t) == id) {
            let changes = collect_changes(b, t);
            if changes.is_empty() {
                summary.same_count += 1;
                result.push(DiffEntry::Same {
                    id,
                    current: b.clone(),
                });
            } else {
                summary.updated_count += 1;
                result.push(DiffEntry::Updated {
                    id,
                    current: b.clone(),
                    snapshot: t.clone(),
                    changes,
                });
            }
        } else {
            summary.removed_count += 1;
            result.push(DiffEntry::Removed {
                id,
                current: b.clone(),
            });
        }
    }

    for t in target {
        let id = id_fn(t);
        if !base_ids.contains(&id) {
            summary.new_count += 1;
            result.push(DiffEntry::New {
                id,
                snapshot: t.clone(),
            });
        }
    }

    result
}

fn collect_changes<T: serde::Serialize>(base: &T, target: &T) -> Vec<FieldChange> {
    let base_val = serde_json::to_value(base).unwrap_or(serde_json::Value::Null);
    let target_val = serde_json::to_value(target).unwrap_or(serde_json::Value::Null);
    collect_field_changes_recursive(&base_val, &target_val, "")
}

fn collect_field_changes_recursive(
    base: &serde_json::Value,
    target: &serde_json::Value,
    prefix: &str,
) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    match (base, target) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(target_map)) => {
            for (key, base_val) in base_map {
                let field = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                if let Some(target_val) = target_map.get(key)
                    && base_val != target_val {
                        if base_val.is_object() && target_val.is_object() {
                            changes.extend(collect_field_changes_recursive(
                                base_val, target_val, &field,
                            ));
                        } else {
                            changes.push(FieldChange {
                                field,
                                old_value: base_val.clone(),
                                new_value: target_val.clone(),
                            });
                        }
                    }
            }
        }
        _ => {
            if base != target {
                changes.push(FieldChange {
                    field: prefix.to_string(),
                    old_value: base.clone(),
                    new_value: target.clone(),
                });
            }
        }
    }

    changes
}

/// 校验敏感字段是否齐备（纯函数）
///
/// 返回 Err(message) 表示缺少字段；返回 Ok(()) 表示齐备
pub fn validate_sensitive_fields(
    snapshot: &SeedSnapshot,
    sensitive_values: &HashMap<String, String>,
) -> Result<(), String> {
    for u in &snapshot.users {
        if u.password_ref == PENDING_INPUT {
            let key = format!("user:{}:password", u.id);
            if !sensitive_values.contains_key(&key) {
                return Err(format!(
                    "缺少敏感字段: {} (用户 {} 的密码)",
                    key, u.username
                ));
            }
        }
    }
    for p in &snapshot.model_providers {
        if p.api_key_ref == PENDING_INPUT {
            let key = format!("model_provider:{}:api_key", p.id);
            if !sensitive_values.contains_key(&key) {
                return Err(format!(
                    "缺少敏感字段: {} (Provider {} 的 API Key)",
                    key, p.name
                ));
            }
        }
    }
    Ok(())
}

/// 解析密码占位符（纯函数）
///
/// current_password_hash：当 ref_value = INHERIT_CURRENT 时由 handler 查 DB 传入（None 表示 DB 中无此用户）
pub fn resolve_password(
    ref_value: &str,
    user_id: &str,
    sensitive_values: &HashMap<String, String>,
    current_password_hash: Option<&str>,
) -> Result<String, String> {
    match ref_value {
        PENDING_INPUT => {
            let key = format!("user:{}:password", user_id);
            sensitive_values
                .get(&key)
                .cloned()
                .ok_or_else(|| format!("缺少密码: {}", key))
        }
        INHERIT_CURRENT => current_password_hash
            .map(|s| s.to_string())
            .ok_or_else(|| format!("INHERIT_CURRENT 但 DB 中无用户 {} 的当前密码", user_id)),
        RANDOM_GENERATE => {
            // 生成随机密码（实际场景应由 handler 转换为 hash 并展示明文给管理员）
            Ok(format!("random_{}", uuid::Uuid::now_v7()))
        }
        _ => Err(format!("未知占位符: {}", ref_value)),
    }
}

/// 解析 API Key 占位符（纯函数）
pub fn resolve_api_key(
    ref_value: &str,
    provider_id: &str,
    sensitive_values: &HashMap<String, String>,
    current_api_key: Option<&str>,
) -> Result<String, String> {
    match ref_value {
        PENDING_INPUT => {
            let key = format!("model_provider:{}:api_key", provider_id);
            sensitive_values
                .get(&key)
                .cloned()
                .ok_or_else(|| format!("缺少 API Key: {}", key))
        }
        INHERIT_CURRENT => current_api_key.map(|s| s.to_string()).ok_or_else(|| {
            format!(
                "INHERIT_CURRENT 但 DB 中无 Provider {} 的当前 API Key",
                provider_id
            )
        }),
        _ => Err(format!("未知占位符: {}", ref_value)),
    }
}
