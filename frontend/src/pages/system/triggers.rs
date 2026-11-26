//! 定时触发器管理 - 列表 + 创建/编辑

use crate::components::hud::{HudPanel, HudSection};
use dioxus::prelude::*;

use crate::api::hr::{list_agents, search_agents};
use crate::api::system::{
    create_cron_trigger, create_schedule, delete_cron_trigger, get_cron_trigger,
    list_cron_triggers, pause_cron_trigger, query_schedule, resume_cron_trigger,
    update_cron_trigger, update_schedule,
};
use crate::components::SearchableSelect;
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::date_time_input::DateTimeInput;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    CreateCronTriggerRequest, ListAgentsRequest, ListCronTriggersResponseItem,
    QueryScheduleRequest, QueryScheduleResponse, SYSTEM_ACTION_TEMPLATES, ScheduleAgentTaskRequest,
    ScheduleOccurrence, SearchAgentsRequest, SystemActionTemplate, UpdateCronTriggerRequest,
    UpdateScheduleRequest, system_action_template,
};
use common::enums::{CreatorType, ScheduleDateTerm, TriggerBehavior, TriggerType};
use common::schedule::{calendar_day_key, calendar_day_keys, format_time_hm_in_tz};

/// 时间戳格式化（**毫秒级**时间戳 → "YYYY-MM-DD HH:MM:SS"）
///
/// 单位与后端 `next_run_at`/`last_run_at`/`created_at` 一致（毫秒，SSOT：`common::schedule`）；
/// 此前误按秒解析 `chrono::Local.timestamp_opt(ts, 0)`，导致展示时间整体偏大
/// （毫秒值当秒 → 年份溢出到数万年）。非正值（未执行过/未排期）显示 "-"。
fn format_time(ts_ms: i64) -> String {
    if ts_ms <= 0 {
        return "-".to_string();
    }
    crate::utils::time::format_datetime_full(ts_ms)
}

/// 验证 JSON 字符串
fn validate_json(s: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str::<serde_json::Value>(s).map_err(|e| format!("JSON 解析失败: {}", e))
}

/// datetime-local 字符串 → Unix 毫秒（本地时区）。
/// 日程一次性触发需要用户指定具体时刻；`datetime-local` 的 value 形如
/// "2026-11-25T09:00"，用 chrono 按本地时区解析后转毫秒，与后端 run_at 口径一致。
fn parse_local_datetime_ms(value: &str) -> Option<i64> {
    use chrono::{Local, TimeZone};
    let naive = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M").ok()?;
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp_millis())
}

/// Unix 毫秒 → datetime-local 字符串（编辑表单回填用）
fn format_local_datetime(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(ts_ms / 1000, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%dT%H:%M").to_string(),
        _ => String::new(),
    }
}

/// 日程日历视图模式
#[derive(Debug, Clone, Copy, PartialEq)]
enum CalendarView {
    /// 规则列表（原有视图）
    List,
    /// 昨天 / 今天 / 明天三列
    ThreeDays,
    /// 本周一至周日七列
    Week,
}

/// 星期中文标签（周一到周日）
fn weekday_label(key: &str) -> &'static str {
    use chrono::{Datelike, NaiveDate};
    NaiveDate::parse_from_str(key, "%Y-%m-%d")
        .map(|d| match d.weekday() {
            chrono::Weekday::Mon => "周一",
            chrono::Weekday::Tue => "周二",
            chrono::Weekday::Wed => "周三",
            chrono::Weekday::Thu => "周四",
            chrono::Weekday::Fri => "周五",
            chrono::Weekday::Sat => "周六",
            chrono::Weekday::Sun => "周日",
        })
        .unwrap_or("")
}

/// 根据 action 模板构造 payload。
///
/// 系统维护动作的 payload 格式由后端 consumer 固定为
/// `{"action":"<id>","extra":{...}}`，表单选模板即可，不必也不该手拼 JSON。
fn build_payload(template: &str, agent_id: &str, settle_limit: &str) -> Result<String, String> {
    let tpl = system_action_template(template)
        .ok_or_else(|| format!("未知系统动作模板: {}", template))?;
    if tpl.action == "agent_rest" {
        let limit = if settle_limit.trim().is_empty() {
            10i64
        } else {
            settle_limit
                .trim()
                .parse::<i64>()
                .map_err(|e| format!("settle_limit 必须为数字: {}", e))?
        };
        let mut extra = serde_json::json!({ "settle_limit": limit });
        // 留空 = 系统默认形态：后端扫描全部待沉淀 Agent，不针对单个
        if !agent_id.trim().is_empty() {
            extra["agent_id"] = serde_json::json!(agent_id.trim());
        }
        return Ok(serde_json::json!({ "action": tpl.action, "extra": extra }).to_string());
    }
    Ok(serde_json::json!({ "action": tpl.action, "extra": {} }).to_string())
}

/// 解析 payload 为 (template, agent_id, settle_limit)
///
/// 非系统维护 action（自定义值 / 历史外框）一律回落 `custom`，由 textarea 原样保留。
fn parse_payload(payload: &str) -> (String, String, String) {
    let v: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return ("custom".to_string(), String::new(), String::new()),
    };
    let action = v.get("action").and_then(|a| a.as_str()).unwrap_or("");
    if system_action_template(action).is_none() {
        return ("custom".to_string(), String::new(), String::new());
    }
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
    (action.to_string(), agent_id, settle_limit)
}

/// 当前选中的系统动作模板（`custom` 或无匹配返回 None）
fn selected_template(value: &str) -> Option<&'static SystemActionTemplate> {
    system_action_template(value)
}

fn trigger_type_text(t: TriggerType) -> &'static str {
    match t {
        TriggerType::Once => "一次性",
        TriggerType::Cron => "Cron",
        TriggerType::Interval => "间隔",
        TriggerType::Schedule => "日程",
    }
}

fn trigger_type_badge_class(t: TriggerType) -> &'static str {
    // 触发类型是「类别标签」，统一走中性 orz-tag chip（与状态徽章区分）
    match t {
        TriggerType::Once => "badge orz-tag badge-sm",
        TriggerType::Cron => "badge orz-tag badge-sm",
        TriggerType::Interval => "badge orz-tag badge-sm",
        TriggerType::Schedule => "badge orz-tag badge-sm",
    }
}

