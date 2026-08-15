//! AI Orz 文档工具集共享库
//!
//! 未来租户规划：wiki-dedup-check（AGENTS §2.1.3 Step 0 五级判定机器预检）、
//! cite-graph-check（四类互引闭环校验）。本 crate 依赖永不进生产二进制。

use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::WalkDir;

/// lint 扫描目标：AGENTS.md + docs/**/*.md + .trae/skills/**/*.md
pub fn collect_target_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    if Path::new("AGENTS.md").exists() {
        files.push(PathBuf::from("AGENTS.md"));
    }
    for dir in ["docs", ".trae/skills"] {
        for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|e| e == "md") {
                files.push(p.to_path_buf());
            }
        }
    }
    files
}

pub struct Violation {
    pub file: PathBuf,
    pub line_no: usize,
    pub rule: &'static str,
    pub snippet: String,
    pub help: &'static str,
}

/// 计算行内 code span 的字节区间（含反引号本身）。奇数个反引号 → None（无法配对，保守不剥离）
fn inline_code_ranges(line: &str) -> Option<Vec<(usize, usize)>> {
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in line.char_indices() {
        if ch == '`' {
            match start {
                None => start = Some(i),
                Some(s) => {
                    ranges.push((s, i + 1));
                    start = None;
                }
            }
        }
    }
    if start.is_some() { None } else { Some(ranges) }
}

/// 剥离行内 code span：把成对反引号片段替换为占位符 `\u{0}N\u{0}`，返回 (处理后行, 被剥离片段)。
/// 奇数个反引号时整行原样返回（保守不剥离）。
/// 注：成功剥离后结果必不含反引号，调用方可用 `stripped.contains('`')` 识别保守路径。
fn strip_inline_code(line: &str) -> (String, Vec<String>) {
    let Some(ranges) = inline_code_ranges(line) else {
        return (line.to_string(), Vec::new());
    };
    if ranges.is_empty() {
        return (line.to_string(), Vec::new());
    }
    let mut out = String::with_capacity(line.len());
    let mut removed = Vec::with_capacity(ranges.len());
    let mut pos = 0usize;
    for (s, e) in ranges {
        out.push_str(&line[pos..s]);
        out.push_str(&format!("\u{0}{}\u{0}", removed.len()));
        removed.push(line[s..e].to_string());
        pos = e;
    }
    out.push_str(&line[pos..]);
    (out, removed)
}

/// 回填 code span：把占位符按序还原为被剥离片段
fn restore_inline_code(line: &str, removed: &[String]) -> String {
    if removed.is_empty() {
        return line.to_string();
    }
    let re = Regex::new(r"\u{0}(\d+)\u{0}").unwrap();
    re.replace_all(line, |c: &regex::Captures| {
        c[1].parse::<usize>()
            .ok()
            .and_then(|i| removed.get(i))
            .map(|s| s.as_str())
            .unwrap_or_else(|| c.get(0).map(|m| m.as_str()).unwrap_or_default())
            .to_string()
    })
    .into_owned()
}

/// 在原始行上找第一个位于 code span 之外的匹配（snippet 定位用；奇数反引号行不会走到这里）
fn find_outside_code<'a>(line: &'a str, re: &Regex) -> Option<regex::Match<'a>> {
    let ranges = inline_code_ranges(line).unwrap_or_default();
    re.find_iter(line)
        .find(|m| !ranges.iter().any(|&(s, e)| m.start() >= s && m.end() <= e))
}

