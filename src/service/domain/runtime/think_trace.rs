//! 轮级思考图轨迹（`think_path` run trace）的落盘出口与查询入口
//!
//! 与 `tool_tracing` 同一套「显式初始化 + 未初始化即 no-op」模式：
//! 生产环境在 [`crate::service::domain::runtime::init`] 注入 base_data_path；单元测试不初始化，
//! 因此不会往 `~/.ai_orz` 写文件（测试隔离红线）。
//!
//! 落盘是「逐轮增量 + 收尾全量」两段式（`agents/{agent_id}/think_trace/{YYYYMMDD}.jsonl`）：
//! - 每轮定型（think → dispatch → evaluate）后追加一行 `partial=true`，只含该轮明细；
//! - run 收尾追加一行 `partial=false`，含完整 rounds 与节点 occurrence 序列、选路来源。
//!
//! 查询按 `(agent_id, run_id, scene)` 合并：收尾行只替换**本片段**的增量 rounds、
//! 片段之间拼接续编号，因此前端在 run 未结束时查询也能看到已完成轮次（半成品），
//! 异常中断也不会整条丢失。
//!
//! 查询侧按 **agent 维度** 分页：`agent_id` 缺省时遍历 `agents/*/think_trace`，
//! 供对话页「多 Agent 聚合流」使用；`project_id` / `task_id` 是 entry 字段过滤
//! （2026-10-22 起新写入的轨迹才有值，历史轨迹该字段为空）。
//!
//! 同一图 run 可能因新输入重跑思考循环（意图修订 / 执行回退），每次重跑追加一行；
//! 查询在分页前按 `(agent_id, run_id, scene)` 合并成一个动作，避免轨迹面板把
//! 同一个动作展示成多个条目、各自统计轮次。

#[path = "think_trace_sidecar.rs"]
mod think_trace_sidecar;

use crate::pkg::jsonl::writer::DailyJsonlWriter;
use crate::pkg::paths;
use common::api::{PagedResult, PaginationParams};
use common::enums::ThinkingIntensity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use think_trace_sidecar::ThinkSidecar;

/// 单次查询允许的最大条数（JSONL 扫描开销控制）
pub const MAX_THINK_TRACE_QUERY_LIMIT: usize = 100;

/// 未显式指定 limit 时的默认页大小
pub const DEFAULT_THINK_TRACE_QUERY_LIMIT: usize = 20;

static BASE_DATA_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 启动期注入基础数据目录（重复调用无副作用）
pub fn init(base_data_path: impl Into<PathBuf>) {
    let _ = BASE_DATA_PATH.set(base_data_path.into());
}

/// 追加一条轨迹
///
/// 未初始化时静默跳过并返回 `Ok(false)`——测试或独立调用场景不应产生文件副作用。
/// 落盘前补齐 `created_at`（毫秒）：查询分页与时间过滤依赖该字段，调用方漏填时
/// 由出口统一兜底，避免历史行出现「有轨迹但无法排序/过滤」的断层。
pub fn append(agent_id: &str, entry: &serde_json::Value) -> anyhow::Result<bool> {
    let Some(base) = BASE_DATA_PATH.get() else {
        return Ok(false);
    };
    let mut entry = entry.clone();
    if entry.get("created_at").is_none() {
        entry["created_at"] =
            serde_json::Value::from(chrono::Utc::now().timestamp_millis().max(0) as u64);
    }
    let trace_dir = paths::agent_think_trace_dir(base, agent_id);
    let writer = DailyJsonlWriter::new(&trace_dir);
    let sidecar = ThinkSidecar::new(trace_dir);
    // 原文 append + 索引 append 在同一把 APPEND_LOCK 临界区内；索引写失败只告警，
    // 原文已落盘，读路径会补齐。
    let entry_ref = &entry;
    writer.append_with_index(entry_ref, |date, offset, len| {
        if let Err(err) = sidecar.append_record(date, entry_ref, offset, len) {
            log_warn!(
                "思考轨迹索引追加失败，将在下次查询补齐: agent_id={}, date={}, run_id={}, err={}",
                agent_id,
                date,
                entry.get("run_id").and_then(|v| v.as_str()).unwrap_or(""),
                err
            );
        }
        Ok(())
    })?;
    Ok(true)
}

// ====================================================================
// 轨迹行的种类 + 写穿入口
// ====================================================================

/// 一行轨迹的种类
///
/// ## 为什么需要它
///
/// 同一条 trace 文件里混着三种粒度：逐轮增量（run 未结束时可见）、run 收尾
/// （完整行）、以及写穿的观测事实（小脑留痕等，随时可能追加）。合并逻辑必须
/// **按种类**决定语义，而不是继续用 `partial` 一职两用：
///
/// - 只有 [`Self::Close`] 会「摘掉本片段的增量轮次再拼收尾行」；
/// - 只有 [`Self::Close`] 会把合并结果的 `partial` 置为 `false`；
/// - [`Self::Fact`] 永远只做追加，不碰轮次与半成品标记。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceLineKind {
    /// 存量行（无 `kind` 字段）：收尾语义由 `partial` 推断
    #[default]
    Legacy,
    /// 逐轮增量行（`partial=true`）
    Round,
    /// run 收尾行（完整行）
    Close,
    /// 写穿的观测事实（`fact` 指明品类）
    Fact,
}

