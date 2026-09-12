//! @ 提及选择器：通用多级菜单（类型 Tab 收窄 + 关键词搜索结果）
//!
//! ## 设计要点
//!
//! - **纯展示组件**：`MentionPicker` 只负责渲染，数据加载与文本改动全部由
//!   [`MentionState`] 承担，调用方只需把 state 传进来即可复用整套交互。
//! - **状态外置**：键盘事件发生在宿主的 textarea 上，无法被浮层组件捕获，
//!   所以菜单的开关 / 高亮 / 候选必须由调用方与组件共享的 state 驱动。
//! - **不碰光标**：输入框始终是受控 `textarea`，插入的是纯文本语法
//!   （见 [`crate::utils::mention`]），不使用 contenteditable，
//!   避免中文输入法组合期间被重渲染打断（历史教训 `0644609c`）。
//! - **菜单生命周期有兜底**：`@` 不做任何「前缀条件」（`hi@张` 这类不补空格的
//!   写法必须能用，见 `common::mention::detect_mention_query`）；代价是打邮箱时
//!   也会闪一下 —— 由**关键词搜不到结果即自动收起** + 同一 `@` 位置的「死前缀」
//!   抑制来兜底，用户不选就自然消失，无需按 ESC（ESC / Enter / 发送也可主动关）。
//!
//! ## 用法
//!
//! ```ignore
//! let mention = MentionState::new(project_id, vec![MentionKind::Agent, MentionKind::Task]);
//! // oninput: mention.sync(&value, caret)
//! // onkeydown: mention.move_selection(±1) / mention.confirm(&input_text())
//! rsx! {
//!     MentionPicker {
//!         state: mention,
//!         tabs: mention_tabs(&[MentionKind::Agent, MentionKind::Task]),
//!         on_pick: on_pick_mention,
//!     }
//! }
//! ```

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::api::hr::query_agents;
use crate::api::organization::list_federation_agents;
use crate::api::project::{list_project_tasks, search_projects, search_tasks};
use crate::utils::mention::{
    MentionKind, MentionQuery, MentionRef, apply_mention_pick, detect_mention_query,
    format_mention_ref, remove_mention_token,
};
use common::api::{
    AgentListItem, AgentQueryRequest, PaginationParams, SearchProjectsRequest, SearchTasksRequest,
};

/// 候选拉取上限（既是单类型上限，也是菜单展示上限）
const CANDIDATE_LIMIT: usize = 20;

/// 一个可选的提及目标
#[derive(Debug, Clone, PartialEq)]
pub struct MentionCandidate {
    /// 实体类型
    pub kind: MentionKind,
    /// 实体 ID
    pub id: String,
    /// 跨组织限定（联邦 Agent）：`agent:<id>@<org_id>` 的 org 段
    pub org: Option<String>,
    /// 展示名
    pub name: String,
    /// 副标题（Agent 角色 / 任务进度 / 对端组织名等补充信息，可为空）
    pub subtitle: String,
}

impl MentionCandidate {
    /// 写入消息正文的提及语法（name 仅作为展示快照，渲染时优先用实时名）
    pub fn token(&self) -> String {
        format_mention_ref(
            &MentionRef {
                kind: self.kind,
                id: self.id.clone(),
                org: self.org.clone(),
            },
            &self.name,
        )
    }

    /// 去重键（`type:id` 或联邦 `type:id@org`）
    pub fn key(&self) -> String {
        match &self.org {
            Some(org) => format!("{}:{}@{}", self.kind.as_str(), self.id, org),
            None => format!("{}:{}", self.kind.as_str(), self.id),
        }
    }
}

/// 类型 Tab
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MentionTab {
    /// 全部（不按类型收窄）
    #[default]
    All,
    /// 组织内 Agent
    Agent,
    /// 任务
    Task,
    /// 项目
    Project,
}

impl MentionTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "全部",
            Self::Agent => "Agent",
            Self::Task => "任务",
            Self::Project => "项目",
        }
    }

    /// 该 Tab 下是否展示某类型的候选
    pub fn matches(self, kind: MentionKind) -> bool {
        match self {
            Self::All => true,
            Self::Agent => kind == MentionKind::Agent,
            Self::Task => kind == MentionKind::Task,
            Self::Project => kind == MentionKind::Project,
        }
    }
}