/// lint 单个 md 内容（纯函数，跳过代码围栏 / ❌ 示例行；行内 code span 剥离后检查）
pub fn lint_content(path: &Path, content: &str) -> Vec<Violation> {
    let re_file = Regex::new(r"file://").unwrap();
    let re_legacy_colon = Regex::new(r"\]\([^)\s]*?\.(rs|sql|toml|sh):\d+(-\d+)?\)").unwrap();
    let re_legacy_colon_l = Regex::new(r"\]\([^)\s]*?\.(rs|sql|toml|sh):L\d+(-L\d+)?\)").unwrap();
    let mut out = Vec::new();
    let mut in_fence = false;
    for (i, line) in content.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue; // 跳过代码围栏
        }
        if t.contains('❌') {
            continue; // 跳过红线示例行
        }
        let (stripped, _) = strip_inline_code(line);
        if stripped.contains('`') {
            continue; // 奇数反引号无法配对，保守跳过整行
        }
        if re_file.is_match(&stripped) {
            // 命中必在 code span 之外；行号与 snippet 用原行定位（保留 code span 原文便于人工查看）
            if let Some(m) = find_outside_code(line, &re_file) {
                let after = &line[m.end()..];
                let is_abs = after.starts_with('/')
                    || after.starts_with("Users/")
                    || after.starts_with("home/");
                out.push(Violation {
                    file: path.to_path_buf(),
                    line_no: i + 1,
                    rule: if is_abs {
                        "R1_abs_path"
                    } else {
                        "R2_file_protocol"
                    },
                    snippet: snippet_of(line, m.start(), m.end()),
                    help: "改写为相对仓库根路径，见 AGENTS §2.1.2",
                });
            }
        }
        for (re, rule) in [
            (&re_legacy_colon, "R3_legacy_colon_lines"),
            (&re_legacy_colon_l, "R3_legacy_colon_lines"),
        ] {
            if re.is_match(&stripped)
                && let Some(m) = find_outside_code(line, re)
            {
                out.push(Violation {
                    file: path.to_path_buf(),
                    line_no: i + 1,
                    rule,
                    snippet: snippet_of(line, m.start(), m.end()),
                    help: "行号应写 #Lx-Ly fragment 而非 :x-y，见 AGENTS §2.1.2",
                });
            }
        }
    }
    out
}