impl TraceLineKind {
    /// 是否为 run 收尾行
    ///
    /// 存量行按 `partial` 兼容推断：`kind` 落地之前，`partial=false` 只用于收尾。
    fn is_close(self, partial: bool) -> bool {
        match self {
            Self::Close => true,
            Self::Legacy => !partial,
            Self::Round | Self::Fact => false,
        }
    }
}

/// 一条 run 的身份信封：所有写穿数据的统一落点
///
/// ## 定位
///
/// 它是「图上下文」的只读投影，不是收集器：不持有任何业务数据，只回答
/// 「这条事实属于哪个 Agent / 哪次 run / 哪个场景」。数据和它的关系类似
/// 「事件 + 分区键」——事件立即落盘，分区键决定它合并进哪条轨迹。
///
/// 构造点就是 run 的入口（`run_think_loop` / 各节点的 `deps`），因此所有权
/// 始终显式，进程里没有 `HashMap<run_id, _>` 这类登记表。
///
/// ## 扩展方式
///
/// 新增一种需要「相对实时可见」的观测数据，只需一次调用：
///
/// ```rust,ignore
/// sink.fact("retrieval", json!({ "hits": hits, "query": terms }));
/// ```
///
/// 落盘行形如 `{"kind":"fact","fact":"retrieval","hits":[...],"query":[...], ...}`，
/// 旧读者忽略未知字段；将来消费侧在 `merge_into` 里按 `fact` 名追加即可。
#[derive(Debug, Clone, Default)]
pub struct TraceSink {
    pub agent_id: String,
    pub run_id: String,
    pub scene: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    /// run 是否已写过收尾行：正常收尾（图跑完）与 RAII 兜底（早退 / Err / 超时）
    /// 共享这一位，保证只有一方写 close，不产生重复收尾行。
    closed: Arc<AtomicBool>,
    /// 测试专用：不落盘，把写出的行留在内存里供断言
    ///
    /// 写穿是本模块唯一的副作用出口，测试若真写文件就得依赖全局落盘基址
    /// （`OnceLock`，进程内只能 init 一次）——用例之间会互相串味、也无法并行。
    #[cfg(test)]
    sink: Option<std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>>,
}

/// [`TraceSink::fact`] 保留键：载荷不许覆盖身份字段，也不许覆盖**合并语义字段**
///
/// 前一组是「这行属于谁」，后一组是「这行参与怎样的合并」。观测品类可以自由
/// 往行上平铺自己的字段，但一旦撞上这两组，就会改掉轨迹的归属或轮次拼接规则
/// （例如某个品类顺手传了 `partial: false`，前端就会把半成品当已完成）。
/// `cerebellum` 是刻意的例外：留痕正是靠它随事实行落盘，由
/// [`TraceSink::cerebellum`] 自己写。
const RESERVED_KEYS: &[&str] = &[
    // 身份 / 行种类
    "kind",
    "fact",
    "agent_id",
    "run_id",
    "scene",
    "project_id",
    "task_id",
    "created_at",
    // 合并语义（见 `merge_into`）
    "partial",
    "path",
    "steps",
    "advisor_calls",
    "fingerprint",
    "rounds",
    "occurrences",
    "routes",
];

impl TraceSink {
    /// 构造一条 run 的身份信封（项目 / 任务维度随后用 [`Self::with_scope`] 补）
    pub fn new(agent_id: impl Into<String>, run_id: impl Into<String>, scene: &str) -> Self {
        Self {
            agent_id: agent_id.into(),
            run_id: run_id.into(),
            scene: scene.to_string(),
            project_id: None,
            task_id: None,
            closed: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            sink: None,
        }
    }