/// 当前会话允许 @ 的类型
///
/// - 项目会话：Agent + 任务（已在项目内，不再允许 @ 项目本身）
/// - 默认对话：Agent + 任务 + 项目（@ 仅作上下文提示，Agent 取组织全量由关键词收窄）
///
/// 单一事实源：`MentionState` 拉候选与 Tab 渲染都走这里，避免两处口径漂移。
pub fn mention_kinds_for(project_id: Option<&str>) -> Vec<MentionKind> {
    if project_id.is_some() {
        // 项目会话：@ Agent 限于项目协作人，且不再允许 @ 项目本身
        vec![MentionKind::Agent, MentionKind::Task]
    } else {
        // 默认对话：@ 仅作上下文提示，三种类型都可；Agent 取组织全量
        vec![MentionKind::Agent, MentionKind::Task, MentionKind::Project]
    }
}

/// 按可用类型生成 Tab 列表（类型多于一个时才需要「全部」）
pub fn mention_tabs(kinds: &[MentionKind]) -> Vec<MentionTab> {
    let mut tabs = Vec::with_capacity(kinds.len() + 1);
    if kinds.len() > 1 {
        tabs.push(MentionTab::All);
    }
    for kind in kinds {
        match kind {
            MentionKind::Agent => tabs.push(MentionTab::Agent),
            MentionKind::Task => tabs.push(MentionTab::Task),
            MentionKind::Project => tabs.push(MentionTab::Project),
        }
    }
    tabs
}

/// 菜单状态：候选加载 + 键盘导航 + 文本插入
///
/// 所有字段都是 `Signal`，因此可在组件间按值（Copy）传递。
/// 必须在组件函数体内通过 [`MentionState::new`] 创建（内部调 `use_signal` / `use_effect`）。
#[derive(Clone, Copy, PartialEq)]
pub struct MentionState {
    /// 当前激活的 @ 查询（`None` 表示菜单关闭）
    menu: Signal<Option<MentionQuery>>,
    /// 已加载的候选（未过 Tab）
    candidates: Signal<Vec<MentionCandidate>>,
    /// 正在拉取候选
    loading: Signal<bool>,
    /// 当前类型 Tab
    tab: Signal<MentionTab>,
    /// 当前高亮项下标（相对「过滤后」的列表）
    index: Signal<usize>,
    /// 本次输入已插入的提及（供「已提及」胶囊条展示与删除）
    picked: Signal<Vec<MentionCandidate>>,
    /// 候选加载请求序号（用于丢弃过期响应，避免慢响应覆盖新结果）
    req: Signal<u64>,
    /// 已判定「搜不到结果」的查询：`(@ 的字节下标, 当时的关键词)`
    ///
    /// 关键词（非空）无命中说明用户多半不是在找实体（在打邮箱、随手输了一串字），
    /// 此时自动收掉菜单。记下这个「死前缀」是为了**吸收后续输入**：在同一个 `@`
    /// 位置继续把它加长不会再把菜单弹回来，否则每次按键都是
    /// 「弹出 → 请求 → 无结果 → 关闭」，比不收还烦。
    ///
    /// 解除条件：退格或换词（不再是前缀扩展）、换一个 `@` 位置，或用户主动
    /// 收起菜单（ESC / Enter / 发送，见 [`MentionState::close`]）。
    dead_prefix: Signal<Option<(usize, String)>>,
}

