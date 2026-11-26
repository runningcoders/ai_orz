//! 定时触发器管理 - 列表 + 新建（编辑在详情页 trigger_detail.rs）

use crate::components::hud::{HudPanel, HudSection};
use dioxus::prelude::*;

use super::trigger_form_fields::{
    RULE_CRON, RULE_INTERVAL, RULE_ONCE, TriggerBasicFields, parse_local_datetime_ms,
};
use crate::api::hr::{list_agents, search_agents};
use crate::api::system::{
    create_cron_trigger, create_schedule, delete_cron_trigger, list_cron_triggers,
    pause_cron_trigger, query_schedule, resume_cron_trigger,
};
use crate::components::SearchableSelect;
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    CreateCronTriggerRequest, ListAgentsRequest, ListCronTriggersRequest,
    ListCronTriggersResponseItem, QueryScheduleRequest, QueryScheduleResponse,
    SYSTEM_ACTION_TEMPLATES, ScheduleAgentTaskRequest, ScheduleOccurrence, SearchAgentsRequest,
    SystemActionTemplate, system_action_template,
};
use common::enums::{CreatorType, ScheduleDateTerm, TriggerBehavior, TriggerStatus, TriggerType};
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
/// 编辑入口已迁至详情页，本函数仅测试保留（验证系统模板 payload 往返）。
#[cfg(test)]
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

/// How: 行为标签（文案统一走 common 的 `TriggerBehavior::label`，避免双端各写一份）
fn behavior_text(b: TriggerBehavior) -> &'static str {
    b.label()
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

/// 时间规则三选一（cron > interval > run_at）→ 表单单选值
/// 编辑入口已迁至详情页，本函数仅测试保留。
#[cfg(test)]
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

/// 列表状态筛选：默认只显示启用（运行中），取消/暂停项需显式切换才可见
#[derive(Debug, Clone, Copy, PartialEq)]
enum StatusFilter {
    Enabled,
    Disabled,
    All,
}

/// 管理端每页条数（与后端默认/上限口径一致）
const TRIGGER_PAGE_SIZE: usize = 50;

/// UI 状态选择 → 后端状态列表：默认只取启用，避免拉全量再本地过滤。
fn status_values(filter: StatusFilter) -> Vec<TriggerStatus> {
    match filter {
        StatusFilter::Enabled => vec![TriggerStatus::Enabled],
        StatusFilter::Disabled => vec![TriggerStatus::Disabled],
        StatusFilter::All => vec![TriggerStatus::Enabled, TriggerStatus::Disabled],
    }
}

/// 形态 + 行为两个下拉合成后端行为列表（取交集），过滤留在数据端。
fn behavior_values(shape: ShapeFilter, behavior: BehaviorFilter) -> Vec<TriggerBehavior> {
    let allowed = match shape {
        ShapeFilter::All => vec![
            TriggerBehavior::SyncMaintenance,
            TriggerBehavior::AsyncNotify,
            TriggerBehavior::AsyncAgentWakeup,
        ],
        ShapeFilter::System => vec![TriggerBehavior::SyncMaintenance],
        ShapeFilter::Schedule => vec![
            TriggerBehavior::AsyncNotify,
            TriggerBehavior::AsyncAgentWakeup,
        ],
    };
    let picked = match behavior {
        BehaviorFilter::All => None,
        BehaviorFilter::Sync => Some(TriggerBehavior::SyncMaintenance),
        BehaviorFilter::Notify => Some(TriggerBehavior::AsyncNotify),
        BehaviorFilter::Wakeup => Some(TriggerBehavior::AsyncAgentWakeup),
    };
    allowed
        .into_iter()
        .filter(|item| picked.is_none_or(|p| p == *item))
        .collect()
}

fn list_request(
    status: StatusFilter,
    shape: ShapeFilter,
    behavior: BehaviorFilter,
    page: usize,
) -> ListCronTriggersRequest {
    ListCronTriggersRequest {
        statuses: Some(status_values(status)),
        behaviors: Some(behavior_values(shape, behavior)),
        offset: Some(page * TRIGGER_PAGE_SIZE),
        limit: Some(TRIGGER_PAGE_SIZE),
        ..Default::default()
    }
}