    /// 测试专用：把写穿重定向到内存（不碰文件系统、不依赖全局落盘基址）
    #[cfg(test)]
    pub fn memory(agent_id: impl Into<String>, run_id: impl Into<String>, scene: &str) -> Self {
        let mut scope = Self::new(agent_id, run_id, scene);
        scope.sink = Some(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
        scope
    }

    /// 测试专用：取走本 scope 已经写出的全部行
    #[cfg(test)]
    pub fn recorded(&self) -> Vec<serde_json::Value> {
        self.sink
            .as_ref()
            .and_then(|sink| sink.lock().ok().map(|lines| lines.clone()))
            .unwrap_or_default()
    }

    /// 补上项目 / 任务维度（对话页右侧「思考轨迹」按项目聚合时用）
    pub fn with_scope(mut self, project_id: Option<&str>, task_id: Option<&str>) -> Self {
        self.project_id = project_id.map(str::to_string);
        self.task_id = task_id.map(str::to_string);
        self
    }

    /// run 的身份是否完整（空 run_id 表示「没有 trace 可写」，静默丢弃）
    fn is_writable(&self) -> bool {
        !self.run_id.is_empty() && !self.agent_id.is_empty()
    }

    /// 身份信封（每次落盘都带全，查询侧靠它归并到同一条轨迹）
    fn envelope(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        map.insert("run_id".into(), self.run_id.clone().into());
        map.insert("agent_id".into(), self.agent_id.clone().into());
        map.insert("scene".into(), self.scene.clone().into());
        if let Some(project_id) = &self.project_id {
            map.insert("project_id".into(), project_id.clone().into());
        }
        if let Some(task_id) = &self.task_id {
            map.insert("task_id".into(), task_id.clone().into());
        }
        map
    }

    /// 落一行（写失败只告警：轨迹是排障数据，绝不能反过来拖垮主流程）
    fn write(&self, entry: serde_json::Value) {
        // 测试：写进内存，避免依赖全局落盘基址（见 `sink` 字段说明）
        #[cfg(test)]
        if let Some(sink) = &self.sink {
            if let Ok(mut lines) = sink.lock() {
                lines.push(entry);
            }
            return;
        }
        if let Err(e) = append(&self.agent_id, &entry) {
            log_warn!(
                "思考轨迹写穿失败: run_id={}, scene={}, kind={}, err={}",
                self.run_id,
                self.scene,
                entry.get("kind").and_then(|v| v.as_str()).unwrap_or(""),
                e
            );
        }
    }

    /// 构造一条观测事实的行（纯函数；写盘由 [`Self::fact`] 负责）
    ///
    /// 单独抽出来是为了可测：行形态（`kind` / `fact` / 载荷平铺 / 身份字段不可覆盖）
    /// 是本模块对外的扩展契约，必须能在不碰文件系统的前提下锁死。
    fn fact_entry(&self, name: &str, payload: serde_json::Value) -> Option<serde_json::Value> {
        if !self.is_writable() {
            return None;
        }
        let mut entry = self.envelope();
        entry.insert("kind".into(), "fact".into());
        entry.insert("fact".into(), name.into());
        match payload {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if RESERVED_KEYS.contains(&key.as_str()) {
                        continue;
                    }
                    entry.insert(key, value);
                }
            }
            other => {
                entry.insert("payload".into(), other);
            }
        }
        Some(serde_json::Value::Object(entry))
    }

    /// 写穿一条观测事实（`kind=fact`）
    ///
    /// `payload` 为对象时**平铺**到行上（便于按字段名 grep 与将来按字段追加），
    /// 非对象则收进 `payload`；保留键一律忽略，防止载荷覆盖身份字段。
    pub fn fact(&self, name: &str, payload: serde_json::Value) {
        if let Some(entry) = self.fact_entry(name, payload) {
            self.write(entry);
        }
    }

    /// 追加一条小脑决策留痕
    pub fn cerebellum(&self, item: ThinkTraceCerebellum) {
        self.fact("cerebellum", serde_json::json!({ "cerebellum": [item] }));
    }

    /// 追加一行「本轮思考明细」（run 未结束时前端即可见）
    ///
    /// ⚠️ 刻意**同步**写盘，不要改成 `spawn_blocking` / 后台队列：JSONL 是
    /// append-only，行序即轮次序，而 `merge_into` 的「增量行累积 → 收尾行替换」
    /// 完全依赖这个顺序。一旦并发写导致后一轮先落盘，合并出的轨迹轮次就会错序。
    /// 单轮一次小写入的阻塞代价，远小于轨迹错乱的排障代价。
    pub fn round(&self, path: &str, round: &ThinkTraceRound) {
        if !self.is_writable() {
            return;
        }
        let mut entry = self.envelope();
        entry.insert("kind".into(), "round".into());
        entry.insert("partial".into(), true.into());
        entry.insert("path".into(), path.into());
        entry.insert("steps".into(), 0.into());
        entry.insert("advisor_calls".into(), 0.into());
        entry.insert("fingerprint".into(), "".into());
        entry.insert("rounds".into(), serde_json::json!([round]));
        self.write(serde_json::Value::Object(entry));
    }

    /// 写一行 run 收尾（`kind=close`, `partial=false`），并保证同一 run 只写一次
    ///
    /// 供 [`TraceFinishGuard`] 在所有退出路径（含早退 / Err / 外部 timeout）兜底；
    /// 图正常跑完时由 `persist_trace` 写含完整轮次的 close 行并先置 `closed`。
    /// 兜底行不带轮次明细，合并语义见 `merge_into`：只会把半成品标记关掉，
    /// 已有轮次不受影响。
    pub fn close_once(&self, reason: &str) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        if !self.is_writable() {
            return;
        }
        let mut entry = self.envelope();
        entry.insert("kind".into(), "close".into());
        entry.insert("partial".into(), false.into());
        entry.insert("close_reason".into(), reason.into());
        entry.insert("rounds".into(), serde_json::json!([]));
        entry.insert("occurrences".into(), serde_json::json!([]));
        entry.insert("routes".into(), serde_json::json!([]));
        self.write(serde_json::Value::Object(entry));
    }

    /// 标记「收尾行已由正常路径写出」，供 RAII guard 跳过兜底写入
    pub fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