impl MentionState {
    /// 创建状态并启动候选加载 effect
    ///
    /// `project_id` 传的是 **Signal 而非值**：effect 同步读取它，切换会话后
    /// 后续候选拉取自动跟随新项目（按值捕获会让项目切换后仍查旧项目）。
    pub fn new(project_id: Signal<Option<String>>) -> Self {
        let mut state = Self {
            menu: use_signal(|| None::<MentionQuery>),
            candidates: use_signal(Vec::<MentionCandidate>::new),
            loading: use_signal(|| false),
            tab: use_signal(MentionTab::default),
            index: use_signal(|| 0usize),
            picked: use_signal(Vec::<MentionCandidate>::new),
            req: use_signal(|| 0u64),
            dead_prefix: use_signal(|| None::<(usize, String)>),
        };

        // 只同步读取 menu / project_id → 仅当「查询词变化 / 菜单开关 / 切换会话」时触发拉取，
        // 不会因为父组件的消息列表刷新而重跑（避免打断输入）
        use_effect(move || {
            // 同时取 start：空结果时要回填「死前缀」（标注是哪个 @ 位置的无命中词）
            let Some((start, query)) = state
                .menu
                .read()
                .as_ref()
                .map(|q| (q.start, q.query.clone()))
            else {
                return;
            };
            let pid = project_id();
            let kinds = mention_kinds_for(pid.as_deref());
            let seq = {
                let mut req = state.req.write();
                *req += 1;
                *req
            };
            state.loading.set(true);
            spawn(async move {
                let list = load_candidates(pid, &kinds, &query).await;
                // 丢弃过期响应：只有最新一次请求能写入结果
                if *state.req.read() == seq {
                    let empty = list.is_empty();
                    state.candidates.set(list);
                    state.loading.set(false);
                    // 关键词（非空）搜不到任何结果 → 用户大概率不是在找实体，收掉菜单。
                    // ⚠️ 先写 dead_prefix 再收：不能走 close()，那会把这个抑制标记清掉。
                    if empty && !query.is_empty() {
                        *state.dead_prefix.write() = Some((start, query));
                        state.hide();
                    }
                }
            });
        });

        state
    }

    /// 菜单是否打开
    pub fn is_open(self) -> bool {
        self.menu.read().is_some()
    }

    /// 输入框内容或光标变化后重新判定（由 `oninput` 调用）
    ///
    /// 若光标处正是**已被判定搜不到结果的 `@` 位置**，且关键词仍是当时那段的前缀
    /// 扩展（用户还在继续往下打），则保持关闭、不再弹菜单 —— 否则每次按键都会
    /// 「弹出 → 请求 → 无结果 → 关闭」，比不收还烦。退格、换词、换 `@` 位置
    /// 都会自动解除抑制（见 [`MentionState::dead_prefix`]）。
    pub fn sync(mut self, text: &str, caret: usize) {
        let next = detect_mention_query(text, caret);
        if let Some(q) = &next {
            let suppressed = self
                .dead_prefix
                .read()
                .as_ref()
                .is_some_and(|(start, prefix)| {
                    *start == q.start && q.query.starts_with(prefix.as_str())
                });
            if suppressed {
                // 抑制生效：确保菜单收起（不能调 close()，那会清掉抑制标记）
                self.hide();
                return;
            }
            let has_dead = self.dead_prefix.read().is_some();
            if has_dead {
                // 换了词 / 换了 @ 位置 → 抑制解除，照常走打开流程
                self.dead_prefix.set(None);
            }
        }
        let changed = match (self.menu.read().as_ref(), next.as_ref()) {
            (Some(a), Some(b)) => a.query != b.query,
            (None, None) => false,
            _ => true,
        };
        if !changed {
            return;
        }
        if next.is_some() {
            // 新查询：清空旧候选避免闪旧结果，并把高亮复位
            self.index.set(0);
            self.candidates.set(Vec::new());
        }
        self.menu.set(next);
    }

    /// 只收起菜单浮层，不碰「死前缀」标记（内部用）
    fn hide(mut self) {
        if self.menu.read().is_some() {
            self.menu.set(None);
            self.index.set(0);
        }
    }

    /// 关闭菜单（用户主动：ESC / Enter 收起 / 发送完成）
    ///
    /// 顺带清掉「死前缀」抑制：主动关掉之后再继续输入，应该重新有弹菜单的机会。
    pub fn close(mut self) {
        self.hide();
        let has_dead = self.dead_prefix.read().is_some();
        if has_dead {
            self.dead_prefix.set(None);
        }
    }

    /// 切换类型 Tab
    pub fn set_tab(mut self, tab: MentionTab) {
        self.tab.set(tab);
        self.index.set(0);
    }

    /// 鼠标悬停高亮
    pub fn hover(mut self, index: usize) {
        self.index.set(index);
    }

    /// 当前 Tab 下可见的候选
    pub fn visible(self) -> Vec<MentionCandidate> {
        let tab = *self.tab.read();
        self.candidates
            .read()
            .iter()
            .filter(|c| tab.matches(c.kind))
            .cloned()
            .collect()
    }