/// 统一写回分页结果：总数用于分页条，越界页回退后由 effect 自动重拉。
fn apply_list(
    list: common::api::ListCronTriggersResponse,
    mut page: Signal<usize>,
    mut triggers: Signal<Vec<ListCronTriggersResponseItem>>,
    mut total: Signal<usize>,
) {
    let pages = list.total.div_ceil(TRIGGER_PAGE_SIZE).max(1);
    if page() >= pages {
        page.set(pages - 1);
    }
    total.set(list.total);
    triggers.set(list.triggers);
}

#[component]
pub fn SystemTriggers() -> Element {
    let triggers = use_signal(Vec::<ListCronTriggersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut show_modal = use_signal(|| false);

    // P3 列表筛选：形态（系统/日程）+ 行为模板 + 状态（默认只看启用）
    let mut shape_filter = use_signal(|| ShapeFilter::All);
    let mut behavior_filter = use_signal(|| BehaviorFilter::All);
    let mut status_filter = use_signal(|| StatusFilter::Enabled);
    // 列表分页：过滤/分页全下推数据端，本地只持有当前页与过滤后的总数
    let trigger_total = use_signal(|| 0usize);
    let mut page = use_signal(|| 0usize);

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

    let toast = use_toast();

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // 列表查询入口：过滤条件/分页变更自动重查。
    // ⚠️ 信号必须在 effect 同步段读取——Dioxus 只追踪闭包同步执行期间的读取，
    // 放进 spawn(async{...}) 里读会脱离响应式上下文，过滤/分页变化就不会触发重查。
    use_effect(move || {
        let req = list_request(status_filter(), shape_filter(), behavior_filter(), page());
        loading.set(true);
        spawn(async move {
            match list_cron_triggers(&req).await {
                Ok(list) => apply_list(list, page, triggers, trigger_total),
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
        let cron_expression = if time_rule == RULE_CRON {
            let c = form_cron().trim().to_string();
            if c.is_empty() {
                toast.error("Cron 表达式不能为空");
                return;
            }
            Some(c)
        } else {
            None
        };
        let interval_seconds = if time_rule == RULE_INTERVAL {
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
        let run_at = if time_rule == RULE_ONCE {
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
            RULE_ONCE => TriggerType::Once,
            RULE_INTERVAL => TriggerType::Interval,
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
        spawn(async move {
            // 新建统一走两个业务入口：日程走 /schedules（带身份与异步行为），
            // 系统定时任务走 CRUD。编辑不在此处 —— 列表「详情」进详情页编辑。
            let result = if shape == "schedule" {
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
            } else {
                let req = CreateCronTriggerRequest {
                    name: name.clone(),
                    trigger_type,
                    cron_expression,
                    interval_seconds,
                    run_at,
                    payload: payload_str,
                };
                create_cron_trigger(req).await.map(|_| ())
            };
            match result {
                Ok(_) => {
                    let msg = if shape == "schedule" {
                        "创建日程成功"
                    } else {
                        "创建系统定时任务成功"
                    };
                    toast.success(msg);
                    show_modal.set(false);
                    match list_cron_triggers(&list_request(
                        status_filter(),
                        shape_filter(),
                        behavior_filter(),
                        page(),
                    ))
                    .await
                    {
                        Ok(list) => apply_list(list, page, triggers, trigger_total),
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
    // 状态 / 形态 / 行为过滤与分页都由后端完成，这里只做展示排序。
    let mut filtered_list: Vec<_> = triggers_list.clone();
    filtered_list.sort_by_key(|t| (t.next_run_at <= 0, t.next_run_at));
    let total_pages = trigger_total().div_ceil(TRIGGER_PAGE_SIZE).max(1);
    let current_page = page().min(total_pages - 1);
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
    let modal_title = if form_shape() == "schedule" {
        "新建日程".to_string()
    } else {
        "新建系统定时任务".to_string()
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
                        class: "btn hud-btn btn-outline",
                        onclick: move |_| {
                            let req = list_request(status_filter(), shape_filter(), behavior_filter(), page());
                            loading.set(true);
                            spawn(async move {
                                match list_cron_triggers(&req).await {
                                    Ok(list) => apply_list(list, page, triggers, trigger_total),
                                    Err(e) => toast.error(format!("查询失败: {}", e)),
                                }
                                loading.set(false);
                            });
                        },
                        "🔍 查询"
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
                            json_error.set(String::new());
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
                            page.set(0);
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
                            page.set(0);
                        },
                        option { value: "all", "全部行为" }
                        option { value: "sync", "{TriggerBehavior::SyncMaintenance.label()}" }
                        option { value: "notify", "{TriggerBehavior::AsyncNotify.label()}" }
                        option { value: "wakeup", "{TriggerBehavior::AsyncAgentWakeup.label()}" }
                    }
                    span { class: "text-xs text-base-content/60 ml-2", "状态" }
                    select {
                        class: "select select-bordered select-sm",
                        onchange: move |e| {
                            status_filter.set(match e.value().as_str() {
                                "disabled" => StatusFilter::Disabled,
                                "all" => StatusFilter::All,
                                _ => StatusFilter::Enabled,
                            });
                            page.set(0);
                        },
                        option { value: "enabled", selected: status_filter() == StatusFilter::Enabled, "启用" }
                        option { value: "disabled", selected: status_filter() == StatusFilter::Disabled, "取消 / 暂停" }
                        option { value: "all", selected: status_filter() == StatusFilter::All, "全部状态" }
                    }
                } else {
                    span { class: "text-xs text-base-content/60",
                        "日历只展开你自己的日程（不含系统触发器）"
                    }
                }
                span { class: "text-xs text-base-content/50 ml-auto",
                    "本页 {filtered_list.len()} 条 / 共 {trigger_total()} 条"
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
                div { class: "overflow-x-auto",
                table { class: "table hud-table hud-trigger-table table-zebra",
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
                                            div { class: "hud-trigger-name", title: "{name}", "{name}" }
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
                                            span { class: "hud-trigger-who text-xs text-base-content/70", title: "{who}", "{who}" }
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
                                            div { class: "hud-trigger-actions flex flex-nowrap justify-end gap-1",
                                            Link {
                                                class: "btn hud-btn btn-ghost btn-sm",
                                                to: crate::pages::Route::SystemTriggerDetail { id: id_edit.clone() },
                                                "详情"
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
                                                                    match list_cron_triggers(&list_request(status_filter(), shape_filter(), behavior_filter(), page())).await {
                                                                        Ok(list) => apply_list(list, page, triggers, trigger_total),
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
                                                                    match list_cron_triggers(&list_request(status_filter(), shape_filter(), behavior_filter(), page())).await {
                                                                        Ok(list) => apply_list(list, page, triggers, trigger_total),
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
            }
            if calendar_view() == CalendarView::List && total_pages > 1 {
                div { class: "flex justify-center mt-4",
                    div { class: "join",
                        button {
                            class: "join-item btn btn-sm",
                            disabled: current_page == 0,
                            onclick: move |_| page.set(current_page.saturating_sub(1)),
                            "«"
                        }
                        button { class: "join-item btn btn-sm", "第 {current_page + 1} 页 / {total_pages}" }
                        button {
                            class: "join-item btn btn-sm",
                            disabled: current_page + 1 >= total_pages,
                            onclick: move |_| page.set(current_page + 1),
                            "»"
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
                    disabled: submitting(),
                    onclick: handle_submit,
                    if submitting() { "保存中..." } else { "保存" }
                }
            },
            div {
                TriggerBasicFields {
                    name: form_name,
                    rule: form_type,
                    cron: form_cron,
                    interval: form_interval,
                    run_at_local: form_run_at_local,
                    shape: Some(form_shape),
                    allow_once: true,
                    hint: Some("系统定时任务 = 同步维护动作；日程 = 归属当前用户、到点提醒或唤醒负责 Agent。时间规则只决定何时触发。".to_string()),
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
                            match list_cron_triggers(&list_request(status_filter(), shape_filter(), behavior_filter(), page())).await {
                                Ok(list) => apply_list(list, page, triggers, trigger_total),
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
