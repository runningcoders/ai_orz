//! 定时触发器管理 - 列表 + 创建/编辑

use dioxus::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone};

use crate::api::system::{
    create_cron_trigger, delete_cron_trigger, get_cron_trigger, list_cron_triggers,
    pause_cron_trigger, resume_cron_trigger, update_cron_trigger,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{CreateCronTriggerRequest, ListCronTriggersResponseItem, UpdateCronTriggerRequest};
use common::enums::TriggerType;

/// 时间戳格式化（秒级时间戳 → "YYYY-MM-DD HH:MM:SS"）
fn format_time(ts: i64) -> String {
    if ts <= 0 {
        return "-".to_string();
    }
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts.to_string())
}

/// 验证 JSON 字符串
fn validate_json(s: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str::<serde_json::Value>(s).map_err(|e| format!("JSON 解析失败: {}", e))
}

/// 根据 action 模板构造 payload
fn build_payload(template: &str, agent_id: &str, settle_limit: &str) -> Result<String, String> {
    match template {
        "agent_rest" => {
            if agent_id.trim().is_empty() {
                return Err("agent_id 不能为空".to_string());
            }
            let limit = if settle_limit.trim().is_empty() {
                10i64
            } else {
                settle_limit
                    .trim()
                    .parse::<i64>()
                    .map_err(|e| format!("settle_limit 必须为数字: {}", e))?
            };
            let payload = serde_json::json!({
                "action": "agent_rest",
                "extra": {
                    "agent_id": agent_id,
                    "settle_limit": limit,
                }
            });
            Ok(payload.to_string())
        }
        _ => Err(format!("未知模板: {}", template)),
    }
}

/// 解析 payload 为 (template, agent_id, settle_limit)
fn parse_payload(payload: &str) -> (String, String, String) {
    let v: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return ("custom".to_string(), String::new(), String::new()),
    };
    let action = v
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string();
    if action == "agent_rest" {
        let extra = v.get("extra").cloned().unwrap_or_default();
        let agent_id = extra
            .get("agent_id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let settle_limit = extra
            .get("settle_limit")
            .and_then(|x| x.as_i64())
            .map(|n| n.to_string())
            .unwrap_or_else(|| "10".to_string());
        ("agent_rest".to_string(), agent_id, settle_limit)
    } else {
        ("custom".to_string(), String::new(), String::new())
    }
}

fn trigger_type_text(t: TriggerType) -> &'static str {
    match t {
        TriggerType::Once => "一次性",
        TriggerType::Cron => "Cron",
        TriggerType::Interval => "间隔",
    }
}

fn trigger_type_badge_class(t: TriggerType) -> &'static str {
    match t {
        TriggerType::Once => "badge-info",
        TriggerType::Cron => "badge-success",
        TriggerType::Interval => "badge-warning",
    }
}

