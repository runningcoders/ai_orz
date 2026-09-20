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
//! - **候选分两级加载**：空关键词走 list 取作用域内实体做首屏推荐，非空关键词
//!   （1 字符起）一律走 search 语义召回 —— 后端 FTS5 已有 LIKE 兜底，短关键词
//!   不再返回空（实现见 [`load_candidates`]）。仅联邦 Agent 这条数据源本质是
//!   列表的路径保留 list + 本地过滤。list 路径**只取一屏所需**，不做大池 ——
//!   菜单屏幅有限，首屏没找到时继续打字走 search 才是正解。
//! - **范围分层打标**：候选与当前会话作用域的关系用行尾中性标签解释（见
//!   [`MentionFlag`]）—— 默认对话把接待 Agent 置顶打「当前接待」；项目会话
//!   把项目内 Agent 打头、组织其余打「未在项目内」缀后，当前项目置顶打
//!   「当前项目」。作用域内的永远排前面，桶内不破坏相关性排序。
//!
//! ## 用法
//!
//! ```ignore
//! let mention = MentionState::new(project_id, reception_agent);
//! // project_id: Signal<Option<String>>，reception_agent: Signal<Option<GetReceptionAgentResponse>>
//! // oninput: mention.sync(&value, caret)
//! // onkeydown: mention.move_selection(±1) / mention.confirm(&input_text())
//! rsx! {
//!     MentionPicker {
//!         state: mention,
//!         tabs: mention_tabs(&[MentionKind::Agent, MentionKind::Project]),
//!         on_pick: on_pick_mention,
//!     }
//! }
//! ```

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::api::hr::{get_agent, query_agents, search_agents};
use crate::api::organization::list_federation_agents;
use crate::api::project::{
    get_project, list_project_tasks, list_projects, list_tasks, search_projects, search_tasks,
};
use crate::utils::mention::{
    MentionKind, MentionQuery, MentionRef, apply_mention_pick, detect_mention_query,
    format_mention_ref, remove_mention_token,
};
use common::api::{
    AgentListItem, AgentQueryRequest, GetAgentRequest, GetProjectRequest,
    GetReceptionAgentResponse, ListProjectsRequest, ListTasksRequest, PaginationParams,
    ProjectListItem, SearchAgentsRequest, SearchProjectsRequest, SearchTasksRequest, TaskListItem,
};

/// 候选展示上限：单类型上限，同时是 search 路径的召回上限
///
/// 取值按**可浏览性**定，不按数据量：`.mention-menu-body` 是 `max-height: 14rem`
/// （224px），一条 `.mention-menu-item`（`padding: 0.375rem` ×2 + `0.875rem` 字号）
/// 约占 32px —— 一屏实际只露得出 6–7 行，再多也全靠划。首屏没找到时用户的自然动作
/// 是**继续打字**（转 search 语义召回覆盖全量），而不是在浮层里翻页，所以给一屏半
/// 的余量即可。
const CANDIDATE_LIMIT: usize = 12;

/// 本地过滤的候选池大小：项目内 Agent 路径**带关键词**时的拉取量
///
/// 2× 展示上限。本地过滤看不到「被截掉的那部分」，池子只能略大于展示量，
/// 保证过滤后仍剩得下一屏；再大就是纯浪费（见 [`CANDIDATE_LIMIT`] 的可浏览性口径）。
const FILTER_POOL: usize = CANDIDATE_LIMIT * 2;

/// 项目内 Agent 路径的拉取量：无关键词只需首屏展示的条数，有关键词才多取一些给本地过滤留余地
fn fetch_limit(keyword: Option<&str>) -> usize {
    if keyword.is_some() {
        FILTER_POOL
    } else {
        CANDIDATE_LIMIT
    }
}

