//! Seed 导入敏感字段（用户密码 / Provider API Key）补填区
//!
//! 快照导出时敏感字段一律不落盘，只写 `PENDING_INPUT` 占位符。后端
//! `apply_snapshot_to_db` 在写入前会调用 `validate_sensitive_fields`，凡是以
//! `PENDING_INPUT` 占位的项都必须由调用方补值，否则任务直接以「缺少敏感字段」失败
//! （见 `src/service/domain/system/seed/diff.rs`）。本模块负责把快照里的占位符
//! 解析成表单项，并把用户输入整理成后端期望的 `sensitive_values` Map。
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
                hint: "该账号将使用此密码登录（已存在则被重置）".to_string(),
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
                hint: format!("模型 {}", model_name),
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

/// 校验必填敏感字段是否已填写（DryRun 预演不写入，不校验）
///
/// 前置校验的意义在于把失败挡在提交之前：后端 `validate_sensitive_fields` 是在
/// 后台任务里跑的，失败后只能以任务 Failed 的形式出现，用户已看不到表单上下文。
pub fn check_sensitive_filled(
    fields: &[SensitiveField],
    values: &HashMap<String, String>,
    strategy: common::api::seed::ImportStrategy,
) -> Result<(), String> {
    if matches!(strategy, common::api::seed::ImportStrategy::DryRun) {
        return Ok(());
    }
    let missing: Vec<&str> = fields
        .iter()
        .filter(|f| {
            values
                .get(&f.key)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true)
        })
        .map(|f| f.label.as_str())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("请补填敏感字段：{}", missing.join("、")))
    }
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
    /// 是否必填（DryRun 预演为 false）
    #[props(default = true)]
    required: bool,
    /// 输入变化回调：每次输入上报全量值，父级只存不渲染
    on_change: EventHandler<HashMap<String, String>>,
}

/// 敏感字段输入区（无外壳，直接嵌入弹窗表单）
#[component]
pub fn SeedSensitiveFields(props: SeedSensitiveFieldsProps) -> Element {
    // 草稿只在本组件内流转；父级拿到值后仅做存储，不回写渲染，避免打断输入
    let mut values = use_signal(HashMap::<String, String>::new);

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
                    let required = props.required;
                    let on_change = props.on_change;
                    rsx! {
                        div { key: "{field_key}", class: "form-control w-full",
                            label { class: "form-label",
                                span { "{label}" }
                                if required {
                                    span { class: "text-error ml-1", "*" }
                                }
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
                if props.required {
                    "快照不保存原始凭据（敏感字段以占位符导出），导入时必须补填后才能写入"
                } else {
                    "仅预演（DryRun）不会写入数据，无需填写敏感字段"
                }
            }
        }
    }
}