/// run 收尾的 RAII 兜底：无论 `run_think_loop` 从哪条路径退出都关闭轨迹
///
/// 正常路径由 `persist_trace` 写完整 close 行；小脑 Direct 直回、零可用轮次、
/// 预算耗尽、LLM 错误预算耗尽、外部 timeout 这些路径都不会走到 `persist_trace`，
/// 没有本 guard 就只剩 `kind=fact` 行，查询侧会永久显示「进行中」。
/// 使用 Drop 而不是逐分支调用，避免后续新增早退路径时再次遗漏。
#[derive(Debug)]
pub struct TraceFinishGuard {
    sink: TraceSink,
    reason: String,
}

impl TraceFinishGuard {
    pub fn new(sink: TraceSink, reason: impl Into<String>) -> Self {
        Self {
            sink,
            reason: reason.into(),
        }
    }

    /// 覆盖默认原因（例如「小脑直回」而不是泛化的「已完成」）
    pub fn set_reason(&mut self, reason: impl Into<String>) {
        self.reason = reason.into();
    }
}

impl Drop for TraceFinishGuard {
    fn drop(&mut self) {
        self.sink.close_once(&self.reason);
    }
}

// ====================================================================
// 轨迹条目（读取侧类型）
// ====================================================================

/// 一次轮级思考循环的轨迹条目
///
/// 字段名与落盘 JSON 一一对应；`#[serde(default)]` 让历史行/字段扩展时仍可读取。
/// 唯一的例外是 `partial_round_count` —— 它是查询合并期的内部状态，不落盘。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThinkTraceEntry {
    /// 本次 run 的唯一 ID（图引擎生成）
    #[serde(default)]
    pub run_id: String,
    /// 发起本次思考的 Agent
    #[serde(default)]
    pub agent_id: String,
    /// 思考场景（awaken / settle / summary / compact / intent-analyze）
    #[serde(default)]
    pub scene: String,
    /// 轨迹使用的蓝图 ID（固定为 `think_round`）
    #[serde(default)]
    pub path: String,
    /// 图执行步数
    #[serde(default)]
    pub steps: usize,
    /// advisor 介入次数
    #[serde(default)]
    pub advisor_calls: usize,
    /// 轨迹指纹（同拓扑同来源时的去重/对比依据）
    #[serde(default)]
    pub fingerprint: String,
    /// 节点 occurrence 序列
    #[serde(default)]
    pub occurrences: Vec<ThinkTraceOccurrence>,
    /// 每次选路的候选与来源
    #[serde(default)]
    pub routes: Vec<ThinkTraceRoute>,
    /// 落盘时间戳（毫秒，append 出口兜底补齐）
    #[serde(default)]
    pub created_at: u64,
    /// 关联项目 ID（2026-10-22 起写入）
    #[serde(default)]
    pub project_id: Option<String>,
    /// 关联任务 ID（2026-10-22 起写入）
    #[serde(default)]
    pub task_id: Option<String>,
    /// 本行的种类（决定合并语义：谁截断增量轮次、谁只做追加）
    ///
    /// 存量行没有该字段 ⇒ [`TraceLineKind::Legacy`]，按 `partial` 推断收尾语义。
    #[serde(default)]
    pub kind: TraceLineKind,
    /// 观测事实的名称（仅 `kind=fact`；如 `cerebellum`）
    ///
    /// 与 `kind` 一起构成本模块的**扩展点**：新增观测品类只需调用
    /// [`TraceSink::fact`] 写一行 `kind=fact, fact=<名字>, <名下的载荷字段>`，
    /// 不需要改 schema，也不会影响旧读者（未知字段被 serde 忽略）。
    #[serde(default)]
    pub fact: Option<String>,
    /// 轮级明细：模型思考文本、reasoning summary、工具调用与决策
    #[serde(default)]
    pub rounds: Vec<ThinkTraceRound>,
    /// 本次 run 内的小脑决策留痕（routing / 思考强度 / 检索精选）
    ///
    /// 由 [`TraceSink::cerebellum`] **写穿**：每条留痕独立落一行
    /// （`kind=fact` / `fact=cerebellum`），查询合并期按 `(agent_id, run_id, scene)`
    /// 拼接。内存里不攒批——读者在进程外（前端 / 查询接口），攒批只会推迟可见性
    /// 并引入「谁来 flush」的所有权问题。
    #[serde(default)]
    pub cerebellum: Vec<ThinkTraceCerebellum>,
    /// 是否为「运行中」的行（对前端 = 这条轨迹还没收尾）
    ///
    /// 思考循环每轮定型（think → dispatch → evaluate）后追加一行 `kind=round`
    /// （`partial=true`），只携带该轮的 `rounds` 明细；run 收尾再追加一行
    /// `kind=close`（`partial=false`），携带完整 `rounds` 与图执行信息。
    ///
    /// ⚠️ 收尾语义**不再**由本字段单独决定（见 [`TraceLineKind::is_close`]）：
    /// 写穿的事实行也带 `partial=true`，若用 `!partial` 判收尾会误触发「摘增量」。
    /// 本字段只回答「查询合并后的结果是否仍是半成品」。
    #[serde(default)]
    pub partial: bool,
    /// 查询合并期内部状态：当前片段已累积的逐轮增量 `rounds` 条数（不落盘）
    ///
    /// 一次唤醒内可能因意图修订多次重跑，每个片段各自「N 行增量 + 1 行收尾」。
    /// 收尾行需要摘掉**本片段**的增量轮次再拼接，否则第二个片段的收尾行会把
    /// 第一个片段的轮次一起冲掉。这个计数精确反映实际收到的增量行数（写失败
    /// 只告警，可能少于收尾行的全量轮数），不能用 `next.rounds.len()` 代替。
    #[serde(skip)]
    pub partial_round_count: usize,
}