fn schedule_text(
    t: TriggerType,
    cron: &str,
    interval: Option<i64>,
    run_at: Option<i64>,
) -> String {
    match t {
        TriggerType::Cron => cron.to_string(),
        TriggerType::Interval => format!("每 {} 秒", interval.unwrap_or(0)),
        TriggerType::Once => match run_at {
            Some(ts) => format_time(ts),
            None => "-".to_string(),
        },
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TriggerEditMode {
    Create,
    Edit(String),
}

#[component]
pub fn SystemTriggers() -> Element {
    let mut triggers = use_signal(Vec::<ListCronTriggersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut show_modal = use_signal(|| false);
    let mut edit_mode = use_signal(|| TriggerEditMode::Create);

    // 表单状态
    let mut form_name = use_signal(String::new);
    let mut form_type = use_signal(|| "cron".to_string());
    let mut form_cron = use_signal(String::new);
    let mut form_interval = use_signal(|| "300".to_string());
    let mut form_template = use_signal(|| "agent_rest".to_string());
    let mut form_agent_id = use_signal(String::new);
    let mut form_settle_limit = use_signal(|| "10".to_string());
    let mut form_payload = use_signal(String::new);
    let mut json_error = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut loading_detail = use_signal(|| false);

    let toast = use_toast();

    // 初始加载
    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_cron_triggers().await {
                Ok(list) => triggers.set(list.triggers),
                Err(e) => toast.error(&format!("加载触发器列表失败: {}", e)),
            }
            loading.set(false);
        });
    });

    let handle_submit = move |_| {
        let name = form_name().trim().to_string();
        if name.is_empty() {
            toast.error("触发器名称不能为空");
            return;
        }

        // 构建 payload
        let payload_str = if form_template() == "agent_rest" {
            match build_payload(&form_template(), &form_agent_id(), &form_settle_limit()) {
                Ok(p) => p,
                Err(e) => {
                    json_error.set(e.clone());
                    toast.error(&e);
                    return;
                }
            }
        } else {
            let p = form_payload();
            if p.trim().is_empty() {
                let msg = "Payload JSON 不能为空";
                json_error.set(msg.to_string());
                toast.error(msg);
                return;
            }
            match validate_json(&p) {
                Ok(_) => p,
                Err(e) => {
                    json_error.set(e.clone());
                    toast.error(&e);
                    return;
                }
            }
        };
        json_error.set(String::new());

        let trigger_type = match form_type().as_str() {
            "once" => TriggerType::Once,
            "interval" => TriggerType::Interval,
            _ => TriggerType::Cron,
        };

        let cron_expression = if form_type() == "cron" {
            let c = form_cron().trim().to_string();
            if c.is_empty() {
                toast.error("Cron 表达式不能为空");
                return;
            }
            Some(c)
        } else {
            None
        };

        let interval_seconds = if form_type() == "interval" {
            match form_interval().trim().parse::<i64>() {
                Ok(n) if n > 0 => Some(n),
                _ => {
                    toast.error("间隔秒数必须为正整数");
                    return;
                }
            }
        } else {
            None
        };

        let run_at = if form_type() == "once" {
            Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            )
        } else {
            None
        };

        submitting.set(true);
        let mode = edit_mode();
        spawn(async move {
            let result = match &mode {
                TriggerEditMode::Create => {
                    let req = CreateCronTriggerRequest {
                        name,
                        trigger_type,
                        cron_expression,
                        interval_seconds,
                        run_at,
                        payload: payload_str,
                    };
                    create_cron_trigger(req).await.map(|_| ())
                }
                TriggerEditMode::Edit(id) => {
                    let req = UpdateCronTriggerRequest {
                        trigger_id: id.clone(),
                        name: Some(name),
                        trigger_type: Some(trigger_type),
                        cron_expression,
                        interval_seconds,
                        run_at,
                        payload: Some(payload_str),
                    };
                    update_cron_trigger(id, req).await.map(|_| ())
                }
            };
            match result {
                Ok(_) => {
                    let msg = if matches!(mode, TriggerEditMode::Create) {
                        "创建触发器成功"
                    } else {
                        "更新触发器成功"
                    };
                    toast.success(msg);
                    show_modal.set(false);
                    match list_cron_triggers().await {
                        Ok(list) => triggers.set(list.triggers),
                        Err(e) => toast.error(&format!("刷新列表失败: {}", e)),
                    }
                }
                Err(e) => {
                    toast.error(&format!("操作失败: {}", e));
                }
            }
            submitting.set(false);
        });
    };

    let triggers_list = triggers.read().clone();
    let cron_presets: [(&str, &str); 6] = [
        ("每分钟", "* * * * *"),
        ("每小时", "0 * * * *"),
        ("每天 0 点", "0 0 * * *"),
        ("每天 9 点", "0 9 * * *"),
        ("每周一 9 点", "0 9 * * 1"),
        ("每月 1 号", "0 0 1 * *"),
    ];

    let modal_title = match edit_mode() {
        TriggerEditMode::Create => "创建触发器".to_string(),
        TriggerEditMode::Edit(_) => "编辑触发器".to_string(),
    };

    let json_placeholder = r#"{"action":"agent_rest","extra":{"agent_id":"xxx","settle_limit":10}}"#;

    rsx! {
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "定时触发器" }
                div { class: "page-header-actions",
                    button {
                        class: "btn btn-ghost btn-sm",
                        onclick: move |_| {
                            loading.set(true);
                            spawn(async move {
                                match list_cron_triggers().await {
                                    Ok(list) => {
                                        triggers.set(list.triggers);
                                        toast.success("刷新成功");
                                    }
                                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                }
                                loading.set(false);
                            });
                        },
                        "🔄 刷新"
                    }
                    button {
                        class: "btn btn-accent",
                        onclick: move |_| {
                            form_name.set(String::new());
                            form_type.set("cron".to_string());
                            form_cron.set(String::new());
                            form_interval.set("300".to_string());
                            form_template.set("agent_rest".to_string());
                            form_agent_id.set(String::new());
                            form_settle_limit.set("10".to_string());
                            form_payload.set(String::new());
                            json_error.set(String::new());
                            edit_mode.set(TriggerEditMode::Create);
                            show_modal.set(true);
                        },
                        "+ 创建触发器"
                    }
                }
            }
            if loading() {
                Loading {}
            } else if triggers_list.is_empty() {
                EmptyState { icon: "⏰".to_string(), message: "暂无触发器".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "类型" }
                        th { "调度信息" }
                        th { "状态" }
                        th { "下次执行" }
                        th { "上次执行" }
                        th { "操作" }
                    }}
                    tbody {
                        for t in triggers_list.iter() {
                            {
                                let id = t.id.clone();
                                let name = t.name.clone();
                                let trigger_type = t.trigger_type;
                                let cron_expr = t.cron_expression.clone().unwrap_or_default();
                                let interval_seconds = t.interval_seconds;
                                let run_at = t.run_at;
                                let is_enabled = t.is_enabled;
                                let next_run_at = t.next_run_at;
                                let last_run_at = t.last_run_at;
                                let id_pause = id.clone();
                                let id_resume = id.clone();
                                let id_delete = id.clone();
                                let id_edit = id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { class: "detail-table-value-bold", "data-label": "名称", "{name}" }
                                        td { "data-label": "类型",
                                            span {
                                                class: "badge trigger-type-badge {trigger_type_badge_class(trigger_type)}",
                                                "{trigger_type_text(trigger_type)}"
                                            }
                                        }
                                        td { "data-label": "调度信息",
                                            span { class: "text-mono",
                                                "{schedule_text(trigger_type, &cron_expr, interval_seconds, run_at)}"
                                            }
                                        }
                                        td { "data-label": "状态",
                                            if is_enabled {
                                                span { class: "badge badge-success", "运行中" }
                                            } else {
                                                span { class: "badge badge-neutral", "暂停" }
                                            }
                                        }
                                        td { "data-label": "下次执行", span { class: "text-mono text-muted", "{format_time(next_run_at)}" } }
                                        td { "data-label": "上次执行",
                                            match last_run_at {
                                                Some(ts) => rsx! {
                                                    span { class: "text-mono text-muted", "{format_time(ts)}" }
                                                },
                                                None => rsx! {
                                                    span { class: "text-muted", "-" }
                                                },
                                            }
                                        }
                                        td { "data-label": "操作",
                                            button {
                                                class: "btn btn-ghost btn-sm",
                                                onclick: move |_| {
                                                    let edit_id = id_edit.clone();
                                                    // 重置表单
                                                    form_name.set(String::new());
                                                    form_type.set("cron".to_string());
                                                    form_cron.set(String::new());
                                                    form_interval.set("300".to_string());
                                                    form_template.set("agent_rest".to_string());
                                                    form_agent_id.set(String::new());
                                                    form_settle_limit.set("10".to_string());
                                                    form_payload.set(String::new());
                                                    json_error.set(String::new());
                                                    // 进入编辑模式
                                                    edit_mode.set(TriggerEditMode::Edit(edit_id.clone()));
                                                    show_modal.set(true);
                                                    loading_detail.set(true);
                                                    spawn(async move {
                                                        match get_cron_trigger(&edit_id).await {
                                                            Ok(detail) => {
                                                                form_name.set(detail.name.clone());
                                                                let t_str = match detail.trigger_type {
                                                                    TriggerType::Once => "once",
                                                                    TriggerType::Cron => "cron",
                                                                    TriggerType::Interval => "interval",
                                                                };
                                                                form_type.set(t_str.to_string());
                                                                if let Some(c) = &detail.cron_expression {
                                                                    form_cron.set(c.clone());
                                                                }
                                                                if let Some(i) = detail.interval_seconds {
                                                                    form_interval.set(i.to_string());
                                                                }
                                                                let (tpl, agent_id, settle_limit) = parse_payload(&detail.payload);
                                                                form_template.set(tpl.clone());
                                                                if tpl == "agent_rest" {
                                                                    form_agent_id.set(agent_id);
                                                                    form_settle_limit.set(settle_limit);
                                                                } else {
                                                                    form_payload.set(detail.payload.clone());
                                                                }
                                                            }
                                                            Err(e) => {
                                                                toast.error(&format!("加载详情失败: {}", e));
                                                                show_modal.set(false);
                                                            }
                                                        }
                                                        loading_detail.set(false);
                                                    });
                                                },
                                                "编辑"
                                            }
                                            if is_enabled {
                                                button {
                                                    class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let pid = id_pause.clone();
                                                        spawn(async move {
                                                            match pause_cron_trigger(&pid).await {
                                                                Ok(_) => {
                                                                    toast.success("已暂停");
                                                                    match list_cron_triggers().await {
                                                                        Ok(list) => triggers.set(list.triggers),
                                                                        Err(e) => toast.error(&format!("刷新列表失败: {}", e)),
                                                                    }
                                                                }
                                                                Err(e) => toast.error(&format!("暂停失败: {}", e)),
                                                            }
                                                        });
                                                    },
                                                    "暂停"
                                                }
                                            } else {
                                                button {
                                                    class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let rid = id_resume.clone();
                                                        spawn(async move {
                                                            match resume_cron_trigger(&rid).await {
                                                                Ok(_) => {
                                                                    toast.success("已恢复");
                                                                    match list_cron_triggers().await {
                                                                        Ok(list) => triggers.set(list.triggers),
                                                                        Err(e) => toast.error(&format!("刷新列表失败: {}", e)),
                                                                    }
                                                                }
                                                                Err(e) => toast.error(&format!("恢复失败: {}", e)),
                                                            }
                                                        });
                                                    },
                                                    "恢复"
                                                }
                                            }
                                            button {
                                                class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let did = id_delete.clone();
                                                    spawn(async move {
                                                        match delete_cron_trigger(&did).await {
                                                            Ok(_) => {
                                                                toast.success("删除成功");
                                                                match list_cron_triggers().await {
                                                                    Ok(list) => triggers.set(list.triggers),
                                                                    Err(e) => toast.error(&format!("刷新列表失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(&format!("删除失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                "删除"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Modal {
            title: modal_title,
            show: show_modal(),
            on_close: move |_| show_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button {
                    class: "btn btn-accent",
                    disabled: submitting() || loading_detail(),
                    onclick: handle_submit,
                    if submitting() {
                        "保存中..."
                    } else if loading_detail() {
                        "加载中..."
                    } else {
                        "保存"
                    }
                }
            },
            if loading_detail() {
                Loading {}
            } else {
                div {
                    div { class: "form-group",
                        label { class: "form-label", "触发器名称 *" }
                        input {
                            class: "form-input",
                            value: "{form_name}",
                            placeholder: "例：每日 9 点沉淀 Agent 记忆",
                            oninput: move |e| form_name.set(e.value())
                        }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "触发类型" }
                        select {
                            class: "form-select",
                            value: "{form_type}",
                            onchange: move |e| form_type.set(e.value()),
                            option { value: "cron", "Cron 表达式" }
                            option { value: "interval", "固定间隔" }
                            option { value: "once", "一次性" }
                        }
                    }
                    if form_type() == "cron" {
                        div { class: "form-group",
                            label { class: "form-label", "Cron 表达式 *" }
                            input {
                                class: "form-input text-mono",
                                value: "{form_cron}",
                                placeholder: "0 9 * * *（分 时 日 月 周）",
                                oninput: move |e| form_cron.set(e.value())
                            }
                            div { class: "cron-presets",
                                for (label, expr) in cron_presets.iter() {
                                    {
                                        let expr_clone = expr.to_string();
                                        rsx! {
                                            button {
                                                class: "btn btn-ghost btn-sm cron-preset-btn",
                                                onclick: move |_| {
                                                    form_cron.set(expr_clone.clone());
                                                },
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if form_type() == "interval" {
                        div { class: "form-group",
                            label { class: "form-label", "间隔秒数 *" }
                            input {
                                class: "form-input",
                                r#type: "number",
                                value: "{form_interval}",
                                placeholder: "300（即 5 分钟）",
                                oninput: move |e| form_interval.set(e.value())
                            }
                        }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "Action 模板" }
                        select {
                            class: "form-select",
                            value: "{form_template}",
                            onchange: move |e| form_template.set(e.value()),
                            option { value: "agent_rest", "agent_rest（Agent 休息与沉淀）" }
                            option { value: "custom", "自定义 JSON" }
                        }
                    }
                    if form_template() == "agent_rest" {
                        div { class: "form-group",
                            label { class: "form-label", "Agent ID *" }
                            input {
                                class: "form-input",
                                value: "{form_agent_id}",
                                placeholder: "Agent 的唯一 ID",
                                oninput: move |e| form_agent_id.set(e.value())
                            }
                        }
                        div { class: "form-group",
                            label { class: "form-label", "settle_limit（沉淀上限）" }
                            input {
                                class: "form-input",
                                r#type: "number",
                                value: "{form_settle_limit}",
                                placeholder: "10",
                                oninput: move |e| form_settle_limit.set(e.value())
                            }
                            div { class: "form-hint", "单次沉淀的短期记忆条数上限，默认 10" }
                        }
                    } else {
                        div { class: "form-group",
                            label { class: "form-label", "Payload JSON *" }
                            textarea {
                                class: "form-textarea text-mono",
                                style: "min-height: 120px;",
                                value: "{form_payload}",
                                placeholder: "{json_placeholder}",
                                oninput: move |e| {
                                    let v = e.value();
                                    if v.trim().is_empty() {
                                        json_error.set(String::new());
                                    } else {
                                        match validate_json(&v) {
                                            Ok(_) => json_error.set(String::new()),
                                            Err(err) => json_error.set(err),
                                        }
                                    }
                                    form_payload.set(v);
                                }
                            }
                            if !json_error().is_empty() {
                                div { class: "json-error", "{json_error()}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
