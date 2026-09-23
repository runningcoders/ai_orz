//! 同步仓库「系统预置技能」到运行期数据目录（预置行本体 + 各 Agent 的装机副本）
//!
//! # 为什么需要它
//!
//! 预置技能**只在「新建 Agent / 应用预设」时写库**，启动时没有同步机制——内置工具有
//! `sync_builtin_tools_to_db`（按字段 diff 自动 UPDATE），技能没有对应物；而每个 Agent
//! 装机时持一份**独立副本**（`skills.content_path = agents/<agent_id>/skills/<uuid>`，
//! `parent_skill_id` 指向预置 ID）。
//!
//! 后果：改了仓库里的 seed 技能（`default.json` 的 description + `skills/<ID>/skill.md`），
//! 对**存量 Agent 零效果**——它们的副本还在照旧文说，甚至可能正在主动劝退模型用某个工具
//! （2026-09-23「协作沟通」技能实例：技能正文写着 `send_message_to_agent` 不在你的工具面板中，
//! 而工具早已对全 Agent 可达）。
//!
//! # 用法
//!
//! 默认 dry-run，只打印计划；`--apply` 才写盘（写前对每个被覆盖文件留 `.bak.<ts>`）。
//!
//! ```text
//! cargo run -p ai-orz-tools --bin sync_seed_skill -- TEMPLATE_COMMUNICATION
//! cargo run -p ai-orz-tools --bin sync_seed_skill -- TEMPLATE_COMMUNICATION --apply
//! cargo run -p ai-orz-tools --bin sync_seed_skill -- --all            # 全部预置技能
//! ```
//!
//! 等价脚本入口：`make seed-sync SKILL=TEMPLATE_COMMUNICATION`（`APPLY=1` 写盘；不传 `SKILL` 即 `--all`）。
//!
//! # 幂等与回滚
//!
//! 目标内容**全部由仓库 seed 推导**，重复执行结果一致。要回滚：`git checkout` 回旧版 seed
//! 再跑一次 `--apply`（覆盖前留的 `.bak.<ts>` 也可直接换回单文件）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use ai_orz_tools::seed_sync::{self, SeedFileSource, SeedSkillDef};
use rusqlite::Connection;

const USAGE: &str = "\
同步仓库「系统预置技能」到运行期数据目录（默认 dry-run）

用法:
  sync_seed_skill <SKILL_ID>... [--apply] [--data-root <PATH>] [--repo-root <PATH>] [--db-file <NAME>]
  sync_seed_skill --all         [--apply] [--data-root <PATH>] [--repo-root <PATH>] [--db-file <NAME>]

参数:
  <SKILL_ID>...       要同步的预置技能 ID（如 TEMPLATE_COMMUNICATION），可给多个
  --all               同步全部预置技能（default.json 里定义的所有 id）
  --apply             写盘；缺省只预览（dry-run）
  --data-root <PATH>  数据目录（含 ai_orz.db / skills/ / agents/）
                      缺省依次尝试：$AI_ORZ_BASE_PATH → <仓库>/.ai_orz → $HOME/.ai_orz/data
  --repo-root <PATH>  仓库根（含 src/service/domain/system/seed/），缺省当前目录
  --db-file <NAME>    数据库文件名，缺省 ai_orz.db
  -h, --help          显示本帮助
";

// ==================== 路径解析 ====================

fn seed_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("src/service/domain/system/seed")
}

fn resolve_repo_root(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    let root = match explicit {
        Some(p) => p,
        None => std::env::current_dir().map_err(|e| format!("取当前目录失败: {e}"))?,
    };
    if !seed_dir(&root).join("default.json").is_file() {
        return Err(format!(
            "{} 不像 ai_orz 仓库根（缺 src/service/domain/system/seed/default.json）；用 --repo-root 指定",
            root.display()
        ));
    }
    Ok(root)
}