/// 单轮思考明细
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkTraceRound {
    /// 轮次编号（1-based，跨压缩累计）
    #[serde(default)]
    pub round: usize,
    /// 本轮模型可见文本（含工具轮里的思考说明）
    #[serde(default)]
    pub assistant_text: Option<String>,
    /// 本轮推理摘要（仅记录 provider 返回的可读 summary，密文永不落盘）
    #[serde(default)]
    pub reasoning: Vec<String>,
    /// 本轮工具调用及结果
    #[serde(default)]
    pub tool_calls: Vec<ThinkTraceToolCall>,
    /// 本轮输入 token
    #[serde(default)]
    pub input_tokens: u64,
    /// 本轮输出 token
    #[serde(default)]
    pub output_tokens: u64,
    /// 本轮提示缓存命中的输入 token（provider 未上报时为 0——不代表未命中）
    #[serde(default)]
    pub cached_tokens: u64,
    /// 本轮思考/推理消耗的输出 token（`output_tokens` 的子集；未上报时为 0）
    #[serde(default)]
    pub reasoning_tokens: u64,
    /// 本轮 LLM 调用耗时（毫秒）
    #[serde(default)]
    pub duration_ms: u64,
    /// 本轮的裁决结果：final / continue / reflect / exit
    #[serde(default)]
    pub decision: Option<String>,
    /// 本轮调用模型时实际下发的思考强度（0–100 分）；
    /// None = Provider 未启用该能力（请求体不下发）。按轮记录而非按 run，
    /// 未来支持逐轮动态调整时无需改存储结构。
    #[serde(default)]
    pub thinking_intensity: Option<ThinkingIntensity>,
}

/// run 内一次小脑调用的决策留痕
///
/// 小脑是可选增强：它的每次判断要么产出结论（进不进主流程、强度多少、
/// 哪些条目入选），要么降级走原逻辑。两种结果都必须能被事后追问，
/// 否则「小脑到底判了什么」只能靠翻日志。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThinkTraceCerebellum {
    /// 场景（`DecisionRequest.scene`：route / thinking_strength / memory_relevance / skill_relevance）
    #[serde(default)]
    pub scene: String,
    /// 决策结论（选中的选项或分数；`fallback` = 未产出可用结论）
    #[serde(default)]
    pub decision: String,
    /// 置信度（Pick 的概率；未产出结论时为 None）
    #[serde(default)]
    pub confidence: Option<f32>,
    /// 降级原因（None = 正常出结论）
    #[serde(default)]
    pub degraded: Option<String>,
    /// 是否真正影响了下游行为
    ///
    /// `false` 的典型场景：小脑算出了强度分数，但 Provider 未配置
    /// `thinking_intensity`（能力开关关闭），分数只留痕、请求体不下发。
    #[serde(default)]
    pub applied: bool,
    /// 本次判定的真实调用耗时（毫秒）；未发生调用时为 `None`
    ///
    /// 存在的唯一理由：**p95 时延的回收点**。时延只进日志的话，「这个场景到底
    /// 会不会恒超时」只能靠 grep 日志重算；进轨迹后可直接按 `scene` 分位统计。
    /// `applied=false`（放行）与 `degraded`（降级）也必须记 —— 它们同样付了
    /// 这次调用的时延，漏记会把 p95 系统性低估（只统计成功样本）。
    #[serde(default)]
    pub latency_ms: Option<u64>,
}

