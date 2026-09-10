//! Seed 导入敏感字段（用户密码 / Provider API Key）补填区
//!
//! 快照导出时敏感字段一律不落盘，只写 `PENDING_INPUT` 占位符。后端
//! `apply_snapshot_to_db` 写入前会调用 `validate_sensitive_fields`：**只在目标环境
//! 中不存在对应实体时才强制补值**——已存在的账号 / Provider 可以留空，写入时沿用其
//! 当前凭据（见 `src/service/domain/system/seed/diff.rs`）。本模块负责把快照里的占位符
//! 解析成表单项，并把用户输入整理成后端期望的 `sensitive_values` Map。
//!
//! 前端不做事前必填拦截：是否"必须填"取决于目标环境是否已有该实体，只有后端掌握这一
//! 信息，前端擅自拦截会误伤"导入已有组织"这类本可留空的场景。
//!
//! ## 为什么抽成独立组件
//!
//! 宿主是 seed 列表页，持有 `seeds` / `current_task` 等信号，任何一次 `.set` 都会让
//! 整棵子树重渲染。敏感字段是**动态列表**（条数随快照内容变化），内联在父组件里会
//! 因重渲染重建 input，打断 IME 组合输入。抽成独立组件后父级只传稳定 props
//! （`fields` 向量 + `use_callback` 生成的 EventHandler），Dioxus 的 props 记忆化
//! 会让它在父级重渲染时跳过，inputs 的 DOM 节点得以保持。

use dioxus::prelude::*;
use std::collections::HashMap;

/// 快照中的敏感字段占位符，与后端 `seed::defs::PENDING_INPUT` 对齐
pub const PENDING_INPUT: &str = "PENDING_INPUT";

/// 单个待补填的敏感字段
#[derive(Debug, Clone, PartialEq)]
pub struct SensitiveField {
    /// 提交键，格式 `{entity_type}:{entity_id}:{field}`，与后端校验完全一致
    pub key: String,
    /// 展示名（如「默认对话模型 的 API Key」）
    pub label: String,
    /// 输入框占位提示
    pub hint: String,
}

/// 从快照 JSON 推导必须由用户补填的敏感字段
///
/// 只处理 `PENDING_INPUT`：`INHERIT_CURRENT` 由后端取当前 DB 值、
/// `RANDOM_GENERATE` 由后端生成，都不需要前端输入。
pub fn extract_sensitive_fields(snapshot: &serde_json::Value) -> Vec<SensitiveField> {
    let mut fields = Vec::new();

    if let Some(users) = snapshot.get("users").and_then(|v| v.as_array()) {
        for user in users {
            if !is_pending(user, "password_ref") {
                continue;
            }
            let id = str_at(user, "id");
            let username = str_at(user, "username");
            fields.push(SensitiveField {
                key: format!("user:{}:password", id),
                label: format!("用户 {} 的密码", username),
                hint: "留空则沿用现有密码；新建账号必填".to_string(),
            });
        }
    }

    if let Some(providers) = snapshot.get("model_providers").and_then(|v| v.as_array()) {
        for provider in providers {
            if !is_pending(provider, "api_key_ref") {
                continue;
            }
            let id = str_at(provider, "id");
            let name = str_at(provider, "name");
            let model_name = str_at(provider, "model_name");
            fields.push(SensitiveField {
                key: format!("model_provider:{}:api_key", id),
                label: format!("{} 的 API Key", name),
                hint: format!("模型 {}；留空则沿用现有 Key", model_name),
            });
        }
    }

    fields
}

/// 解析快照文件文本并推导敏感字段清单
///
/// 解析失败返回可直接 toast 的提示文案。
pub fn parse_sensitive_fields(content: &str) -> Result<Vec<SensitiveField>, String> {
    let snapshot: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("解析快照失败: {}", e))?;
    Ok(extract_sensitive_fields(&snapshot))
}

fn str_at(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn is_pending(value: &serde_json::Value, key: &str) -> bool {
    value.get(key).and_then(|v| v.as_str()) == Some(PENDING_INPUT)
}

#[derive(Props, Clone, PartialEq)]
pub struct SeedSensitiveFieldsProps {
    /// 待补填字段（由快照解析得出，弹窗打开时刷新）
    fields: Vec<SensitiveField>,
    /// DryRun 预演不写入：直接隐藏输入区（无需任何凭据）
    #[props(default = false)]
    dry_run: bool,
    /// 输入变化回调：每次输入上报全量值，父级只存不渲染
    on_change: EventHandler<HashMap<String, String>>,
}

/// 敏感字段输入区（无外壳，直接嵌入弹窗表单）
///
/// 不做必填标记：是否必须填取决于目标环境是否已存在该实体，由后端裁决。已存在的
/// 账号 / Provider 留空即沿用现有凭据，只有新建的才需要填写。
#[component]
pub fn SeedSensitiveFields(props: SeedSensitiveFieldsProps) -> Element {
    // 草稿只在本组件内流转；父级拿到值后仅做存储，不回写渲染，避免打断输入
    let mut values = use_signal(HashMap::<String, String>::new);

    if props.dry_run {
        return rsx! {
            p { class: "text-xs text-base-content/50 py-2",
                "仅预演（DryRun）不会写入数据，无需填写凭据"
            }
        };
    }

    if props.fields.is_empty() {
        return rsx! {
            p { class: "text-xs text-base-content/50 py-2",
                "未发现需要补填的敏感字段"
            }
        };
    }

    rsx! {
        div { class: "py-2 space-y-2",
            for field in props.fields.iter() {
                {
                    // EventHandler（Callback）是 Copy：可直接被每轮的 move 闭包捕获，
                    // 不需要 clone，也不会在第二次迭代被判「use of moved value」
                    let field_key = field.key.clone();
                    let label = field.label.clone();
                    let hint = field.hint.clone();
                    let current = values.read().get(&field.key).cloned().unwrap_or_default();
                    let on_change = props.on_change;
                    rsx! {
                        div { key: "{field_key}", class: "form-control w-full",
                            label { class: "form-label",
                                span { "{label}" }
                            }
                            input {
                                class: "input input-bordered w-full",
                                r#type: "password",
                                // 避免浏览器把登录密码自动填进这些字段
                                autocomplete: "new-password",
                                value: "{current}",
                                placeholder: "{hint}",
                                oninput: {
                                    let k = field_key.clone();
                                    move |e| {
                                        values.write().insert(k.clone(), e.value());
                                        on_change.call(values.read().clone());
                                    }
                                },
                            }
                        }
                    }
                }
            }
            p { class: "text-base-content/60 text-xs",
                "快照不保存原始凭据（以占位符导出）。目标环境中已存在的账号 / Provider 可留空，将沿用其现有凭据；新建的必须填写，否则导入会失败。"
            }
        }
    }
}