/// 返回 (数据目录, 来源说明)
fn resolve_data_root(
    explicit: Option<PathBuf>,
    repo_root: &Path,
    db_file: &str,
) -> Result<(PathBuf, String), String> {
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();

    if let Some(p) = explicit {
        candidates.push((p, "--data-root".to_string()));
    } else {
        if let Ok(env) = std::env::var("AI_ORZ_BASE_PATH")
            && !env.is_empty()
        {
            candidates.push((PathBuf::from(env), "$AI_ORZ_BASE_PATH".to_string()));
        }
        candidates.push((
            repo_root.join(".ai_orz"),
            "仓库内 .ai_orz（dev 兜底）".to_string(),
        ));
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push((
                PathBuf::from(home).join(".ai_orz/data"),
                "部署根默认 $HOME/.ai_orz/data".to_string(),
            ));
        }
    }

    for (dir, src) in candidates {
        if dir.join(db_file).is_file() {
            return Ok((dir, src));
        }
        if src.starts_with("--data-root") {
            return Err(format!(
                "{} 下没有 {db_file}；--data-root 应指向数据目录（含 ai_orz.db / skills/ / agents/）",
                dir.display()
            ));
        }
    }

    Err(
        "未找到数据目录（候选内均无目标数据库）：用 --data-root 指定，或设 AI_ORZ_BASE_PATH"
            .to_string(),
    )
}

// ==================== seed 定义（引用内容已落盘为文本） ====================

struct SeedSkill {
    id: String,
    name: String,
    description: String,
    tags: Vec<String>,
    files: Vec<SeedFile>,
}

struct SeedFile {
    path: String,
    content: String,
}

fn load_seed_skills(repo_root: &Path) -> Result<Vec<SeedSkill>, String> {
    let dir = seed_dir(repo_root);
    let json_path = dir.join("default.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("读取 {} 失败: {e}", json_path.display()))?;
    let defs: Vec<SeedSkillDef> =
        seed_sync::parse_seed_json(&text).map_err(|e| format!("{}: {e}", json_path.display()))?;

    let mut out = Vec::with_capacity(defs.len());
    for def in defs {
        for s in &def.skipped {
            println!("   ⚠️  跳过 {}::{}", def.id, s);
        }
        let mut files = Vec::with_capacity(def.files.len());
        for f in &def.files {
            let content = match &f.source {
                SeedFileSource::Content(c) => c.clone(),
                SeedFileSource::Ref(rp) => {
                    let p = dir.join(rp);
                    std::fs::read_to_string(&p)
                        .map_err(|e| format!("读取 {} 失败: {e}", p.display()))?
                }
            };
            files.push(SeedFile {
                path: f.path.clone(),
                content,
            });
        }
        out.push(SeedSkill {
            id: def.id,
            name: def.name,
            description: def.description,
            tags: def.tags,
            files,
        });
    }
    Ok(out)
}

// ==================== 计划 ====================

struct TargetRow {
    id: String,
    author_id: String,
    is_seed: bool,
    content_path: String,
    description: String,
    tags: String,
}

struct FilePlan {
    /// 相对数据根的路径，便于阅读
    rel: String,
    abs: PathBuf,
    old: Option<String>,
    new: String,
}

struct RowPlan {
    skill_name: String,
    row: TargetRow,
    new_description: String,
    new_tags: String,
    files: Vec<FilePlan>,
}

impl RowPlan {
    fn desc_changed(&self) -> bool {
        self.row.description != self.new_description
    }

    fn tags_changed(&self) -> bool {
        self.row.tags != self.new_tags
    }

    fn changed(&self) -> bool {
        self.desc_changed()
            || self.tags_changed()
            || self.files.iter().any(|f| f.old.as_deref() != Some(&f.new))
    }
}

fn load_rows(conn: &Connection, skill_id: &str) -> Result<Vec<TargetRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, author_id, author_type, description, tags, content_path \
             FROM skills WHERE id = ?1 OR parent_skill_id = ?1 ORDER BY author_type, id",
        )
        .map_err(|e| format!("准备查询失败: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![skill_id], |r| {
            let author_type: i64 = r.get(2)?;
            Ok(TargetRow {
                id: r.get(0)?,
                author_id: r.get(1)?,
                is_seed: author_type == 0,
                description: r.get(3)?,
                tags: r.get(4)?,
                content_path: r.get(5)?,
            })
        })
        .map_err(|e| format!("查询 skills 失败: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读 skills 行失败: {e}"))?);
    }
    Ok(out)
}