/// 截取违规点前后约 15 字节的上下文片段（对齐 UTF-8 字符边界，避免切坏中文）
fn snippet_of(line: &str, s: usize, e: usize) -> String {
    let s = floor_char_boundary(line, s.saturating_sub(15));
    let e = ceil_char_boundary(line, (e + 15).min(line.len()));
    line[s..e].to_string()
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// 迁移：单文件内容改写（纯函数；返回 (新内容, 替换次数)）
///
/// 逐行处理，跳过规则与 [lint_content] 一致（代码围栏 / ❌ 示例行 / 奇数反引号行）；
/// 成对反引号行先剥离 code span 再做替换，替换后回填，行内代码永不改写。
/// 三条替换均为行内匹配不跨行，逐行处理安全。
///
/// 注：regex crate 不支持 lookahead，任务语义用等价实现：
/// - 排除 http 外链（`(?!https?://)`）→ 闭包内判 `path.contains("://")` 原样返回；
/// - 行尾断言（`(?=["'\s]|$)`）→ 捕获结尾界定符 `(["'\s]|$)` 并在替换尾部回填。
pub fn migrate_content(content: &str) -> (String, usize) {
    let re_abs = Regex::new(r#"file:///[^\s)"]*?/ai_orz/"#).unwrap();
    let re_pseudo = Regex::new(r"file://").unwrap();
    // R3：markdown 链接形态 `](path:L75-L137)` → `](path#L75-L137)`
    let re_colon_link =
        Regex::new(r"\]\(([^)\s]*?\.(rs|sql|toml|sh)):L?(\d+)(?:-L?(\d+))?\)").unwrap();
    // R4：裸引用形态 `path:L75-L137`（YAML source_files / 正文裸引用，行尾或后跟空白/引号）。
    // `](` 链接形态后跟 `)` 不满足结尾界定符，天然不会命中本规则。
    let re_colon_bare =
        Regex::new(r#"(\S+\.(?:rs|sql|toml|sh)):L?(\d+)(?:-L?(\d+))?(["'\s]|$)"#).unwrap();
    let mut count = 0usize;
    let mut out = String::with_capacity(content.len());
    let mut in_fence = false;
    for raw in content.split_inclusive('\n') {
        let (line, nl) = match raw.strip_suffix('\n') {
            Some(b) => (b, "\n"),
            None => (raw, ""),
        };
        let t = line.trim_start();
        if t.starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            out.push_str(nl);
            continue;
        }
        if in_fence || t.contains('❌') {
            out.push_str(line);
            out.push_str(nl);
            continue;
        }
        let (stripped, removed) = strip_inline_code(line);
        if stripped.contains('`') {
            // 奇数反引号无法配对，保守不剥离不替换
            out.push_str(line);
            out.push_str(nl);
            continue;
        }
        let s1 = re_abs
            .replace_all(&stripped, |_: &regex::Captures| {
                count += 1;
                ""
            })
            .into_owned();
        let s2 = re_pseudo
            .replace_all(&s1, |_: &regex::Captures| {
                count += 1;
                ""
            })
            .into_owned();
        let s3 = re_colon_link
            .replace_all(&s2, |c: &regex::Captures| {
                let path = &c[1];
                if path.contains("://") {
                    return c[0].to_string(); // http 等外链不动
                }
                count += 1;
                let a: u32 = c[3].parse().unwrap();
                let b: u32 = c.get(4).map(|m| m.as_str().parse().unwrap()).unwrap_or(a);
                let frag = if a == b {
                    format!("#L{a}")
                } else {
                    format!("#L{a}-L{b}")
                };
                format!("]({path}{frag})")
            })
            .into_owned();
        let s4 = re_colon_bare
            .replace_all(&s3, |c: &regex::Captures| {
                let path = &c[1];
                if path.contains("://") {
                    return c[0].to_string(); // 外链（含 URL 中途起匹配的残缺形态）不动
                }
                count += 1;
                let a: u32 = c[2].parse().unwrap();
                let b: u32 = c.get(3).map(|m| m.as_str().parse().unwrap()).unwrap_or(a);
                let frag = if a == b {
                    format!("#L{a}")
                } else {
                    format!("#L{a}-L{b}")
                };
                let tail = c.get(4).map(|m| m.as_str()).unwrap_or("");
                format!("{path}{frag}{tail}")
            })
            .into_owned();
        if s4 == stripped {
            // 无任何替换 → 直接用原行，避免占位符往返造成意外扰动
            out.push_str(line);
        } else {
            out.push_str(&restore_inline_code(&s4, &removed));
        }
        out.push_str(nl);
    }
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_catches_abs_and_pseudo() {
        let v = lint_content(
            Path::new("t.md"),
            "- [a](file:///Users/x/ai_orz/src/a.rs)\n- [b](file://src/b.rs)\n",
        );
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].rule, "R1_abs_path");
        assert_eq!(v[1].rule, "R2_file_protocol");
    }

    #[test]
    fn lint_skips_fence_and_emoji() {
        let v = lint_content(
            Path::new("t.md"),
            "```\nfile://x\n```\n- ❌ bad: file:///Users/x\n",
        );
        assert!(v.is_empty());
    }

    #[test]
    fn lint_backtick_line_only_outside_code_reported() {
        // 反引号内 file:// 不报；反引号外 file:// 报
        let v = lint_content(
            Path::new("t.md"),
            "- ok `code file://src/a.rs` but [b](file://src/b.rs) bad\n",
        );
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "R2_file_protocol");
        assert_eq!(v[0].line_no, 1);
        // snippet 取自原行（保留 code span 原文，便于人工定位）
        assert!(v[0].snippet.contains("file://src/b.rs"));
    }

    #[test]
    fn lint_odd_backtick_line_skipped() {
        // 奇数反引号无法配对 code span，保守跳过整行
        let v = lint_content(Path::new("t.md"), "- odd `file://src/a.rs\n");
        assert!(v.is_empty());
    }

    #[test]
    fn lint_catches_legacy_colon() {
        let v = lint_content(Path::new("t.md"), "- [a](src/a.rs:15-42)\n");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "R3_legacy_colon_lines");
    }

    #[test]
    fn lint_ignores_github_external_fragment() {
        // 合法 GitHub 外链的 #L15 不该被报
        let v = lint_content(
            Path::new("t.md"),
            "- [a](https://github.com/o/r/blob/main/src/a.rs#L15-L42)\n",
        );
        assert!(v.is_empty());
    }

    #[test]
    fn migrate_full_chain() {
        let (out, n) = migrate_content(
            "- [a](file:///Users/x/rust/ai_orz/src/a.rs#L1-L9)\n- [b](file://src/b.rs:15-42)\n",
        );
        // 三次替换：R1 绝对前缀剥离 + R2 伪协议剥离 + R3 冒号行号转 fragment
        assert_eq!(n, 3);
        assert!(out.contains("](src/a.rs#L1-L9)"));
        assert!(out.contains("](src/b.rs#L15-L42)"));
        // 行级处理不吞行尾换行
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn migrate_skips_fence_and_emoji_protects_code_span() {
        let (out, n) = migrate_content(
            "```\nfile://src/a.rs\n```\n- ❌ bad: file://src/b.rs\n- ok `file://src/c.rs`\n- real file://src/d.rs\n",
        );
        // 围栏内 / ❌ 行原样保留；code span 内 file:// 不迁移（该行无替换 → 原行保留）；
        // 仅最后一行 code span 之外正常替换
        assert_eq!(n, 1);
        assert!(out.contains("file://src/a.rs"));
        assert!(out.contains("file://src/b.rs"));
        assert!(out.contains("file://src/c.rs"));
        assert!(out.contains("- real src/d.rs"));
    }

    #[test]
    fn migrate_mixed_table_line_link_migrated_code_kept() {
        // 知识卡「关键文件表」典型混排行：链接带 file:/// 绝对前缀 + 行内代码
        let input = "| [common/src/enums/task.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/enums/task.rs) | 枚举类型安全样例 | `#[repr(i32)]` + `#[derive(sqlx::Type)]` |\n";
        let (out, n) = migrate_content(input);
        // 链接部分被迁移（R1 绝对前缀剥离），行内代码原样保留
        assert_eq!(n, 1);
        assert!(out.contains("](common/src/enums/task.rs)"));
        assert!(out.contains("`#[repr(i32)]` + `#[derive(sqlx::Type)]`"));
        assert!(!out.contains("file://"));
    }

    #[test]
    fn migrate_odd_backtick_line_untouched() {
        // 奇数反引号无法配对 code span，保守不剥离不替换
        let (out, n) = migrate_content("- odd `file://src/a.rs\n");
        assert_eq!(n, 0);
        assert!(out.contains("`file://src/a.rs"));
    }

    #[test]
    fn migrate_bare_yaml_line_suffix() {
        let (out, n) = migrate_content(
            "  - file://src/service/dao/project/sqlite.rs:L75-L137\nref src/x.rs:75 here\n",
        );
        // 伪协议剥离 + 裸行号转 fragment（范围形态与单行形态各 1 次）
        assert_eq!(n, 3);
        assert!(out.contains("  - src/service/dao/project/sqlite.rs#L75-L137"));
        assert!(out.contains("ref src/x.rs#L75 here"));
    }

    #[test]
    fn migrate_ignores_http_colon_lines() {
        let (out, n) = migrate_content(
            "- [a](https://example.com/a.rs:15-42)\nsee https://example.com/b.rs:15-42 end\n",
        );
        // http 外链（链接形态与裸 URL 形态）均不改写
        assert_eq!(n, 0);
        assert!(out.contains("](https://example.com/a.rs:15-42)"));
        assert!(out.contains("https://example.com/b.rs:15-42"));
    }
}