    /// 当前高亮下标
    pub fn index(self) -> usize {
        *self.index.read()
    }

    /// 是否正在加载候选
    pub fn is_loading(self) -> bool {
        *self.loading.read()
    }

    /// 当前类型 Tab
    pub fn tab(self) -> MentionTab {
        *self.tab.read()
    }

    /// 键盘移动高亮；返回是否应吞掉该按键（菜单开着就吞，无论有无候选）
    pub fn move_selection(mut self, delta: i32) -> bool {
        if !self.is_open() {
            return false;
        }
        let len = self.visible().len();
        if len == 0 {
            return true;
        }
        let cur = self.index() as i32;
        let next = if delta > 0 {
            (cur + 1) % len as i32
        } else {
            (cur - 1 + len as i32) % len as i32
        };
        self.index.set(next as usize);
        true
    }

    /// 确认当前高亮项：返回 `(新文本, 新光标)`；菜单未开或无候选返回 `None`
    ///
    /// 鼠标点选路径由 `MentionPicker` 先 `hover` 到对应下标再回调，
    /// 因此键鼠两条路径共用这一个入口，不会出现「点了 A 插入 B」。
    pub fn confirm(mut self, text: &str) -> Option<(String, usize)> {
        let menu = self.menu.read().clone()?;
        let list = self.visible();
        let item = list
            .get(self.index().min(list.len().saturating_sub(1)))?
            .clone();
        let (new_text, caret) = apply_mention_pick(text, &menu, &item.token());
        self.picked.with_mut(|v| {
            let key = item.key();
            if !v.iter().any(|c| c.key() == key) {
                v.push(item);
            }
        });
        self.close();
        Some((new_text, caret))
    }

    /// 已插入的提及列表（供胶囊条展示）
    pub fn picked(self) -> Vec<MentionCandidate> {
        self.picked.read().clone()
    }

    /// 摘掉一个已插入的提及：同步移除胶囊并返回新文本
    pub fn remove_picked(mut self, text: &str, key: &str) -> String {
        let token = self
            .picked
            .read()
            .iter()
            .find(|c| c.key() == key)
            .map(|c| c.token());
        self.picked.with_mut(|v| v.retain(|c| c.key() != key));
        match token {
            Some(t) => remove_mention_token(text, &t),
            None => text.to_string(),
        }
    }

    /// 清空已插入记录（发送成功后调用，避免污染下一次输入）
    pub fn reset_picked(mut self) {
        self.picked.set(Vec::new());
    }

    /// 预置一个提及（视图锚定用，如「进入任务视图默认 @ 该任务」）
    ///
    /// 与 [`confirm`](Self::confirm) 的区别：不依赖菜单状态、不做区间替换，
    /// 只把候选登记进「已提及」列表，并返回可追加到正文的提及语法串。
    /// 调用方负责把它拼进输入框文本 —— 因此**不覆盖用户已输入的内容**是调用方的责任。
    pub fn preset(mut self, item: MentionCandidate) -> String {
        let token = item.token();
        let key = item.key();
        self.picked.with_mut(|v| {
            if !v.iter().any(|c| c.key() == key) {
                v.push(item.clone());
            }
        });
        token
    }
}

/// 按类型 + 关键词拉取候选
///
/// 单类型失败不影响其他类型（部分可用优于整体为空）。
async fn load_candidates(
    project_id: Option<String>,
    kinds: &[MentionKind],
    keyword: &str,
) -> Vec<MentionCandidate> {
    let kw = keyword.trim();
    let kw = if kw.is_empty() { None } else { Some(kw) };
    let mut out = Vec::new();
    for kind in kinds {
        match kind {
            // Agent 默认对话取组织全量（@ 仅作上下文提示）；项目会话限于协作人。
            // 联邦 Agent（跨组织）两种会话都追加：委派不受项目边界限制。
            MentionKind::Agent => {
                if let Some(pid) = project_id.as_deref() {
                    out.extend(load_project_agents(pid, kw).await);
                } else {
                    out.extend(load_org_agents(kw).await);
                }
                out.extend(load_federation_agents(kw).await);
            }
            MentionKind::Task => {
                out.extend(load_tasks(project_id.as_deref(), kw).await);
            }
            MentionKind::Project => out.extend(load_projects(kw).await),
        }
    }
    out
}