/// 单轮内的一次工具调用
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkTraceToolCall {
    /// 工具调用 trace ID（可据此查 tool_tracing 的完整入参/出参）
    #[serde(default)]
    pub call_id: Option<String>,
    /// 工具 wire name
    #[serde(default)]
    pub name: String,
    /// 模型自述的调用意图（`internal_intent` 附注；缺失为 None）
    ///
    /// 与 `arguments` 分开存：入参是「传了什么」，意图是「想干什么」。
    /// 前端时间线用它做摘要，不必再从参数反推。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// 入参 JSON 预览（截断；不含 `internal_intent`）
    #[serde(default)]
    pub arguments: String,
    /// 结果预览（截断；未执行完成为 None）
    #[serde(default)]
    pub result: Option<String>,
    /// 执行状态：ok / error / blocked / not_found / pending
    #[serde(default)]
    pub status: String,
}

/// 单个节点的执行记录
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkTraceOccurrence {
    /// 节点名（blueprint 中的 NodeDef）
    #[serde(default)]
    pub node: String,
    /// 节点执行序号（同节点重复执行时递增）
    #[serde(default)]
    pub seq: usize,
    /// 所属图步编号
    #[serde(default)]
    pub step: usize,
    /// 节点产出的准入信号（None 表示未产出）
    #[serde(default)]
    pub signal: Option<String>,
}

/// 一次选路记录
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkTraceRoute {
    /// 图步编号
    #[serde(default)]
    pub step: usize,
    /// 候选节点
    #[serde(default)]
    pub candidates: Vec<String>,
    /// 实际选择节点
    #[serde(default)]
    pub chosen: String,
    /// 选路来源（unique / deterministic / advisor / fallback）
    #[serde(default)]
    pub via: String,
    /// 选择原因（advisor / fallback 时通常有值）
    #[serde(default)]
    pub reason: Option<String>,
}

// ====================================================================
// 查询
// ====================================================================

/// Think trace 分页查询条件
///
/// `agent_id` 为 `None` 时遍历全部 Agent 目录；`agent_ids` 给定时只扫该集合
/// （对话页「当前任务出现过的 Agent」）。`project_id` / `task_id` 为 entry
/// 字段过滤，用于对话页按项目聚合多 Agent 轨迹。
///
/// `run_id` 是**精确匹配**：think trace 的 `run_id` 在 awaken / settle / compact
/// 场景下就是 MemoryTrace.id，因此记忆详情可凭自身 id 反查本次思考过程。
#[derive(Debug, Clone, Default)]
pub struct ThinkTraceQuery {
    pub agent_id: Option<String>,
    /// Agent 集合过滤（对话页「当前任务出现过的 Agent」聚合视角）；
    /// 与 `agent_id` 同时存在时两者都生效（交集）
    pub agent_ids: Option<Vec<String>>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    /// 精确匹配某次 run（即 MemoryTrace.id）
    pub run_id: Option<String>,
    pub scene: Option<String>,
    /// created_at 下界（含）
    pub started_after: Option<u64>,
    /// created_at 上界（含）
    pub started_before: Option<u64>,
    pub pagination: PaginationParams,
}

/// 查询 think trace（未初始化时返回空页，不产生文件系统副作用）
pub fn query(query: ThinkTraceQuery) -> anyhow::Result<PagedResult<ThinkTraceEntry>> {
    let Some(base) = BASE_DATA_PATH.get() else {
        return Ok(PagedResult::default());
    };
    query_at(base, &query)
}