fn schedule_text(t: TriggerType, cron: &str, interval: Option<i64>, run_at: Option<i64>) -> String {
    match t {
        TriggerType::Cron => cron.to_string(),
        TriggerType::Interval => format!("每 {} 秒", interval.unwrap_or(0)),
        TriggerType::Once => match run_at {
            Some(ts) => format_time(ts),
            None => "-".to_string(),
        },
        // 日程是形态标签，时间规则三选一（cron > interval > run_at）
        TriggerType::Schedule => {
            if !cron.is_empty() {
                cron.to_string()
            } else if let Some(seconds) = interval {
                format!("每 {} 秒", seconds)
            } else {
                run_at.map(format_time).unwrap_or_else(|| "-".to_string())
            }
        }
    }
}

/// How: 行为标签（可读文案，不暴露枚举裸值）
fn behavior_text(b: TriggerBehavior) -> &'static str {
    match b {
        TriggerBehavior::SyncMaintenance => "同步维护",
        TriggerBehavior::AsyncNotify => "异步通知",
        TriggerBehavior::AsyncAgentWakeup => "Agent 唤醒",
    }
}

/// 行为标签样式：与触发类型一样是「类别标签」，统一走中性 orz-tag chip。
/// 异步两族用 info 变体做轻区分（与状态徽章的成败色脱钩）。
fn behavior_badge_class(b: TriggerBehavior) -> &'static str {
    match b {
        TriggerBehavior::SyncMaintenance => "badge orz-tag badge-sm",
        TriggerBehavior::AsyncNotify | TriggerBehavior::AsyncAgentWakeup => {
            "badge orz-tag badge-sm badge-info"
        }
    }
}

/// 形态判定：系统维度 = 同步维护模板（存量 6 条，Who 组为 NULL）。
/// 日程 = 异步模板（Who 组完整）。这是 A-设计 §10「同页分形态」的判据。
fn is_system_shape(behavior: TriggerBehavior) -> bool {
    behavior == TriggerBehavior::SyncMaintenance
}

/// 表单的业务类型：日程 vs 系统定时任务（两个不同概念，不是时间规则）
fn shape_of_trigger(t: &ListCronTriggersResponseItem) -> &'static str {
    // 与列表筛选、Who 列共用同一判据；旧表单建的 trigger_type=Schedule 但
    // 行为仍是 SyncMaintenance 的记录也按系统项处理（它没有归属用户，不能走日程接口）。
    if is_system_shape(t.behavior) {
        "system"
    } else {
        "schedule"
    }
}

/// 时间规则三选一（cron > interval > run_at）→ 表单单选值
fn time_rule_of(cron: Option<&str>, interval: Option<i64>, _run_at: Option<i64>) -> &'static str {
    if cron.is_some_and(|c| !c.trim().is_empty()) {
        "cron"
    } else if interval.is_some() {
        "interval"
    } else {
        "once"
    }
}

/// Agent 选择组件产出 "名称 (ID)"，反解出 ID 写入表单
fn agent_id_from_option(selection: &str) -> String {
    match selection.rfind('(') {
        Some(i) if selection.ends_with(')') => selection[i + 1..selection.len() - 1].to_string(),
        _ => selection.to_string(),
    }
}

/// 日历实例与列表共用同一套筛选语义。
///
/// 「形态」是行为模板的分区（系统 = `SyncMaintenance`），不是数据维度，
/// 所以日历实例拿同一套判据在本地过滤即可 —— 日历只是把 query 返回的
/// 区间实例按天分桶，不需要另一条数据通路，也不需要额外接口参数。
fn occurrence_matches_filters(
    occ: &ScheduleOccurrence,
    shape: ShapeFilter,
    behavior: BehaviorFilter,
) -> bool {
    let shape_ok = match shape {
        ShapeFilter::All => true,
        ShapeFilter::System => is_system_shape(occ.behavior),
        ShapeFilter::Schedule => !is_system_shape(occ.behavior),
    };
    let behavior_ok = match behavior {
        BehaviorFilter::All => true,
        BehaviorFilter::Sync => occ.behavior == TriggerBehavior::SyncMaintenance,
        BehaviorFilter::Notify => occ.behavior == TriggerBehavior::AsyncNotify,
        BehaviorFilter::Wakeup => occ.behavior == TriggerBehavior::AsyncAgentWakeup,
    };
    shape_ok && behavior_ok
}

/// CreatorType 可读文案
fn creator_type_text(c: Option<CreatorType>) -> &'static str {
    match c {
        Some(CreatorType::Agent) => "Agent",
        Some(CreatorType::User) => "用户",
        Some(CreatorType::System) => "系统",
        None => "-",
    }
}

/// `标签/ID` 文案（ID 缺失时只留标签）
fn label_with_id(label: &str, id: Option<&str>) -> String {
    match id.filter(|s| !s.is_empty()) {
        Some(id) => format!("{label}/{id}"),
        None => label.to_string(),
    }
}

/// Who 列文案：创建者 → 负责 Agent → 归属用户。
///
/// 三个字段语义不同，不能互相顶替：创建者是「谁建的」（可以是用户），
/// 负责 Agent 是「到点谁执行」，归属用户是「日程算谁的、提醒推给谁」。
/// 系统触发器三者皆空 → 显示 "系统"。
fn who_text(
    creator_type: Option<CreatorType>,
    creator_id: Option<&str>,
    agent_id: Option<&str>,
    owner_user_id: Option<&str>,
) -> String {
    if creator_type.is_none() && agent_id.is_none() && owner_user_id.is_none() {
        return "系统".to_string();
    }
    let mut parts = vec![label_with_id(creator_type_text(creator_type), creator_id)];
    if let Some(agent) = agent_id.filter(|s| !s.is_empty()) {
        parts.push(format!("负责 Agent/{agent}"));
    }
    if let Some(owner) = owner_user_id.filter(|s| !s.is_empty()) {
        parts.push(format!("归属 {owner}"));
    }
    parts.join(" → ")
}