/// 范围标签：解释「这条候选与当前会话作用域的关系」
///
/// 渲染在选项行尾的中性胶囊（见 `.mention-menu-flag`），不参与类型配色 ——
/// 类型语义已由左侧彩色 chip 承担，标签只回答「它是不是当前范围里的」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionFlag {
    /// 默认对话：当前负责接待的 Agent（置顶）
    Reception,
    /// 项目会话：项目负责 Agent / PMO（置顶）
    Owner,
    /// 项目会话：当前所在项目（置顶；其他项目也可引用）
    CurrentProject,
    /// 项目会话：未参与该项目的 Agent（来自组织全量，缀后）
    OutOfProject,
}

impl MentionFlag {
    pub fn label(self) -> &'static str {
        match self {
            Self::Reception => "当前接待",
            Self::Owner => "负责人",
            Self::CurrentProject => "当前项目",
            Self::OutOfProject => "未在项目内",
        }
    }
}

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
    /// 范围标签（与当前会话作用域的关系，见 [`MentionFlag`]）
    pub flag: Option<MentionFlag>,
}

impl MentionCandidate {
    /// 打上范围标签（装配候选时的链式收尾）
    fn with_flag(mut self, flag: MentionFlag) -> Self {
        self.flag = Some(flag);
        self
    }

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
/// - 项目会话：Agent + 任务 + 项目（Agent 双桶分层：项目内打头、未在项目内的打标
///   缀后；项目可跨项目引用，当前项目置顶打标；任务仍只收项目内的）
/// - 默认对话：Agent + 项目（以项目维度维护上下文：接待 Agent 置顶打「当前接待」；
///   任务不进默认会话候选 —— 任务一律挂在项目下讨论，游离任务引导去项目里 @）
///
/// 单一事实源：`MentionState` 拉候选与 Tab 渲染都走这里，避免两处口径漂移。
pub fn mention_kinds_for(project_id: Option<&str>) -> Vec<MentionKind> {
    if project_id.is_some() {
        // 项目会话：@ Agent 双桶分层（项目内 + 组织其余），且开放 @ 项目本身
        vec![MentionKind::Agent, MentionKind::Task, MentionKind::Project]
    } else {
        // 默认对话：Agent 取组织全量（接待 Agent 置顶打标）+ 项目；不收任务
        vec![MentionKind::Agent, MentionKind::Project]
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
    /// 非空关键词无命中说明用户多半不是在找实体（在打邮箱、随手输了一串字），
    /// 此时自动收掉菜单。记下这个「死前缀」是为了**吸收后续输入**：在同一个 `@`
    /// 位置继续把它加长不会再把菜单弹回来，否则每次按键都是
    /// 「弹出 → 请求 → 无结果 → 关闭」，比不收还烦。
    ///
    /// ⚠️ 空关键词（首屏）无命中**不进这个机制**：首屏展示的是当前作用域的
    /// 实体列表，无命中只说明没有实体，与用户输入打偏无关。
    ///
    /// 解除条件：退格或换词（不再是前缀扩展）、换一个 `@` 位置，或用户主动
    /// 收起菜单（ESC / Enter / 发送，见 [`MentionState::close`]）。
    dead_prefix: Signal<Option<(usize, String)>>,
}

impl MentionState {
    /// 创建状态并启动候选加载 effect
    ///
    /// 两个入参都传 **Signal 而非值**：effect 同步读取它们，
    /// - 切换会话后候选拉取自动跟随新项目（按值捕获会让项目切换后仍查旧项目）；
    /// - 接待 Agent 异步到达（挂载后拉取）时补触发一次拉取，让「当前接待」
    ///   置顶打标在菜单打开期间也能生效，而不是等下一次输入。
    pub fn new(
        project_id: Signal<Option<String>>,
        reception_agent: Signal<Option<GetReceptionAgentResponse>>,
    ) -> Self {
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

        // 只同步读取 menu / project_id / reception_agent → 仅当「查询词变化 / 菜单
        // 开关 / 切换会话 / 接待 Agent 到达」时触发拉取，不会因为父组件的消息列表
        // 刷新而重跑（避免打断输入）
        use_effect(move || {
            // 先读接待 Agent 注册订阅（默认对话置顶打「当前接待」要用），
            // 再取 start：空结果时要回填「死前缀」（标注是哪个 @ 位置的无命中词）
            let reception = reception_agent().map(|a| (a.agent_id, a.agent_name));
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
                let list = load_candidates(pid, &kinds, &query, reception).await;
                // 丢弃过期响应：只有最新一次请求能写入结果
                if *state.req.read() == seq {
                    let empty = list.is_empty();
                    state.candidates.set(list);
                    state.loading.set(false);
                    // 非空关键词仍无命中 → 用户大概率不是在找实体，收掉菜单。
                    // ⚠️ 先写 dead_prefix 再收：不能走 close()，那会清掉抑制标记。
                    if empty && !query.trim().is_empty() {
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
///
/// **两级策略**：
/// - 空关键词：走 list 取当前作用域内的实体做首屏推荐（菜单一打开就有内容）。
/// - 非空关键词（1 字符起）：交给 search 的 FTS5 + 向量混合搜索。后端 FTS5 已有
///   LIKE 兜底，短关键词不再返回空，前端无需按长度分流。
/// - 例外：联邦 Agent 的数据源本质是收窄后的列表，无论关键词长短都走
///   list + 本地过滤（见 [`load_federation_agents`]）。
///
/// **范围分层**（标签见 [`MentionFlag`]）：项目会话的 Agent 双桶分层 —— 负责人
/// 置顶打「负责人」、项目内打头、组织其余打「未在项目内」缀后
/// （[`load_agent_candidates`]）；当前项目置顶打「当前项目」
/// （[`load_project_candidates`]）；默认对话的接待 Agent 置顶打「当前接待」。
/// 搜索命中也保持「作用域内在前」，桶内不破坏相关性排序。
async fn load_candidates(
    project_id: Option<String>,
    kinds: &[MentionKind],
    keyword: &str,
    reception: Option<(String, String)>,
) -> Vec<MentionCandidate> {
    let kw = keyword.trim();
    let kw = if kw.is_empty() { None } else { Some(kw) };
    let mut out = Vec::new();
    for kind in kinds {
        match kind {
            MentionKind::Agent => {
                out.extend(
                    load_agent_candidates(project_id.as_deref(), kw, reception.clone()).await,
                );
            }
            MentionKind::Task => {
                if let Some(kw) = kw {
                    out.extend(search_tasks_by_keyword(project_id.as_deref(), Some(kw)).await);
                } else {
                    out.extend(load_tasks(project_id.as_deref()).await);
                }
            }
            MentionKind::Project => {
                out.extend(load_project_candidates(project_id.as_deref(), kw).await);
            }
        }
    }
    out
}

/// Agent 候选：按会话作用域双桶分层
///
/// - **项目会话**：负责人（PMO）解析置顶打「负责人」——不领任务就不在 assignee
///   集合里，纯桶一推导会整个漏掉；桶一 = 项目内协作 Agent（任务 assignee 推导，
///   不打标、打头，负责人若在其中则摘出置顶）；桶二 = 组织全量里其余的 Agent
///   （打「未在项目内」缀后）。有关键词时桶二切 `search_agents` 服务端语义召回
///   （组织可能很大，list 全量本地过滤覆盖不全），桶内保持相关性排序；联邦 Agent
///   天然不在项目内，缀后不动。
/// - **默认对话**：组织全量（关键词走 search）+ 联邦；接待 Agent 若在候选中则
///   置顶打「当前接待」，首屏（无关键词）没刷出来时直接补一个置顶，保证
///   「当前接待」总是第一眼可见 —— 用户 @ 的第一直觉就是找接待 Agent。
async fn load_agent_candidates(
    project_id: Option<&str>,
    keyword: Option<&str>,
    reception: Option<(String, String)>,
) -> Vec<MentionCandidate> {
    match project_id {
        Some(pid) => {
            // 负责人（PMO）先解析：id 只有 get_project 有，必须单独取
            let owner_id = get_project(GetProjectRequest {
                id: pid.to_string(),
                ..Default::default()
            })
            .await
            .ok()
            .and_then(|p| p.owner_agent_id);

            let mut out = load_project_agents(pid, keyword).await;
            // 桶一已含负责人（领过任务）→ 摘出置顶打「负责人」标
            if let Some(oid) = owner_id.as_deref()
                && let Some(pos) = out.iter().position(|c| c.id == oid)
            {
                let c = out.remove(pos).with_flag(MentionFlag::Owner);
                out.insert(0, c);
            }
            let mut seen: HashSet<String> = out.iter().map(|c| c.key()).collect();
            let org = if let Some(kw) = keyword {
                search_org_agents(Some(kw)).await
            } else {
                load_org_agents().await
            };
            // 桶一漏了负责人 → 首屏合成置顶兜底：名字优先从组织全量取（零额外
            // 请求），没有再 get_agent；有关键词时交给桶二按命中打标，不置顶
            if keyword.is_none()
                && let Some(oid) = owner_id.clone()
                && !seen.contains(&format!("{}:{}", MentionKind::Agent.as_str(), oid))
            {
                let name = match org.iter().find(|c| c.id == oid) {
                    Some(c) => Some(c.name.clone()),
                    None => get_agent(GetAgentRequest {
                        id: oid.clone(),
                        ..Default::default()
                    })
                    .await
                    .ok()
                    .map(|a| a.name),
                };
                if let Some(name) = name {
                    seen.insert(format!("{}:{}", MentionKind::Agent.as_str(), oid));
                    out.insert(
                        0,
                        MentionCandidate {
                            kind: MentionKind::Agent,
                            id: oid,
                            org: None,
                            name,
                            subtitle: String::new(),
                            flag: Some(MentionFlag::Owner),
                        },
                    );
                }
            }
            let mut appended = 0usize;
            for c in org {
                if seen.insert(c.key()) {
                    let flag = if owner_id.as_deref() == Some(c.id.as_str()) {
                        MentionFlag::Owner
                    } else {
                        MentionFlag::OutOfProject
                    };
                    out.push(c.with_flag(flag));
                    appended += 1;
                    if appended >= CANDIDATE_LIMIT {
                        break;
                    }
                }
            }
            out.extend(load_federation_agents(keyword).await);
            out
        }
        None => {
            let mut out = if let Some(kw) = keyword {
                search_org_agents(Some(kw)).await
            } else {
                load_org_agents().await
            };
            if let Some((id, name)) = reception {
                let key = format!("{}:{}", MentionKind::Agent.as_str(), id);
                if let Some(pos) = out.iter().position(|c| c.key() == key) {
                    let c = out.remove(pos).with_flag(MentionFlag::Reception);
                    out.insert(0, c);
                } else if keyword.is_none() {
                    out.insert(
                        0,
                        MentionCandidate {
                            kind: MentionKind::Agent,
                            id,
                            org: None,
                            name,
                            subtitle: String::new(),
                            flag: Some(MentionFlag::Reception),
                        },
                    );
                }
            }
            out.extend(load_federation_agents(keyword).await);
            out
        }
    }
}

/// 项目候选：当前项目置顶打「当前项目」标，其余项目（组织全量）正常列出
///
/// 项目会话才打标（`project_id` 即当前项目）；默认对话无「当前」概念，原样返回。
/// 首屏（无关键词）没刷出当前项目时（列表分页/排序没盖到），兜底 `get_project`
/// 补一个置顶 —— 「标识出当前项目」是硬需求，不能让用户翻列表找自己所在的项目。
async fn load_project_candidates(
    project_id: Option<&str>,
    keyword: Option<&str>,
) -> Vec<MentionCandidate> {
    let mut out = if let Some(kw) = keyword {
        search_projects_by_keyword(Some(kw)).await
    } else {
        load_projects().await
    };
    let Some(pid) = project_id else {
        return out;
    };
    let key = format!("{}:{}", MentionKind::Project.as_str(), pid);
    if let Some(pos) = out.iter().position(|c| c.key() == key) {
        let c = out.remove(pos).with_flag(MentionFlag::CurrentProject);
        out.insert(0, c);
    } else if keyword.is_none()
        && let Some(name) = get_project(GetProjectRequest {
            id: pid.to_string(),
            ..Default::default()
        })
        .await
        .ok()
        .map(|p| p.name)
    {
        out.insert(
            0,
            MentionCandidate {
                kind: MentionKind::Project,
                id: pid.to_string(),
                org: None,
                name,
                subtitle: String::new(),
                flag: Some(MentionFlag::CurrentProject),
            },
        );
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
        flag: None,
    }
}

/// TaskListItem → 提及候选（任务类型）
fn task_to_candidate(t: TaskListItem) -> MentionCandidate {
    MentionCandidate {
        kind: MentionKind::Task,
        id: t.id,
        org: None,
        name: t.title,
        subtitle: format!("进度 {}%", t.progress),
        flag: None,
    }
}

/// ProjectListItem → 提及候选（项目类型）
fn project_to_candidate(p: ProjectListItem) -> MentionCandidate {
    MentionCandidate {
        kind: MentionKind::Project,
        id: p.id,
        org: None,
        name: p.name,
        subtitle: String::new(),
        flag: None,
    }
}

/// 本地包含匹配（大小写不敏感）+ 截断到展示上限：仍走本地过滤的路径（项目内
/// Agent、联邦 Agent）的共同收尾
///
/// 只匹配展示名。副标题（角色 / 进度 / 联邦对端组织名）不参与 —— 用户 @ 时想的是
/// 「那个 Agent / 任务 / 项目叫什么」，把副标题纳进来会带来成片意外命中。
fn filter_by_name(items: Vec<MentionCandidate>, keyword: Option<&str>) -> Vec<MentionCandidate> {
    match keyword {
        Some(kw) => {
            let kw = kw.to_lowercase();
            items
                .into_iter()
                .filter(|c| c.name.to_lowercase().contains(&kw))
                .take(CANDIDATE_LIMIT)
                .collect()
        }
        None => items.into_iter().take(CANDIDATE_LIMIT).collect(),
    }
}

/// 联邦 Agent 候选：聚合各 Active 对端开放的 Agent（P5）
///
/// 响应无搜索参数，全量拉回后本地按名称过滤；单个对端失败已在服务端跳过。
async fn load_federation_agents(keyword: Option<&str>) -> Vec<MentionCandidate> {
    let Ok(resp) = list_federation_agents().await else {
        return Vec::new();
    };
    let items: Vec<MentionCandidate> = resp
        .groups
        .into_iter()
        .flat_map(|g| {
            g.agents.into_iter().map(move |a| MentionCandidate {
                kind: MentionKind::Agent,
                id: a.id,
                org: Some(g.org_id.clone()),
                name: a.name.clone(),
                subtitle: format!("联邦 · {}", g.org_name),
                flag: None,
            })
        })
        .collect();
    filter_by_name(items, keyword)
}

/// 项目内可 @ 的 Agent = 项目下任务的 assignee（去重）
///
/// 项目没有成员表，任务 assignee 是「谁真正在这个项目干活」的唯一事实源，
/// 与 `pages/project/project_detail.rs` 推导协作 Agent 的口径保持一致。
///
/// 长短关键词都走 `query_agents(ids)` 再本地过滤：`AgentQuery.keyword` 已被后端
/// 废弃（仅打 warn 不生效），而 `search_agents` 不支持 ids 收窄 —— 走搜索会漏。
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
        pagination: PaginationParams {
            limit: Some(fetch_limit(keyword)),
            offset: None,
        },
        ..Default::default()
    };
    match query_agents(&req).await {
        Ok(page) => filter_by_name(
            page.items.into_iter().map(agent_to_candidate).collect(),
            keyword,
        ),
        Err(_) => Vec::new(),
    }
}

/// 默认对话下的 Agent **首屏**候选：组织全量 list 取一屏
///
/// 只服务空关键词，非空关键词一律走 [`search_org_agents`]。不传 keyword：
/// 后端 `AgentQuery.keyword` 已废弃且被静默忽略，传了只会制造「以为在过滤」的错觉。
async fn load_org_agents() -> Vec<MentionCandidate> {
    let req = AgentQueryRequest {
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

/// 默认对话下**长关键词**的 Agent 候选：交给 search 的语义召回
async fn search_org_agents(keyword: Option<&str>) -> Vec<MentionCandidate> {
    let req = SearchAgentsRequest {
        keyword: keyword.map(|s| s.to_string()),
        pagination: PaginationParams {
            limit: Some(CANDIDATE_LIMIT),
            offset: None,
        },
        ..Default::default()
    };
    match search_agents(&req).await {
        Ok(page) => page.items.into_iter().map(agent_to_candidate).collect(),
        Err(_) => Vec::new(),
    }
}

/// 任务**首屏**候选：list 取作用域内任务
///
/// 只服务空关键词，非空关键词一律走 [`search_tasks_by_keyword`]。
/// - 项目会话：`list_project_tasks(project_id)` 天然按项目收窄（返回该项目全量任务，
///   项目内任务量级可控）
/// - 默认对话：`list_tasks` 不做作用域过滤，显式带 limit 避免默认无上限拉全表
async fn load_tasks(project_id: Option<&str>) -> Vec<MentionCandidate> {
    match project_id {
        Some(pid) => match list_project_tasks(pid).await {
            Ok(resp) => resp
                .tasks
                .into_iter()
                .map(task_to_candidate)
                .take(CANDIDATE_LIMIT)
                .collect(),
            Err(_) => Vec::new(),
        },
        None => {
            let req = ListTasksRequest {
                pagination: PaginationParams {
                    limit: Some(CANDIDATE_LIMIT),
                    offset: None,
                },
            };
            match list_tasks(req).await {
                Ok(page) => page.items.into_iter().map(task_to_candidate).collect(),
                Err(_) => Vec::new(),
            }
        }
    }
}

/// 任务候选（长关键词路径）：FTS5 + 向量语义混合搜索
async fn search_tasks_by_keyword(
    project_id: Option<&str>,
    keyword: Option<&str>,
) -> Vec<MentionCandidate> {
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
        Ok(page) => page.items.into_iter().map(task_to_candidate).collect(),
        Err(_) => Vec::new(),
    }
}

/// 项目**首屏**候选：全局项目 list 取一屏。只服务空关键词，非空关键词一律走
/// [`search_projects_by_keyword`]。
async fn load_projects() -> Vec<MentionCandidate> {
    let req = ListProjectsRequest {
        pagination: PaginationParams {
            limit: Some(CANDIDATE_LIMIT),
            offset: None,
        },
    };
    match list_projects(req).await {
        Ok(page) => page.items.into_iter().map(project_to_candidate).collect(),
        Err(_) => Vec::new(),
    }
}

/// 项目候选（长关键词路径）：FTS5 + 向量语义混合搜索
async fn search_projects_by_keyword(keyword: Option<&str>) -> Vec<MentionCandidate> {
    let req = SearchProjectsRequest {
        keyword: keyword.map(|s| s.to_string()),
        pagination: PaginationParams {
            limit: Some(CANDIDATE_LIMIT),
            offset: None,
        },
        ..Default::default()
    };
    match search_projects(&req).await {
        Ok(page) => page.items.into_iter().map(project_to_candidate).collect(),
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
                                    if let Some(flag) = item.flag {
                                        span { class: "mention-menu-flag", "{flag.label()}" }
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