/// AgentListItem → 提及候选（Agent 类型）
fn agent_to_candidate(a: AgentListItem) -> MentionCandidate {
    let subtitle = a.roles.first().cloned().unwrap_or_default();
    MentionCandidate {
        kind: MentionKind::Agent,
        id: a.id,
        org: None,
        name: a.name,
        subtitle,
    }
}

/// 联邦 Agent 候选：聚合各 Active 对端开放的 Agent（P5）
///
/// 响应无搜索参数，全量拉回后本地按关键词过滤；单个对端失败已在服务端跳过。
async fn load_federation_agents(keyword: Option<&str>) -> Vec<MentionCandidate> {
    let Ok(resp) = list_federation_agents().await else {
        return Vec::new();
    };
    let kw = keyword.map(|s| s.to_lowercase());
    resp.groups
        .into_iter()
        .flat_map(|g| {
            g.agents.into_iter().map(move |a| MentionCandidate {
                kind: MentionKind::Agent,
                id: a.id,
                org: Some(g.org_id.clone()),
                name: a.name.clone(),
                subtitle: format!("联邦 · {}", g.org_name),
            })
        })
        .filter(|c| match &kw {
            Some(k) => c.name.to_lowercase().contains(k) || c.subtitle.to_lowercase().contains(k),
            None => true,
        })
        .take(CANDIDATE_LIMIT)
        .collect()
}

/// 项目内可 @ 的 Agent = 项目下任务的 assignee（去重）
///
/// 项目没有成员表，任务 assignee 是「谁真正在这个项目干活」的唯一事实源，
/// 与 `pages/project/project_detail.rs` 推导协作 Agent 的口径保持一致。
async fn load_project_agents(project_id: &str, keyword: Option<&str>) -> Vec<MentionCandidate> {
    let mut seen = HashSet::new();
    let mut ids: Vec<String> = Vec::new();
    if let Ok(resp) = list_project_tasks(project_id).await {
        for t in resp.tasks {
            if t.assignee_type == 1 && seen.insert(t.assignee_id.clone()) {
                ids.push(t.assignee_id);
            }
        }
    }
    if ids.is_empty() {
        return Vec::new();
    }
    let req = AgentQueryRequest {
        ids: Some(ids),
        keyword: keyword.map(|s| s.to_string()),
        pagination: PaginationParams {
            limit: Some(CANDIDATE_LIMIT),
            offset: None,
        },
        ..Default::default()
    };
    match query_agents(&req).await {
        Ok(page) => page.items.into_iter().map(agent_to_candidate).collect(),
        Err(_) => Vec::new(),
    }
}

/// 默认对话下可 @ 的 Agent = 组织全量（不限于某项目的协作人）
///
/// @ 现在只作上下文提示，因此候选范围放开到整个组织，由关键词收窄。
async fn load_org_agents(keyword: Option<&str>) -> Vec<MentionCandidate> {
    let req = AgentQueryRequest {
        keyword: keyword.map(|s| s.to_string()),
        pagination: PaginationParams {
            limit: Some(CANDIDATE_LIMIT),
            offset: None,
        },
        ..Default::default()
    };
    match query_agents(&req).await {
        Ok(page) => page.items.into_iter().map(agent_to_candidate).collect(),
        Err(_) => Vec::new(),
    }
}