fn build_row_plan(seed: &SeedSkill, row: TargetRow, data_root: &Path) -> RowPlan {
    let files = seed
        .files
        .iter()
        .map(|f| {
            let rel = format!("{}/{}", row.content_path, f.path);
            let abs = data_root.join(&rel);
            let old = std::fs::read_to_string(&abs).ok();
            FilePlan {
                rel,
                abs,
                old,
                new: f.content.clone(),
            }
        })
        .collect();

    RowPlan {
        skill_name: seed.name.clone(),
        row,
        new_description: seed.description.clone(),
        new_tags: seed_sync::tags_to_json(&seed.tags),
        files,
    }
}

fn render(plans: &[RowPlan]) -> String {
    let mut s = String::new();
    for (idx, p) in plans.iter().enumerate() {
        if idx > 0 {
            s.push('\n');
        }
        s.push_str(&format!(
            "-- {}（{}）  {}\n",
            p.row.id,
            p.skill_name,
            if p.row.is_seed {
                "预置行"
            } else {
                "Agent 副本"
            }
        ));
        s.push_str(&format!("   author_id    {}\n", p.row.author_id));
        s.push_str(&format!(
            "   description  {}\n",
            if p.desc_changed() {
                format!(
                    "改  {} → {} 字",
                    p.row.description.chars().count(),
                    p.new_description.chars().count()
                )
            } else {
                format!("不变（{} 字）", p.new_description.chars().count())
            }
        ));
        s.push_str(&format!(
            "   tags         {}\n",
            if p.tags_changed() {
                format!("改  {} → {}", p.row.tags, p.new_tags)
            } else {
                format!("不变（{}）", p.new_tags)
            }
        ));
        for f in &p.files {
            let unchanged = f.old.as_deref() == Some(&f.new);
            s.push_str(&format!(
                "   [{}] {}\n",
                if unchanged { "=" } else { "改" },
                f.rel
            ));
            s.push_str(&format!(
                "       {}\n",
                match &f.old {
                    Some(old) if old != &f.new => {
                        format!("{} → {} 字节", old.len(), f.new.len())
                    }
                    Some(old) => format!("{} 字节", old.len()),
                    None => format!("(磁盘缺文件) → {} 字节", f.new.len()),
                }
            ));
        }
    }
    s
}

// ==================== 写盘 ====================

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct Summary {
    files_written: usize,
    files_backed_up: usize,
    db_rows: usize,
    backups: Vec<PathBuf>,
}

fn apply(plans: &[RowPlan], db_path: &Path) -> Result<Summary, String> {
    let ts = now_secs();
    let mut summary = Summary {
        files_written: 0,
        files_backed_up: 0,
        db_rows: 0,
        backups: Vec::new(),
    };

    // 先落文件（正文型，DAO 每次装载直读磁盘 → 即时生效），
    // 再在一个事务里更新 DB 元数据（description / tags）。
    for p in plans {
        if !p.changed() {
            continue;
        }
        for f in &p.files {
            if f.old.as_deref() == Some(&f.new) {
                continue;
            }
            if let Some(parent) = f.abs.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录 {} 失败: {e}", parent.display()))?;
            }
            if f.old.is_some() {
                let bak = PathBuf::from(format!("{}.bak.{ts}", f.abs.display()));
                std::fs::copy(&f.abs, &bak)
                    .map_err(|e| format!("备份 {} 失败: {e}", f.abs.display()))?;
                summary.files_backed_up += 1;
                summary.backups.push(bak);
            }
            std::fs::write(&f.abs, &f.new)
                .map_err(|e| format!("写入 {} 失败: {e}", f.abs.display()))?;
            summary.files_written += 1;
        }
    }

    let mut conn =
        Connection::open(db_path).map_err(|e| format!("打开 {} 失败: {e}", db_path.display()))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("开启事务失败: {e}"))?;
    let now = now_ms();
    for p in plans {
        if !p.desc_changed() && !p.tags_changed() {
            continue;
        }
        tx.execute(
            "UPDATE skills SET description = ?1, tags = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![p.new_description, p.new_tags, now, p.row.id],
        )
        .map_err(|e| format!("更新 skills({}) 失败: {e}", p.row.id))?;
        summary.db_rows += 1;
    }
    tx.commit().map_err(|e| format!("提交事务失败: {e}"))?;

    Ok(summary)
}