/// When: 生效窗口文案（无窗口返回空串）
fn window_text(start_at: Option<i64>, end_at: Option<i64>) -> String {
    match (start_at, end_at) {
        (None, None) => String::new(),
        (Some(s), None) => format!("起 {}", format_time(s)),
        (None, Some(e)) => format!("止 {}", format_time(e)),
        (Some(s), Some(e)) => format!("{} ~ {}", format_time(s), format_time(e)),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TriggerEditMode {
    Create,
    Edit(String),
}

/// 列表形态筛选
#[derive(Debug, Clone, Copy, PartialEq)]
enum ShapeFilter {
    All,
    System,
    Schedule,
}

/// 列表行为筛选
#[derive(Debug, Clone, Copy, PartialEq)]
enum BehaviorFilter {
    All,
    Sync,
    Notify,
    Wakeup,
}

#[component]
pub fn SystemTriggers() -> Element {
    let mut triggers = use_signal(Vec::<ListCronTriggersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut show_modal = use_signal(|| false);
    let mut edit_mode = use_signal(|| TriggerEditMode::Create);

    // P3 列表筛选：形态（系统/日程）+ 行为模板
    let mut shape_filter = use_signal(|| ShapeFilter::All);
    let mut behavior_filter = use_signal(|| BehaviorFilter::All);

    // P4 日程日历视图：列表 / 三天 / 周
    let mut calendar_view = use_signal(|| CalendarView::List);
    let mut schedule_query = use_signal(|| None::<QueryScheduleResponse>);
    let mut calendar_loading = use_signal(|| false);

    // 表单状态：业务类型（日程 / 系统定时任务）与时间规则（cron / interval / once）是两个维度
    let mut form_name = use_signal(String::new);
    let mut form_shape = use_signal(|| "system".to_string());
    let mut form_type = use_signal(|| "cron".to_string());
    let mut form_cron = use_signal(String::new);
    let mut form_interval = use_signal(|| "300".to_string());
    let mut form_template = use_signal(|| "agent_rest".to_string());
    let mut form_agent_id = use_signal(String::new);
    let mut form_settle_limit = use_signal(|| "10".to_string());
    let mut form_payload = use_signal(String::new);
    let mut form_behavior = use_signal(|| "wakeup".to_string());
    let mut form_description = use_signal(String::new);
    // 一次性触发的本地时间（datetime-local 字符串），提交时解析为毫秒 run_at
    let mut form_run_at_local = use_signal(String::new);
    // Agent 选择组件（复用通用 SearchableSelect）：预载列表 + 输入时搜索
    let mut agent_options = use_signal(Vec::<String>::new);
    let mut agent_loading = use_signal(|| false);
    let mut json_error = use_signal(String::new);
    // 当前选中的系统动作模板（`custom` = None），供表单区分参数区
    let active_template = selected_template(&form_template());
    let mut submitting = use_signal(|| false);
    let mut loading_detail = use_signal(|| false);
    // 当前编辑项的详情快照（P3 四组只读概览；系统项新增字段由此只读呈现）
    let mut detail_snapshot = use_signal(|| None::<ListCronTriggersResponseItem>);

    let toast = use_toast();

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // 初始加载
    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_cron_triggers().await {
                Ok(list) => triggers.set(list.triggers),
                Err(e) => toast.error(format!("加载触发器列表失败: {}", e)),
            }
            loading.set(false);
        });
    });

    // Agent 候选：入场先拉一页，输入时再按关键词搜索（沿用知识图谱页的口径）
    use_effect(move || {
        spawn(async move {
            agent_loading.set(true);
            match list_agents(ListAgentsRequest::default()).await {
                Ok(page) => agent_options.set(
                    page.items
                        .iter()
                        .map(|a| format!("{} ({})", a.name, a.id))
                        .collect(),
                ),
                Err(e) => toast.error(format!("加载 Agent 列表失败: {}", e)),
            }
            agent_loading.set(false);
        });
    });

    let handle_agent_search = move |keyword: String| {
        let kw = keyword.trim().to_string();
        spawn(async move {
            agent_loading.set(true);
            let result = if kw.is_empty() {
                list_agents(ListAgentsRequest::default()).await
            } else {
                search_agents(&SearchAgentsRequest {
                    keyword: Some(kw),
                    ..Default::default()
                })
                .await
            };
            match result {
                Ok(page) => agent_options.set(
                    page.items
                        .iter()
                        .map(|a| format!("{} ({})", a.name, a.id))
                        .collect(),
                ),
                Err(e) => toast.error(format!("搜索 Agent 失败: {}", e)),
            }
            agent_loading.set(false);
        });
    };

    let mut handle_agent_select = move |selection: String| {
        form_agent_id.set(agent_id_from_option(&selection));
    };

    // 日历视图切换时按日期词拉取展开实例（相对日词由服务端按系统时区折算）
    use_effect(move || {
        let mode = calendar_view();
        if mode == CalendarView::List {
            return;
        }
        let term = match mode {
            CalendarView::ThreeDays => ScheduleDateTerm::ThreeDays,
            CalendarView::Week => ScheduleDateTerm::ThisWeek,
            CalendarView::List => ScheduleDateTerm::Today,
        };
        calendar_loading.set(true);
        spawn(async move {
            let req = QueryScheduleRequest {
                date_term: Some(term),
                limit: Some(500),
                ..Default::default()
            };
            match query_schedule(req).await {
                Ok(resp) => schedule_query.set(Some(resp)),
                Err(e) => toast.error(format!("加载日程失败: {}", e)),
            }
            calendar_loading.set(false);
        });
    });

    let handle_submit = move |_| {
        let name = form_name().trim().to_string();
        if name.is_empty() {
            toast.error("名称不能为空");
            return;
        }

        let shape = form_shape();
        // Action 模板 / payload 只在「系统定时任务」侧有意义；日程走独立入口
        let payload_str = if shape == "system" {
            if form_template() != "custom" {
                // 所有系统维护动作都由模板生成 payload，用户不碰 JSON
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
            }
        } else {
            String::new()
        };
        json_error.set(String::new());

        // 时间规则独立成一维，三选一：cron / interval / once（毫秒时间戳）
        let time_rule = form_type();
        let cron_expression = if time_rule == "cron" {
            let c = form_cron().trim().to_string();
            if c.is_empty() {
                toast.error("Cron 表达式不能为空");
                return;
            }
            Some(c)
        } else {
            None
        };
        let interval_seconds = if time_rule == "interval" {
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
        let run_at = if time_rule == "once" {
            match parse_local_datetime_ms(&form_run_at_local()) {
                Some(ts) => Some(ts),
                None => {
                    toast.error("请选择执行时间");
                    return;
                }
            }
        } else {
            None
        };

        let trigger_type = match time_rule.as_str() {
            "once" => TriggerType::Once,
            "interval" => TriggerType::Interval,
            _ => TriggerType::Cron,
        };
        let behavior = if form_behavior() == "notify" {
            TriggerBehavior::AsyncNotify
        } else {
            TriggerBehavior::AsyncAgentWakeup
        };
        let description = {
            let d = form_description().trim().to_string();
            if d.is_empty() { name.clone() } else { d }
        };
        let agent_id = form_agent_id().trim().to_string();

        submitting.set(true);
        let mode = edit_mode();
        spawn(async move {
            // 两个业务入口：日程走 /schedules（带身份与异步行为），系统定时任务走 CRUD
            let result = if shape == "schedule" {
                match &mode {
                    TriggerEditMode::Create => {
                        let req = ScheduleAgentTaskRequest {
                            title: name.clone(),
                            description,
                            payload: None,
                            cron_expression: cron_expression.clone(),
                            interval_seconds,
                            run_at,
                            start_at: None,
                            end_at: None,
                            agent_id: if agent_id.is_empty() {
                                None
                            } else {
                                Some(agent_id)
                            },
                            behavior: Some(behavior),
                            action: None,
                            max_runs: None,
                        };
                        create_schedule(req).await.map(|_| ())
                    }
                    TriggerEditMode::Edit(id) => {
                        let req = UpdateScheduleRequest {
                            trigger_id: id.clone(),
                            title: Some(name.clone()),
                            description: Some(description),
                            payload: None,
                            cron_expression: cron_expression.clone(),
                            interval_seconds,
                            run_at,
                            start_at: None,
                            end_at: None,
                            max_runs: None,
                            agent_id: if agent_id.is_empty() {
                                None
                            } else {
                                Some(agent_id)
                            },
                        };
                        update_schedule(req).await.map(|_| ())
                    }
                }
            } else {
                match &mode {
                    TriggerEditMode::Create => {
                        let req = CreateCronTriggerRequest {
                            name: name.clone(),
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
                            name: Some(name.clone()),
                            trigger_type: Some(trigger_type),
                            cron_expression,
                            interval_seconds,
                            run_at,
                            payload: Some(payload_str),
                        };
                        update_cron_trigger(req).await.map(|_| ())
                    }
                }
            };
            match result {
                Ok(_) => {
                    let msg = if matches!(mode, TriggerEditMode::Create) {
                        if shape == "schedule" {
                            "创建日程成功"
                        } else {
                            "创建系统定时任务成功"
                        }
                    } else if shape == "schedule" {
                        "更新日程成功"
                    } else {
                        "更新系统定时任务成功"
                    };
                    toast.success(msg);
                    show_modal.set(false);
                    match list_cron_triggers().await {
                        Ok(list) => triggers.set(list.triggers),
                        Err(e) => toast.error(format!("刷新列表失败: {}", e)),
                    }
                }
                Err(e) => {
                    toast.error(format!("操作失败: {}", e));
                }
            }
            submitting.set(false);
        });
    };

    let triggers_list = triggers.read().clone();
    let mut filtered_list: Vec<_> = triggers_list
        .iter()
        .filter(|t| {
            let shape_ok = match shape_filter() {
                ShapeFilter::All => true,
                ShapeFilter::System => is_system_shape(t.behavior),
                ShapeFilter::Schedule => !is_system_shape(t.behavior),
            };
            let behavior_ok = match behavior_filter() {
                BehaviorFilter::All => true,
                BehaviorFilter::Sync => t.behavior == TriggerBehavior::SyncMaintenance,
                BehaviorFilter::Notify => t.behavior == TriggerBehavior::AsyncNotify,
                BehaviorFilter::Wakeup => t.behavior == TriggerBehavior::AsyncAgentWakeup,
            };
            shape_ok && behavior_ok
        })
        .cloned()
        .collect();
    // 时间轴：按下次执行时间升序；无有效排期的记录沉底，不污染时间轴头部
    filtered_list.sort_by_key(|t| (t.next_run_at <= 0, t.next_run_at));
    // 触发器状态分布聚合（前端本地计算）
    let now_ms = crate::utils::time::now_ms();
    let enabled_count = triggers_list.iter().filter(|t| t.is_enabled).count();
    let disabled_count = triggers_list.iter().filter(|t| !t.is_enabled).count();
    let soon_count = triggers_list
        .iter()
        // next_run_at 为毫秒时间戳，阈值也须用毫秒（1 小时 = 3_600_000ms）
        .filter(|t| t.is_enabled && t.next_run_at > 0 && (t.next_run_at - now_ms).abs() < 3_600_000)
        .count();
    let trigger_status_slices: Vec<DonutSlice> = vec![
        DonutSlice {
            label: "启用".to_string(),
            value: enabled_count as u64,
            color: "#10b981".to_string(),
        },
        DonutSlice {
            label: "暂停".to_string(),
            value: disabled_count as u64,
            color: "#6b7280".to_string(),
        },
        DonutSlice {
            label: "1h 内执行".to_string(),
            value: soon_count as u64,
            color: "#fa520f".to_string(),
        },
    ];
    let cron_presets: [(&str, &str); 6] = [
        ("每分钟", "* * * * *"),
        ("每小时", "0 * * * *"),
        ("每天 0 点", "0 0 * * *"),
        ("每天 9 点", "0 9 * * *"),
        ("每周一 9 点", "0 9 * * 1"),
        ("每月 1 号", "0 0 1 * *"),
    ];

    let modal_title = match (edit_mode(), form_shape().as_str()) {
        (TriggerEditMode::Create, "schedule") => "新建日程".to_string(),
        (TriggerEditMode::Create, _) => "新建系统定时任务".to_string(),
        (TriggerEditMode::Edit(_), "schedule") => "编辑日程".to_string(),
        (TriggerEditMode::Edit(_), _) => "编辑系统定时任务".to_string(),
    };

    let json_placeholder =
        r#"{"action":"agent_rest","extra":{"agent_id":"xxx","settle_limit":10}}"#;

    rsx! {
        AppLayout {
        HudPanel { signal: Some(true), extra_class: Some("mb-4".to_string()),
            div { class: "card-body",
                HudSection { title: "📊 触发器状态分布".to_string() }
                DonutChart {
                    data: trigger_status_slices,
                    width: Some(400.0),
                    height: Some(220.0),
                    center_label: Some("触发器".to_string()),
                }
            }
        }
        HudPanel { signal: Some(true),
            title: Some("定时任务与日程".to_string()),
            actions: Some(rsx!{
                    button {
                        class: "btn hud-btn btn-ghost btn-sm",
                        onclick: move |_| {
                            loading.set(true);
                            spawn(async move {
                                match list_cron_triggers().await {
                                    Ok(list) => {
                                        triggers.set(list.triggers);
                                        toast.success("刷新成功");
                                    }
                                    Err(e) => toast.error(format!("刷新失败: {}", e)),
                                }
                                loading.set(false);
                            });
                        },
                        "🔄 刷新"
                    }
                    button {
                        class: "btn hud-btn btn-primary",
                        onclick: move |_| {
                            form_name.set(String::new());
                            form_shape.set("system".to_string());
                            form_type.set("cron".to_string());
                            form_cron.set(String::new());
                            form_interval.set("300".to_string());
                            form_template.set("agent_rest".to_string());
                            form_agent_id.set(String::new());
                            form_settle_limit.set("10".to_string());
                            form_payload.set(String::new());
                            form_behavior.set("wakeup".to_string());
                            form_description.set(String::new());
                            form_run_at_local.set(String::new());
                            // 清掉上一条编辑遗留的只读概览，否则创建弹窗会显示旧日程的四组快照
                            detail_snapshot.set(None);
                            json_error.set(String::new());
                            edit_mode.set(TriggerEditMode::Create);
                            show_modal.set(true);
                        },
                        "+ 新建任务/日程"
                    }
            }),
            // P4 视图切换：规则列表 / 三天 / 周
            div { class: "flex gap-2 mb-3 items-center",
                span { class: "text-xs text-base-content/60", "视图" }
                div { class: "join",
                    button {
                        class: "btn btn-xs join-item",
                        class: if calendar_view() == CalendarView::List { "btn-active" } else { "" },
                        onclick: move |_| calendar_view.set(CalendarView::List),
                        "列表"
                    }
                    button {
                        class: "btn btn-xs join-item",
                        class: if calendar_view() == CalendarView::ThreeDays { "btn-active" } else { "" },
                        onclick: move |_| {
                            // 日历只展开当前用户的异步日程：系统触发器与同步维护
                            // 永远不在结果里，把筛选重置回「全部」避免空列表误导
                            shape_filter.set(ShapeFilter::All);
                            behavior_filter.set(BehaviorFilter::All);
                            calendar_view.set(CalendarView::ThreeDays);
                        },
                        "三天"
                    }
                    button {
                        class: "btn btn-xs join-item",
                        class: if calendar_view() == CalendarView::Week { "btn-active" } else { "" },
                        onclick: move |_| {
                            shape_filter.set(ShapeFilter::All);
                            behavior_filter.set(BehaviorFilter::All);
                            calendar_view.set(CalendarView::Week);
                        },
                        "周视图"
                    }
                }
            }
            // P3 筛选栏：同页分形态（系统触发器 / 日程）+ 行为模板。
            // 日历视图不渲染这两个筛选：查询只返回当前用户的异步日程，
            // 「系统触发器」「同步维护」在日历里恒为空，留着只会误导。
            div { class: "flex gap-3 flex-wrap mb-3 items-center",
                if calendar_view() == CalendarView::List {
                    span { class: "text-xs text-base-content/60", "类型" }
                    select {
                        class: "select select-bordered select-sm",
                        onchange: move |e| {
                            shape_filter.set(match e.value().as_str() {
                                "system" => ShapeFilter::System,
                                "schedule" => ShapeFilter::Schedule,
                                _ => ShapeFilter::All,
                            });
                        },
                        option { value: "all", "全部形态" }
                        option { value: "system", "系统定时任务" }
                        option { value: "schedule", "日程" }
                    }
                    span { class: "text-xs text-base-content/60 ml-2", "行为" }
                    select {
                        class: "select select-bordered select-sm",
                        onchange: move |e| {
                            behavior_filter.set(match e.value().as_str() {
                                "sync" => BehaviorFilter::Sync,
                                "notify" => BehaviorFilter::Notify,
                                "wakeup" => BehaviorFilter::Wakeup,
                                _ => BehaviorFilter::All,
                            });
                        },
                        option { value: "all", "全部行为" }
                        option { value: "sync", "同步维护" }
                        option { value: "notify", "异步通知" }
                        option { value: "wakeup", "Agent 唤醒" }
                    }
                } else {
                    span { class: "text-xs text-base-content/60",
                        "日历只展开你自己的日程（不含系统触发器）"
                    }
                }
                span { class: "text-xs text-base-content/50 ml-auto",
                    "共 {filtered_list.len()} 条 / 总 {triggers_list.len()} 条"
                }
            }
            if calendar_view() != CalendarView::List {
                // P4 日程日历：按天分桶展示展开后的实例
                if calendar_loading() {
                    Loading {}
                } else if let Some(resp) = schedule_query() {
                    {
                        let day_count = if calendar_view() == CalendarView::Week { 7 } else { 3 };
                        let timezone = resp.timezone.clone();
                        let keys = calendar_day_keys(resp.from, day_count, &timezone)
                            .unwrap_or_default();
                        let mut buckets: Vec<(String, Vec<ScheduleOccurrence>)> = keys
                            .iter()
                            .map(|k| (k.clone(), Vec::<ScheduleOccurrence>::new()))
                            .collect();
                        // 形态/行为筛选在区间实例上就地生效（与列表同一判据）
                        let matched: Vec<&ScheduleOccurrence> = resp
                            .occurrences
                            .iter()
                            .filter(|occ| {
                                occurrence_matches_filters(
                                    occ,
                                    shape_filter(),
                                    behavior_filter(),
                                )
                            })
                            .collect();
                        for occ in matched.iter() {
                            // 与服务端展开用同一时区，避免浏览器本地时区不同导致分桶错位
                            if let Ok(key) = calendar_day_key(occ.run_at, &timezone)
                                && let Some((_, list)) =
                                    buckets.iter_mut().find(|(k, _)| *k == key)
                            {
                                list.push((*occ).clone());
                            }
                        }
                        // 服务端只保证全局升序；分桶后每天再显式按执行时间升序，形成日时间轴
                        for (_, list) in buckets.iter_mut() {
                            list.sort_by_key(|occ| occ.run_at);
                        }
                        rsx! {
                            div { class: "text-xs text-base-content/50 mb-2",
                                "时区 {resp.timezone} · 共 {matched.len()} 项日程实例（区间内 {resp.occurrences.len()} 条）"
                            }
                            div { class: "grid gap-3",
                                class: if day_count == 7 { "grid-cols-1 md:grid-cols-7" } else { "grid-cols-1 md:grid-cols-3" },
                                for (idx, (key, items)) in buckets.iter().enumerate() {
                                    div { class: "border border-base-300 rounded-lg p-2 min-h-24 bg-base-100",
                                        div { class: "text-xs font-medium mb-2 flex justify-between",
                                            span { "{key}" }
                                            span { class: "text-base-content/50",
                                                if calendar_view() == CalendarView::Week {
                                                    "{weekday_label(key)}"
                                                } else if idx == 1 {
                                                    "今天"
                                                } else if idx == 0 {
                                                    "昨天"
                                                } else {
                                                    "明天"
                                                }
                                            }
                                        }
                                        if items.is_empty() {
                                            div { class: "text-xs text-base-content/30", "无日程" }
                                        } else {
                                            div { class: "flex flex-col gap-1",
                                                for occ in items.iter() {
                                                    {
                                                        let time_label = format_time_hm_in_tz(
                                                            occ.run_at,
                                                            &timezone,
                                                        )
                                                        .unwrap_or_else(|_| "--:--".to_string());
                                                        rsx! {
                                                            div { class: "text-xs p-1.5 rounded bg-base-200 border-l-2 border-primary",
                                                                title: occ.description.clone(),
                                                                div { class: "font-medium",
                                                                    "{time_label} {occ.name}"
                                                                }
                                                                div { class: "text-base-content/50",
                                                                    if occ.behavior == TriggerBehavior::AsyncNotify { "通知" } else { "唤醒" }
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
                            if matched.is_empty() {
                                EmptyState {
                                    icon: "📅".to_string(),
                                    message: if resp.occurrences.is_empty() {
                                        "当前区间暂无日程实例".to_string()
                                    } else {
                                        "当前筛选条件下无日程实例".to_string()
                                    },
                                }
                            }
                        }
                    }
                } else {
                    EmptyState {
                        icon: "📅".to_string(),
                        message: "点击视图切换加载日程".to_string(),
                    }
                }
            } else if loading() {
                Loading {}
            } else if filtered_list.is_empty() {
                EmptyState {
                    icon: "⏰".to_string(),
                    message: if triggers_list.is_empty() {
                        "暂无触发器".to_string()
                    } else {
                        "当前筛选条件下无匹配项".to_string()
                    },
                }
            } else {
                table { class: "table hud-table table-zebra",
                    thead { tr {
                        th { "名称 / What" }
                        th { "形态 / How" }
                        th { "When" }
                        th { "Who" }
                        th { "状态" }
                        th { "下次执行" }
                        th { "上次执行" }
                        th { "操作" }
                    }}
                    tbody {
                        for t in filtered_list.iter() {
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
                                let behavior = t.behavior;
                                let action = t.action.clone();
                                let description = t.description.clone();
                                let window = window_text(t.start_at, t.end_at);
                                let who = who_text(
                                    t.creator_type,
                                    t.creator_id.as_deref(),
                                    t.agent_id.as_deref(),
                                    t.owner_user_id.as_deref(),
                                );
                                let id_pause = id.clone();
                                let id_resume = id.clone();
                                let id_delete = id.clone();
                                let id_edit = id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { class: "detail-table-value-bold", "data-label": "名称 / What",
                                            div { "{name}" }
                                            if !description.is_empty() {
                                                div { class: "text-xs text-base-content/60 max-w-xs truncate",
                                                    title: "{description}",
                                                    "{description}"
                                                }
                                            } else if let Some(action_label) = &action {
                                                div { class: "text-xs text-base-content/50 font-mono",
                                                    "{action_label}"
                                                }
                                            }
                                        }
                                        td { "data-label": "形态 / How",
                                            div { class: "flex flex-col gap-1 items-start",
                                                span {
                                                    class: "{trigger_type_badge_class(trigger_type)}",
                                                    "{trigger_type_text(trigger_type)}"
                                                }
                                                span {
                                                    class: "{behavior_badge_class(behavior)}",
                                                    "{behavior_text(behavior)}"
                                                }
                                            }
                                        }
                                        td { "data-label": "When",
                                            div { class: "font-mono",
                                                "{schedule_text(trigger_type, &cron_expr, interval_seconds, run_at)}"
                                            }
                                            if !window.is_empty() {
                                                div { class: "text-xs text-base-content/60",
                                                    "{window}"
                                                }
                                            }
                                        }
                                        td { "data-label": "Who",
                                            span { class: "text-xs text-base-content/70", "{who}" }
                                        }
                                        td { "data-label": "状态",
                                            if is_enabled {
                                                span { class: "badge hud-badge badge-success", "运行中" }
                                            } else {
                                                span { class: "badge hud-badge badge-neutral", "暂停" }
                                            }
                                        }
                                        td { "data-label": "下次执行", span { class: "font-mono text-base-content/70", "{format_time(next_run_at)}" } }
                                        td { "data-label": "上次执行",
                                            match last_run_at {
                                                Some(ts) => rsx! {
                                                    span { class: "font-mono text-base-content/70", "{format_time(ts)}" }
                                                },
                                                None => rsx! {
                                                    span { class: "text-base-content/70", "-" }
                                                },
                                            }
                                        }
                                        td { "data-label": "操作",
                                            button {
                                                class: "btn hud-btn btn-ghost btn-sm",
                                                onclick: move |_| {
                                                    let edit_id = id_edit.clone();
                                                    // 重置表单
                                                    form_name.set(String::new());
                                                    form_shape.set("system".to_string());
                                                    form_type.set("cron".to_string());
                                                    form_cron.set(String::new());
                                                    form_interval.set("300".to_string());
                                                    form_template.set("agent_rest".to_string());
                                                    form_agent_id.set(String::new());
                                                    form_settle_limit.set("10".to_string());
                                                    form_payload.set(String::new());
                                                    form_behavior.set("wakeup".to_string());
                                                    form_description.set(String::new());
                                                    form_run_at_local.set(String::new());
                                                    detail_snapshot.set(None);
                                                    json_error.set(String::new());
                                                    // 进入编辑模式
                                                    edit_mode.set(TriggerEditMode::Edit(edit_id.clone()));
                                                    show_modal.set(true);
                                                    loading_detail.set(true);
                                                    spawn(async move {
                                                        match get_cron_trigger(&edit_id).await {
                                                            Ok(detail) => {
                                                                form_name.set(detail.name.clone());
                                                                let shape = shape_of_trigger(&detail);
                                                                form_shape.set(shape.to_string());
                                                                // 业务类型与时间规则是两个维度：先定形态，再按实际存的字段回填时间规则
                                                                let time_rule = time_rule_of(
                                                                    detail.cron_expression.as_deref(),
                                                                    detail.interval_seconds,
                                                                    detail.run_at,
                                                                );
                                                                form_type.set(time_rule.to_string());
                                                                if let Some(c) = &detail.cron_expression {
                                                                    form_cron.set(c.clone());
                                                                }
                                                                if let Some(i) = detail.interval_seconds {
                                                                    form_interval.set(i.to_string());
                                                                }
                                                                if let Some(r) = detail.run_at {
                                                                    form_run_at_local.set(format_local_datetime(r));
                                                                }
                                                                form_behavior.set(
                                                                    if detail.behavior == TriggerBehavior::AsyncNotify {
                                                                        "notify"
                                                                    } else {
                                                                        "wakeup"
                                                                    }
                                                                    .to_string(),
                                                                );
                                                                form_description.set(detail.description.clone());
                                                                if let Some(agent) = &detail.agent_id {
                                                                    form_agent_id.set(agent.clone());
                                                                }
                                                                // P3 四组只读概览
                                                                detail_snapshot.set(Some(detail.clone()));
                                                                if shape == "system" {
                                                                    let (tpl, agent_id, settle_limit) = parse_payload(&detail.payload);
                                                                    form_template.set(tpl.clone());
                                                                    if tpl == "agent_rest" {
                                                                        form_agent_id.set(agent_id);
                                                                        form_settle_limit.set(settle_limit);
                                                                    } else {
                                                                        form_payload.set(detail.payload.clone());
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                toast.error(format!("加载详情失败: {}", e));
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
                                                    class: "btn hud-btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let pid = id_pause.clone();
                                                        spawn(async move {
                                                            match pause_cron_trigger(&pid).await {
                                                                Ok(_) => {
                                                                    toast.success("已暂停");
                                                                    match list_cron_triggers().await {
                                                                        Ok(list) => triggers.set(list.triggers),
                                                                        Err(e) => toast.error(format!("刷新列表失败: {}", e)),
                                                                    }
                                                                }
                                                                Err(e) => toast.error(format!("暂停失败: {}", e)),
                                                            }
                                                        });
                                                    },
                                                    "暂停"
                                                }
                                            } else {
                                                button {
                                                    class: "btn hud-btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let rid = id_resume.clone();
                                                        spawn(async move {
                                                            match resume_cron_trigger(&rid).await {
                                                                Ok(_) => {
                                                                    toast.success("已恢复");
                                                                    match list_cron_triggers().await {
                                                                        Ok(list) => triggers.set(list.triggers),
                                                                        Err(e) => toast.error(format!("刷新列表失败: {}", e)),
                                                                    }
                                                                }
                                                                Err(e) => toast.error(format!("恢复失败: {}", e)),
                                                            }
                                                        });
                                                    },
                                                    "恢复"
                                                }
                                            }
                                            button {
                                                class: "btn hud-btn btn-error btn-sm",
                                                onclick: move |_| {
                                                    pending_delete_id.set(id_delete.clone());
                                                    show_delete_confirm.set(true);
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
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button {
                    class: "btn hud-btn btn-primary",
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
                    // P3 四组只读概览：Who/What/How 与生效窗口由后端固化，
                    // 表单不提供这些字段的编辑入口（系统项尤其是 readonly 语义）。
                    if let Some(snap) = detail_snapshot() {
                        div { class: "border border-base-300 rounded-lg p-3 mb-4 bg-base-200/40",
                            div { class: "text-xs font-semibold text-base-content/70 mb-2",
                                "调度规则概览（只读 · When / Who / What / How）"
                            }
                            div { class: "grid grid-cols-1 md:grid-cols-2 gap-x-4 gap-y-1 text-xs",
                                div { class: "flex gap-2",
                                    span { class: "text-base-content/50 w-10", "When" }
                                    span { class: "font-mono",
                                        "{schedule_text(snap.trigger_type, snap.cron_expression.as_deref().unwrap_or(\"\"), snap.interval_seconds, snap.run_at)}"
                                    }
                                }
                                div { class: "flex gap-2",
                                    span { class: "text-base-content/50 w-10", "Who" }
                                    span {
                                        "{who_text(snap.creator_type, snap.creator_id.as_deref(), snap.agent_id.as_deref(), snap.owner_user_id.as_deref())}"
                                    }
                                }
                                div { class: "flex gap-2",
                                    span { class: "text-base-content/50 w-10", "What" }
                                    span { class: "font-mono",
                                        "{snap.action.clone().unwrap_or_else(|| \"-\".to_string())}"
                                    }
                                }
                                div { class: "flex gap-2",
                                    span { class: "text-base-content/50 w-10", "How" }
                                    span { "{behavior_text(snap.behavior)}" }
                                }
                                if !snap.description.is_empty() {
                                    div { class: "flex gap-2 md:col-span-2",
                                        span { class: "text-base-content/50 w-10 shrink-0", "说明" }
                                        span { class: "text-base-content/70", "{snap.description}" }
                                    }
                                }
                                if !window_text(snap.start_at, snap.end_at).is_empty() {
                                    div { class: "flex gap-2 md:col-span-2",
                                        span { class: "text-base-content/50 w-10 shrink-0", "窗口" }
                                        span { class: "font-mono text-base-content/70",
                                            "{window_text(snap.start_at, snap.end_at)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "form-label", "名称 *" }
                        input {
                            class: "input input-bordered w-full",
                            value: "{form_name}",
                            placeholder: "例：每日 9 点沉淀 Agent 记忆",
                            oninput: move |e| form_name.set(e.value())
                        }
                    }
                    // 两个维度分开选：业务类型是「日程 / 系统定时任务」，时间规则只决定何时触发
                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                        div { class: "form-control w-full",
                            label { class: "form-label", "类型（业务）" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{form_shape}",
                                onchange: move |e| form_shape.set(e.value()),
                                option { value: "system", "系统定时任务" }
                                option { value: "schedule", "日程" }
                            }
                        }
                        div { class: "form-control w-full",
                            label { class: "form-label", "时间规则" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{form_type}",
                                onchange: move |e| form_type.set(e.value()),
                                option { value: "cron", "Cron 表达式" }
                                option { value: "interval", "固定间隔" }
                                option { value: "once", "一次性（指定时刻）" }
                            }
                        }
                    }
                    div { class: "form-hint mb-2",
                        "系统定时任务 = 同步维护动作；日程 = 归属当前用户、到点提醒或唤醒负责 Agent。时间规则只决定何时触发。"
                    }
                    if form_type() == "cron" {
                        div { class: "form-control w-full",
                            label { class: "form-label", "Cron 表达式 *" }
                            input {
                                class: "form-input font-mono",
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
                                                class: "btn hud-btn btn-ghost btn-sm cron-preset-btn",
                                                onclick: move |_| form_cron.set(expr_clone.clone()),
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if form_type() == "interval" {
                        div { class: "form-control w-full",
                            label { class: "form-label", "间隔秒数 *" }
                            input {
                                class: "input input-bordered w-full",
                                r#type: "number",
                                value: "{form_interval}",
                                placeholder: "300（即 5 分钟）",
                                oninput: move |e| form_interval.set(e.value())
                            }
                        }
                    }
                    if form_type() == "once" {
                        div { class: "form-control w-full",
                            label { class: "form-label", "执行时刻 *" }
                            DateTimeInput {
                                value: form_run_at_local(),
                                on_change: move |v| form_run_at_local.set(v),
                            }
                        }
                    }
                    if form_shape() == "schedule" {
                        div { class: "form-control w-full",
                            label { class: "form-label", "说明（到点回灌的内容，留空用名称）" }
                            textarea {
                                class: "form-textarea",
                                style: "min-height: 70px;",
                                value: "{form_description}",
                                oninput: move |e| form_description.set(e.value())
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                            div { class: "form-control w-full",
                                label { class: "form-label", "行为" }
                                select {
                                    class: "select select-bordered w-full",
                                    value: "{form_behavior}",
                                    onchange: move |e| form_behavior.set(e.value()),
                                    option { value: "wakeup", "唤醒 Agent 执行" }
                                    option { value: "notify", "提醒归属用户" }
                                }
                            }
                            div { class: "form-control w-full",
                                label { class: "form-label", "负责 Agent" }
                                SearchableSelect {
                                    placeholder: "选择负责 Agent（可搜索）...".to_string(),
                                    selected: None,
                                    options: agent_options(),
                                    on_select: move |selection: String| handle_agent_select(selection),
                                    on_search: Some(EventHandler::new(move |kw: String| handle_agent_search(kw))),
                                    loading: agent_loading(),
                                }
                                div { class: "form-hint",
                                    if form_agent_id().is_empty() {
                                        "唤醒执行必须指定；仅提醒用户时可留空"
                                    } else {
                                        "已选 Agent ID：{form_agent_id()}"
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "form-control w-full",
                            label { class: "form-label", "Action 模板" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{form_template}",
                                onchange: move |e| {
                                    form_template.set(e.value());
                                    json_error.set(String::new());
                                },
                                for tpl in SYSTEM_ACTION_TEMPLATES {
                                    option { value: tpl.action, "{tpl.label}" }
                                }
                                option { value: "custom", "自定义 JSON（高级）" }
                            }
                        }
                        if let Some(tpl) = active_template {
                            div { class: "form-hint", "{tpl.description}" }
                            if tpl.needs_agent {
                                div { class: "form-control w-full",
                                    label { class: "form-label", "执行 Agent（可留空）" }
                                    SearchableSelect {
                                        placeholder: "选择 Agent（可搜索，留空 = 全部）...".to_string(),
                                        selected: None,
                                        options: agent_options(),
                                        on_select: move |selection: String| handle_agent_select(selection),
                                        on_search: Some(EventHandler::new(move |kw: String| handle_agent_search(kw))),
                                        loading: agent_loading(),
                                    }
                                    div { class: "form-hint",
                                        if form_agent_id().is_empty() {
                                            "留空则扫描全部待沉淀 Agent（系统默认形态）"
                                        } else {
                                            "已选 Agent ID：{form_agent_id()}"
                                        }
                                    }
                                }
                                div { class: "form-control w-full",
                                    label { class: "form-label", "settle_limit（沉淀上限）" }
                                    input {
                                        class: "input input-bordered w-full",
                                        r#type: "number",
                                        value: "{form_settle_limit}",
                                        placeholder: "10",
                                        oninput: move |e| form_settle_limit.set(e.value())
                                    }
                                    div { class: "form-hint", "单次沉淀的短期记忆条数上限，默认 10" }
                                }
                            }
                            div { class: "form-hint", "Payload 由模板自动生成，无需手填 JSON" }
                        } else {
                            div { class: "form-control w-full",
                                label { class: "form-label", "Payload JSON *" }
                                textarea {
                                    class: "form-textarea font-mono",
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

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除".to_string(),
            message: "确定删除此触发器？此操作不可撤销。".to_string(),
            on_confirm: move |_| {
                let id = pending_delete_id();
                show_delete_confirm.set(false);
                spawn(async move {
                    match delete_cron_trigger(&id).await {
                        Ok(_) => {
                            toast.success("删除成功");
                            match list_cron_triggers().await {
                                Ok(list) => triggers.set(list.triggers),
                                Err(e) => toast.error(format!("刷新列表失败: {}", e)),
                            }
                        }
                        Err(e) => toast.error(format!("删除失败: {}", e)),
                    }
                });
            },
            on_cancel: move |_| {
                show_delete_confirm.set(false);
            }
        }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_rule_of_uses_single_rule_not_shape() {
        assert_eq!(time_rule_of(Some("0 9 * * *"), Some(300), Some(1)), "cron");
        assert_eq!(time_rule_of(Some("  "), Some(300), Some(1)), "interval");
        assert_eq!(time_rule_of(None, None, Some(1)), "once");
    }

    #[test]
    fn agent_id_from_option_parses_label_and_plain_id() {
        assert_eq!(agent_id_from_option("研发 Agent (agent-42)"), "agent-42");
        assert_eq!(agent_id_from_option("agent-7"), "agent-7");
    }

    #[test]
    fn system_templates_cover_all_backend_actions() {
        let actions: Vec<&str> = SYSTEM_ACTION_TEMPLATES.iter().map(|t| t.action).collect();
        assert_eq!(
            actions,
            vec![
                "agent_rest",
                "project_followup",
                "tool_log_cleanup",
                "directory_reconcile",
                "pending_notification_redelivery",
                "vector_maintenance",
            ]
        );
        assert!(selected_template("agent_rest").is_some_and(|t| t.needs_agent));
        assert!(selected_template("vector_maintenance").is_some_and(|t| !t.needs_agent));
        assert!(selected_template("not_a_real_action").is_none());
    }

    #[test]
    fn build_payload_of_maintenance_template_needs_no_agent() {
        let payload = build_payload("vector_maintenance", "", "").unwrap();
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(value["action"], "vector_maintenance");
        assert_eq!(value["extra"], serde_json::json!({}));
    }

    #[test]
    fn build_payload_agent_rest_allows_empty_agent_for_global_settle() {
        let payload = build_payload("agent_rest", "", "").unwrap();
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(value["action"], "agent_rest");
        assert_eq!(value["extra"]["settle_limit"], 10);
        assert!(value["extra"].get("agent_id").is_none());

        let scoped = build_payload("agent_rest", "agent-7", "5").unwrap();
        let value: serde_json::Value = serde_json::from_str(&scoped).unwrap();
        assert_eq!(value["extra"]["agent_id"], "agent-7");
        assert_eq!(value["extra"]["settle_limit"], 5);
    }

    #[test]
    fn parse_payload_roundtrips_all_templates_and_falls_back_to_custom() {
        for tpl in SYSTEM_ACTION_TEMPLATES {
            let payload = build_payload(tpl.action, "", "").unwrap();
            let (parsed, _, _) = parse_payload(&payload);
            assert_eq!(parsed, tpl.action);
        }
        assert_eq!(
            parse_payload(r#"{"action":"other","extra":{}}"#).0,
            "custom"
        );
    }
}
