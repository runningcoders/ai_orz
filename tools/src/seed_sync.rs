//! 预置技能同步（`sync_seed_skill` bin）的纯逻辑层
//!
//! 只放**无副作用、可单测**的部分：命令行解析、`default.json` 解析。
//! 文件系统与 SQLite 的编排留在 `src/bin/sync_seed_skill.rs`，与 `docs_migrate` 同一套分工
//! （纯函数进 lib → 被 `cargo test --lib` 覆盖；bin 只做 I/O）。
//!
//! 背景说明见 bin 头部注释：预置技能只在「新建 Agent / 应用预设」时写库，启动无同步机制，
//! 每个 Agent 装机时持独立副本 ⇒ 改仓库 seed 对存量 Agent 零效果。

use std::path::PathBuf;

// ==================== 命令行 ====================

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub skill_ids: Vec<String>,
    pub all: bool,
    pub apply: bool,
    pub data_root: Option<PathBuf>,
    pub repo_root: Option<PathBuf>,
    pub db_file: String,
}

/// 解析命令行（`argv` 不含程序名）
pub fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args {
        db_file: "ai_orz.db".to_string(),
        ..Args::default()
    };
    let mut i = 0usize;
    while i < argv.len() {
        let cur = argv[i].as_str();
        match cur {
            "--all" => args.all = true,
            "--apply" => args.apply = true,
            "--data-root" => args.data_root = Some(PathBuf::from(take_value(argv, &mut i, cur)?)),
            "--repo-root" => args.repo_root = Some(PathBuf::from(take_value(argv, &mut i, cur)?)),
            "--db-file" => args.db_file = take_value(argv, &mut i, cur)?,
            other if other.starts_with('-') => return Err(format!("未知参数: {other}")),
            other => args.skill_ids.push(other.to_string()),
        }
        i += 1;
    }
    match (args.all, args.skill_ids.is_empty()) {
        (true, false) => return Err("--all 与 <SKILL_ID> 不能同时给".to_string()),
        (false, true) => return Err("未指定技能：给 <SKILL_ID> 或 --all".to_string()),
        _ => {}
    }
    Ok(args)
}

/// 取 `--flag <value>` 的取值，并把游标停在 value 上（外层循环负责再 +1）
fn take_value(argv: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    argv.get(*i)
        .cloned()
        .ok_or_else(|| format!("{flag} 缺少取值"))
}

// ==================== default.json ====================

/// `default.json` 里的技能元信息
///
/// 技能正文不在主文件里：它们在仓库的 `skills/<ID>/` 目录下，由调用方按 id 定位后
/// 整目录读取（编译期 `include_str!`/`include_bytes!` 内嵌，运行时打成技能包）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedSkillDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// 解析 `seed/default.json` 的 `skills` 数组（只要元信息）
pub fn parse_seed_json(text: &str) -> Result<Vec<SeedSkillDef>, String> {
    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("解析 default.json 失败: {e}"))?;
    let arr = root
        .get("skills")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "default.json 缺少 skills 数组".to_string())?;

    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        out.push(SeedSkillDef {
            id: item
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "default.json 里有 skill 缺 id 字段".to_string())?
                .to_string(),
            name: item
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: item
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tags: item
                .get("tags")
                .and_then(serde_json::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    Ok(out)
}

/// DB 的 `tags` 列是紧凑 JSON 数组（`["neural","messaging"]`），与 seed 定义一致
pub fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parse_single_skill() {
        let a = parse_args(&v(&["TEMPLATE_COMMUNICATION"])).unwrap();
        assert_eq!(a.skill_ids, vec!["TEMPLATE_COMMUNICATION"]);
        assert!(!a.all);
        assert!(!a.apply);
        assert_eq!(a.db_file, "ai_orz.db");
    }

    #[test]
    fn parse_all_with_apply_and_roots() {
        let a = parse_args(&v(&[
            "--all",
            "--apply",
            "--data-root",
            "/tmp/d",
            "--repo-root",
            "/tmp/r",
            "--db-file",
            "x.db",
        ]))
        .unwrap();
        assert!(a.all && a.apply);
        assert_eq!(a.data_root, Some(PathBuf::from("/tmp/d")));
        assert_eq!(a.repo_root, Some(PathBuf::from("/tmp/r")));
        assert_eq!(a.db_file, "x.db");
    }

    #[test]
    fn parse_rejects_all_with_ids() {
        let e = parse_args(&v(&["--all", "TEMPLATE_COMMUNICATION"])).unwrap_err();
        assert!(e.contains("不能同时给"), "实际: {e}");
    }

    #[test]
    fn parse_rejects_empty_selection() {
        let e = parse_args(&v(&["--apply"])).unwrap_err();
        assert!(e.contains("未指定技能"), "实际: {e}");
    }

    #[test]
    fn parse_rejects_unknown_flag_and_missing_value() {
        assert!(
            parse_args(&v(&["--nope"]))
                .unwrap_err()
                .contains("未知参数")
        );
        assert!(
            parse_args(&v(&["--data-root"]))
                .unwrap_err()
                .contains("缺少取值")
        );
    }

    #[test]
    fn tags_serialized_compactly_like_db() {
        let tags = vec!["neural".to_string(), "messaging".to_string()];
        assert_eq!(tags_to_json(&tags), r#"["neural","messaging"]"#);
    }

    #[test]
    fn parse_seed_json_reads_skill_metadata() {
        let json = r#"{
          "skills": [
            {
              "id": "TEMPLATE_X",
              "name": "示例",
              "description": "说明",
              "tags": ["neural"],
              "package": "skills/TEMPLATE_X.zip"
            }
          ]
        }"#;
        let defs = parse_seed_json(json).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].id, "TEMPLATE_X");
        assert_eq!(defs[0].name, "示例");
        assert_eq!(defs[0].description, "说明");
        assert_eq!(defs[0].tags, vec!["neural"]);
    }

    #[test]
    fn parse_seed_json_rejects_missing_skills_or_id() {
        assert!(
            parse_seed_json(r#"{"agents":[]}"#)
                .unwrap_err()
                .contains("缺少 skills 数组")
        );
        assert!(
            parse_seed_json(r#"{"skills":[{"name":"no id"}]}"#)
                .unwrap_err()
                .contains("缺 id")
        );
    }
}