/// 在指定基础目录下查询（包内可测，避免依赖全局初始化）
///
/// ## 成本模型：以及为什么这里不做早停
///
/// 查询走旁车索引：索引层只读轻量字段做过滤 + 日期粗筛，再按字节坐标 seek 回原文
/// 只反序列化**命中行**（不再全目录 + 全文件逐行反序列化 rounds/reasoning）。索引
/// 缺失/落后时由底层增量重建，语义与全量扫描完全一致。
///
/// ⚠️ 索引层**不截断命中数**：`total` 必须是 `merge_same_run` 后的真实总数，前端
/// `has_next = offset + len < total` 与「共 N 条」都依赖它。早停会把 total 变成
/// 「已扫描到的条数」，翻页按钮当场失效（永远推不出下一页）。正确方向是缩小时间
/// 窗口（调用方传 `started_after`，索引层在文件级粗筛掉整段历史分片）。
pub(crate) fn query_at(
    base_data_path: &Path,
    query: &ThinkTraceQuery,
) -> anyhow::Result<PagedResult<ThinkTraceEntry>> {
    let limit = query
        .pagination
        .limit
        .unwrap_or(DEFAULT_THINK_TRACE_QUERY_LIMIT);
    if limit > MAX_THINK_TRACE_QUERY_LIMIT {
        anyhow::bail!(
            "think trace query limit exceeds maximum {}",
            MAX_THINK_TRACE_QUERY_LIMIT
        );
    }
    let offset = query.pagination.offset.unwrap_or(0);

    // 【索引路径】每个 agent 目录一个旁车索引：索引层过滤 + 日期粗筛后只 seek 读命中行。
    // ⚠️ 不截断命中数——merge_same_run 必须在全部命中上做，total 才是真实值（见下方注释）。
    // 索引缺失/落后/损坏时由底层增量重建（兼容直接写原文的存量数据与测试夹具）。
    let mut entries = Vec::new();
    for agent_dir in agent_dirs(
        base_data_path,
        query.agent_id.as_deref(),
        query.agent_ids.as_deref(),
    )? {
        let trace_dir = agent_dir.join("think_trace");
        if !trace_dir.is_dir() {
            continue;
        }
        let sidecar = ThinkSidecar::new(trace_dir.clone());
        for hit in sidecar.query(query)? {
            match sidecar.read_entry(&hit.date, hit.offset, hit.len) {
                Ok(entry) => entries.push(entry),
                // 索引指向的行损坏/被清理：跳过并告警，与旧「单行损坏跳过」口径一致
                Err(err) => log_warn!(
                    "跳过无法读取的思考轨迹行: dir={}, date={}, offset={}, err={}",
                    trace_dir.display(),
                    hit.date,
                    hit.offset,
                    err
                ),
            }
        }
    }

    // 同一 run 内可能因新输入重跑思考循环（意图修订 / 执行回退），每次重跑都会
    // 追加一行。先按时间正序把同一 `(agent_id, run_id, scene)` 的多行合成一条，
    // 否则「同一动作被重新触发」会被展示成两个动作、各自统计轮次。
    entries.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.run_id.cmp(&b.run_id))
    });
    let mut entries = merge_same_run(entries);

    // 最新在前；同一毫秒内按 run_id 倒序，保证分页顺序稳定
    entries.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.run_id.cmp(&a.run_id))
    });

    let total = entries.len();
    let items = entries.into_iter().skip(offset).take(limit).collect();
    Ok(PagedResult { items, total })
}

/// 合并同一 run 内被重新触发而追加的多行轨迹
///
/// `run_id` 标识一次图 run，但一次 run 里思考循环可能执行多次：
/// - 新消息触发意图修订时，`IntentNode` 重跑，`intent-{main}` 轨迹追加一行；
/// - `poll_exit` 回退执行时，Deliberate 的思考循环重跑，主 run 轨迹追加一行。
///
/// 这些行共享 `(agent_id, run_id, scene)`，是**同一个动作的不同片段**。查询层
/// 按该键聚合，让轨迹面板呈现一条连续路径，而不是两个各自计数的动作。
fn merge_same_run(entries: Vec<ThinkTraceEntry>) -> Vec<ThinkTraceEntry> {
    let mut merged: Vec<ThinkTraceEntry> = Vec::with_capacity(entries.len());
    // 键与前端展示标识同源（common::api::think_trace_entry_key）
    let mut positions: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        let key = common::api::think_trace_entry_key(&entry.agent_id, &entry.run_id, &entry.scene);
        match positions.get(&key).copied() {
            Some(index) => merge_into(&mut merged[index], entry),
            None => {
                // 首行不经过 merge_into，需要在这里定好片段的初始状态：
                //
                // ①「是否半成品」由**行种类**决定，不能直接信 `partial` 字段。
                //    写穿的事实行（`TraceSink::fact`）根本不含该字段，反序列化
                //    拿到的是 `serde(default)` 的 `false`；而入口门的留痕发生在
                //    第一轮之前，事实行常常就是首行 ⇒ 照搬字段会让 run 刚开跑
                //    就显示「已完成」（`merge_into` 只在收尾行置 false、从不置
                //    true，整条 run 都翻不回来）。
                // ② 增量行需补上片段计数，否则该行不会被本片段的收尾行摘除
                //    （会多出一轮）。
                let mut first = entry;
                if !first.kind.is_close(first.partial) {
                    first.partial = true;
                }
                if first.partial {
                    first.partial_round_count = first.rounds.len();
                }
                positions.insert(key, merged.len());
                merged.push(first);
            }
        }
    }
    merged
}