async fn load_tasks(project_id: Option<&str>, keyword: Option<&str>) -> Vec<MentionCandidate> {
    let req = SearchTasksRequest {
        keyword: keyword.map(|s| s.to_string()),
        project_id: project_id.map(|s| s.to_string()),
        pagination: PaginationParams {
            limit: Some(CANDIDATE_LIMIT),
            offset: None,
        },
        ..Default::default()
    };
    match search_tasks(&req).await {
        Ok(page) => page
            .items
            .into_iter()
            .map(|t| MentionCandidate {
                kind: MentionKind::Task,
                id: t.id,
                org: None,
                name: t.title,
                subtitle: format!("进度 {}%", t.progress),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

async fn load_projects(keyword: Option<&str>) -> Vec<MentionCandidate> {
    let req = SearchProjectsRequest {
        keyword: keyword.map(|s| s.to_string()),
        pagination: PaginationParams {
            limit: Some(CANDIDATE_LIMIT),
            offset: None,
        },
        ..Default::default()
    };
    match search_projects(&req).await {
        Ok(page) => page
            .items
            .into_iter()
            .map(|p| MentionCandidate {
                kind: MentionKind::Project,
                id: p.id,
                org: None,
                name: p.name,
                subtitle: String::new(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// 已提及胶囊条（输入框上方）
///
/// 输入框是纯文本 textarea，无法内嵌 chip，因此在上方用胶囊条给出可视化确认，
/// 并支持点击摘除（同步把正文里的语法串删掉）。
#[component]
pub fn MentionPickedBar(picked: Vec<MentionCandidate>, on_remove: Callback<String>) -> Element {
    if picked.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "flex flex-wrap items-center gap-2 mb-2",
            span { class: "text-xs text-base-content/50", "已提及" }
            for item in picked.into_iter() {
                div {
                    key: "{item.key()}",
                    class: "mention-chip {item.kind.chip_class()} gap-1",
                    span { "@{item.name}" }
                    button {
                        class: "opacity-60 hover:opacity-100",
                        r#type: "button",
                        title: "移除提及",
                        onclick: {
                            let key = item.key();
                            move |_| on_remove.call(key.clone())
                        },
                        "×"
                    }
                }
            }
        }
    }
}

/// @ 提及候选菜单（浮层，通常挂在输入区上方）
///
/// - `tabs`：可见的类型 Tab（由 [`mention_tabs`] 生成）
/// - `on_pick`：选中回调，调用方负责写回文本并恢复光标
#[component]
pub fn MentionPicker(
    state: MentionState,
    tabs: Vec<MentionTab>,
    on_pick: Callback<MentionCandidate>,
) -> Element {
    let current_tab = state.tab();
    let items = state.visible();
    let index = state.index().min(items.len().saturating_sub(1));
    let loading = state.is_loading();

    rsx! {
        div { class: "mention-menu",
            if tabs.len() > 1 {
                div { class: "mention-menu-tabs",
                    for tab in tabs.into_iter() {
                        button {
                            key: "{tab.label()}",
                            class: if tab == current_tab { "mention-menu-tab is-active" } else { "mention-menu-tab" },
                            r#type: "button",
                            // 用 mousedown + prevent_default：避免点击时按钮抢走 textarea 焦点，
                            // 否则焦点落到 tab 上后按 ESC 会触发 :focus-visible 默认白边，
                            // 且 textarea 的 ESC 关闭逻辑不再生效（与候选项 onmousedown 处理一致）
                            onmousedown: move |e| {
                                e.prevent_default();
                                state.set_tab(tab);
                            },
                            "{tab.label()}"
                        }
                    }
                }
            }
            div { class: "mention-menu-body",
                if items.is_empty() {
                    div { class: "px-3 py-4 text-sm text-base-content/50 text-center",
                        if loading { "搜索中..." } else { "无匹配结果" }
                    }
                } else {
                    for (i, item) in items.into_iter().enumerate() {
                        {
                            // 闭包先拿走一份，剩下的 item 才能继续用于渲染
                            // （rsx 属性按书写顺序求值，move 闭包内不能再借 item）
                            let picked_item = item.clone();
                            rsx! {
                                div {
                                    key: "{item.key()}",
                                    class: if i == index { "mention-menu-item is-active" } else { "mention-menu-item" },
                                    onmouseenter: move |_| state.hover(i),
                                    // 用 mousedown 而非 click：click 前 textarea 会先失焦，
                                    // 导致光标位置丢失、插入点错乱
                                    onmousedown: move |e| {
                                        e.prevent_default();
                                        // 先对齐高亮再回调，保证 confirm() 取到的是鼠标所在项
                                        state.hover(i);
                                        on_pick.call(picked_item.clone());
                                    },
                                    span { class: "mention-chip {item.kind.chip_class()}", "@{item.name}" }
                                    if !item.subtitle.is_empty() {
                                        span { class: "text-xs text-base-content/50 truncate", "{item.subtitle}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "mention-menu-hint", "↑↓ 选择 · Enter 确认 · Esc 取消" }
        }
    }
}