// ==================== 主流程 ====================

fn run(argv: &[String]) -> Result<(), String> {
    let args = seed_sync::parse_args(argv)?;
    let repo_root = resolve_repo_root(args.repo_root.clone())?;
    let (data_root, root_src) =
        resolve_data_root(args.data_root.clone(), &repo_root, &args.db_file)?;
    let db_path = data_root.join(&args.db_file);

    println!("仓库根  : {}", repo_root.display());
    println!("数据目录: {}  （来源：{root_src}）", data_root.display());
    println!("数据库  : {}", db_path.display());
    println!(
        "模式    : {}\n",
        if args.apply {
            "APPLY（写盘）"
        } else {
            "dry-run（不写盘）"
        }
    );

    let seeds = load_seed_skills(&repo_root)?;
    let selected: Vec<&SeedSkill> = if args.all {
        seeds.iter().collect()
    } else {
        let mut v = Vec::with_capacity(args.skill_ids.len());
        for id in &args.skill_ids {
            match seeds.iter().find(|s| &s.id == id) {
                Some(s) => v.push(s),
                None => {
                    return Err(format!(
                        "default.json 里没有技能 {id}；现有：{}",
                        seeds
                            .iter()
                            .map(|s| s.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
        v
    };

    let conn =
        Connection::open(&db_path).map_err(|e| format!("打开 {} 失败: {e}", db_path.display()))?;
    let mut plans = Vec::new();
    let mut missing = Vec::new();
    for seed in &selected {
        let rows = load_rows(&conn, &seed.id)?;
        if rows.is_empty() {
            missing.push(seed.id.clone());
            continue;
        }
        for row in rows {
            plans.push(build_row_plan(seed, row, &data_root));
        }
    }
    drop(conn);

    if !missing.is_empty() {
        let msg = format!(
            "数据目录里没有这些预置技能的行（parent_skill_id 也没有引用）：{}",
            missing.join(", ")
        );
        if args.all {
            println!("⚠️  {msg}\n");
        } else {
            return Err(format!("{msg}；确认 --data-root 指向正确的数据目录"));
        }
    }

    println!("{}", render(&plans));

    let todo: Vec<&RowPlan> = plans.iter().filter(|p| p.changed()).collect();
    let files_todo: usize = todo
        .iter()
        .map(|p| {
            p.files
                .iter()
                .filter(|f| f.old.as_deref() != Some(&f.new))
                .count()
        })
        .sum();
    let db_todo = todo
        .iter()
        .filter(|p| p.desc_changed() || p.tags_changed())
        .count();

    println!("== 汇总 ==");
    println!("   命中行: {}（其中待变更 {}）", plans.len(), todo.len());
    println!("   待写文件: {files_todo} 个；待更新 DB 字段行: {db_todo} 条");

    if !args.apply {
        if todo.is_empty() {
            println!("   ✅ 已是最新，无需写盘");
        } else {
            println!("   ℹ️  dry-run 未写盘；确认后加 --apply（或 APPLY=1 make seed-sync）");
        }
        return Ok(());
    }

    if todo.is_empty() {
        println!("   ✅ 已是最新，未写盘");
        return Ok(());
    }

    let summary = apply(&plans, &db_path)?;
    println!(
        "   ✅ 已写入文件 {} 个（备份 {} 个）；更新 DB 行 {} 条",
        summary.files_written, summary.files_backed_up, summary.db_rows
    );
    for b in &summary.backups {
        println!("      备份: {}", b.display());
    }
    println!("   ℹ️  技能正文每次装载都直读磁盘（DAO 无缓存），本次写入即时生效；服务无需重启");
    Ok(())
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&argv) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("✗ {e}");
            ExitCode::FAILURE
        }
    }
}