/// 把后一段轨迹并入前一段（入参按 `created_at` 正序，前一段即更早的片段）
///
/// - `created_at` / `project_id` / `task_id` 保留最早片段的值（run 起点语义，
///   与 `MemoryTrace.id` 的回查口径一致）
/// - `steps` / `advisor_calls` 求和
/// - `rounds` / `occurrences` / `routes` 按时间拼接，并让轮次、节点序号与图步号
///   连续：后一段若从 1 重新计数则整体后移，若已按唤醒级累计则原样保留
/// - `fingerprint` 由合并后的 occurrence 序列重算
fn merge_into(base: &mut ThinkTraceEntry, next: ThinkTraceEntry) {
    let step_offset = base.steps;

    // 收尾行到达：先摘掉「本片段」逐轮增量累积的 rounds，
    // 再按跨片段偏移拼接收尾行的完整 rounds。
    //
    // ⚠️ 判据是**行种类**而不是 `!partial`：写穿的观测事实（小脑留痕等）
    // 也带 `partial=true`，但它们没有轮次，不该参与「摘增量」；反过来，
    // 它们出现时也不能把合并结果重新标成半成品。
    //
    // ⚠️ 只摘本片段，不能整体替换 `base.rounds`：一次唤醒内可能因意图修订多次
    // 重跑（见 [`merge_same_run`] 注释），每个片段各自 N 行增量 + 1 行收尾。
    // 早期版本「收尾行整体替换」会让第二个片段的收尾行把第一个片段的轮次
    // 一并冲掉；直接拼接又会让本片段每轮出现两次。
    // 增量行不带图执行信息，故 steps / occurrences / routes 的累加不受影响。
    let final_line = next.kind.is_close(next.partial);
    let next_round_count = next.rounds.len();
    if final_line {
        let drop_count = base.partial_round_count.min(base.rounds.len());
        base.rounds.truncate(base.rounds.len() - drop_count);
        base.partial_round_count = 0;
    }

    // 轮次：意图重跑从 1 计数，需要后移；执行回退已带 `start_round`（累计值），
    // 已是连续编号，不能再加偏移
    let round_offset = match (base.rounds.last(), next.rounds.first()) {
        (Some(last), Some(first)) if first.round <= last.round => {
            last.round.saturating_add(1).saturating_sub(first.round)
        }
        _ => 0,
    };
    for mut round in next.rounds {
        round.round = round.round.saturating_add(round_offset);
        base.rounds.push(round);
    }
    // 增量行可能部分写失败（只告警），按实际收到的行数累计，供本片段收尾行摘除
    if !final_line {
        base.partial_round_count = base.partial_round_count.saturating_add(next_round_count);
    }

    // occurrence：节点级序号跨片段续编号，图步号整体后移。
    // 偏移必须取自合并前的快照：同一片段内后面的 occurrence 不能再吃一遍偏移。
    let base_seq_by_node: HashMap<String, usize> = base
        .occurrences
        .iter()
        .map(|occurrence| (occurrence.node.clone(), occurrence.seq))
        .collect();
    for mut occurrence in next.occurrences {
        if let Some(previous) = base_seq_by_node.get(&occurrence.node).copied() {
            occurrence.seq = occurrence.seq.saturating_add(previous);
        }
        occurrence.step = occurrence.step.saturating_add(step_offset);
        base.occurrences.push(occurrence);
    }

    for mut route in next.routes {
        route.step = route.step.saturating_add(step_offset);
        base.routes.push(route);
    }

    // 小脑留痕：写穿落盘，同一 run 的多行按时间拼接。
    //
    // 不需要去重：写穿是「一条留痕一行」，不存在同一批次被写两次的路径；
    // 若将来出现「一次记录拆成多行」的写法，这里才是需要加去重的地方。
    base.cerebellum.extend(next.cerebellum);

    base.steps = base.steps.saturating_add(next.steps);
    base.advisor_calls = base.advisor_calls.saturating_add(next.advisor_calls);
    // 指纹是实际 occurrence 序列的压缩表达，合并后必须重算
    base.fingerprint = base
        .occurrences
        .iter()
        .map(|occurrence| format!("{}#{}", occurrence.node, occurrence.seq))
        .collect::<Vec<_>>()
        .join(">");

    if base.project_id.is_none() {
        base.project_id = next.project_id;
    }
    if base.task_id.is_none() {
        base.task_id = next.task_id;
    }
    // 首个片段可能只是一行写穿事实（没有 path / 蓝图信息）：图执行信息随收尾行
    // 才到达，这里补上空值，否则合并结果的 path 永远是空串。
    if base.path.is_empty() {
        base.path = next.path;
    }
    // 半成品标记只由收尾行关闭：写穿的事实行（`kind=fact`）随时可能追加到
    // 收尾行之后（例如 chain_reply 的留痕），它们**不能**把已完成轨迹重新标成
    // 半成品——前端靠这个位判断「还在跑」。
    if final_line {
        base.partial = false;
    }
}

/// 列出待扫描的 Agent 数据目录
fn agent_dirs(
    base_data_path: &Path,
    agent_id: Option<&str>,
    agent_ids: Option<&[String]>,
) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(agent_id) = agent_id {
        return Ok(vec![paths::agent_data_dir(base_data_path, agent_id)]);
    }
    // 显式 Agent 集合：只扫这几个目录（排序 + 去重，保证扫描顺序稳定）
    if let Some(agent_ids) = agent_ids {
        let mut dirs: Vec<PathBuf> = agent_ids
            .iter()
            .map(|id| paths::agent_data_dir(base_data_path, id))
            .collect();
        dirs.sort();
        dirs.dedup();
        return Ok(dirs);
    }
    let root = paths::agents_root_dir(base_data_path);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}

#[cfg(test)]
#[path = "think_trace_tests.rs"]
mod tests;
