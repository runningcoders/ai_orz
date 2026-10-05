//! CI 门禁：禁止「巨型内联测试模块」—— cargo run -p ai-orz-tools --bin inline_test_lint
//!
//! 背景：Rust 惯例是把测试写成源文件末尾的 `#[cfg(test)] mod tests`。小模块无妨，
//! 但内联测试长到几百行后，业务实现被测试挤到需要滚屏才能读完（本仓曾有
//! `identity_credentials.rs` 2051 行里 999 行是测试、`search_memory.rs` 868 行里
//! 570 行是测试），review 时读业务代码要反复跳过测试噪音。
//!
//! 规则：非 `*_test.rs` 的源文件里，顶层内联测试模块（`#[cfg(test)]` 紧跟
//! `mod xxx {`）超过阈值即 fail。拆分用 `tools/split_inline_tests.py`：
//!
//! ```text
//! python3 tools/split_inline_tests.py src/foo/bar.rs
//! ```
//!
//! 三个需要区分的判定（都踩过）：
//! 1. `#[cfg(test)]` 后面**必须是 `mod xxx {`** 才是测试模块。业务代码里可能有
//!    测试专用辅助函数也带 `#[cfg(test)]`（如 `settle_memory.rs` 的
//!    `short_term_of_mut`），不能算进来。
//! 2. **已拆分形态**（`#[path = "x_tests.rs"] mod tests;`）不参与统计——那正是
//!    本门禁想推动的目标形态，不是违规。
//! 3. 花括号配平时字符串字面量里的 `{`/`}` 会干扰计数，本工具按「行首缩进层级」
//!    近似判定（与拆分脚本同一套假设），对含 `r#"..."#` 原始字符串的文件可能
//!    偏差；偏差只会导致漏报（低估行数），不会误报，因此不阻断。
//!
//! 存量豁免：仓库里已有一批 100~200 行的内联测试模块（低于阈值）。阈值默认
//! 200 行，可用环境变量覆盖；确需临时放行可加 `// inline-test-allow: <行数>` 注释
//! 声明该文件的实际行数（需与实测一致），或把阈值调高。

use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use walkdir::WalkDir;

/// 默认阈值（测试模块跨度行数）。可用 `INLINE_TEST_MAX_LINES` 覆盖。
const DEFAULT_MAX_LINES: usize = 200;

const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    ".workbuddy",
    "docs",
    "frontend",
    ".ai_orz",
];

/// 只扫这些根（与 cargo workspace 一致）
const SCAN_ROOTS: &[&str] = &["src", "common", "tools"];

struct Finding {
    file: PathBuf,
    line_no: usize,
    mod_name: String,
    lines: usize,
    file_total: usize,
    /// 该文件上的 `// inline-test-allow` 声明值（若存在）
    allowed: Option<usize>,
}

impl Finding {
    fn ratio(&self) -> f64 {
        if self.file_total == 0 {
            0.0
        } else {
            self.lines as f64 * 100.0 / self.file_total as f64
        }
    }
}

/// 顶层 `#[cfg(test)]`（不缩进）
fn is_top_level_cfg_test(line: &str) -> bool {
    line.trim() == "#[cfg(test)]" && !line.starts_with(' ') && !line.starts_with('\t')
}

/// 收集单个文件里的内联测试模块
fn scan_file(path: &Path, root: &Path) -> Vec<Finding> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let rel = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    let file_str = rel.to_string_lossy().replace('\\', "/");

    // 已拆分形态：`#[path = "..."] mod xxx;`（无花括号）→ 目标形态，不统计
    let path_mod = Regex::new(r#"^\s*#\[path\s*=\s*"[^"]+"\]\s*$"#).unwrap();
    // 豁免声明
    let allow_re = Regex::new(r"inline-test-allow:\s*(\d+)").unwrap();

    let mut allowed = None;
    for l in &lines {
        if let Some(cap) = allow_re.captures(l) {
            allowed = cap.get(1).and_then(|m| m.as_str().parse().ok());
        }
    }

    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !is_top_level_cfg_test(l) {
            continue;
        }
        // ① 下一行必须是 `mod xxx {` 才算测试模块
        let Some(next) = lines.get(i + 1) else { break };
        let nxt = next.trim();
        let Some(mods) = Regex::new(r"^mod\s+(\w+)\s*\{$").unwrap().captures(nxt) else {
            continue;
        };
        let mod_name = mods.get(1).unwrap().as_str().to_string();

        // ② 已拆分形态排除：`mod xxx;` 无花括号（这里已排除，因为要求 `{` 结尾）
        // 花括号配平找闭合行
        let mut depth = 0i32;
        let mut end = None;
        for (k, l2) in lines.iter().enumerate().skip(i + 1) {
            for ch in l2.chars() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                }
            }
            if depth == 0 && k > i + 1 {
                end = Some(k);
                break;
            }
        }
        let Some(end) = end else { continue };
        // 若闭合后紧跟 `#[path`（不可能，但留个兜底）则跳过
        if lines
            .get(end + 1)
            .is_some_and(|l| path_mod.is_match(l.trim()))
        {
            continue;
        }

        let span = end - i + 1;
        out.push(Finding {
            file: rel.clone(),
            line_no: i + 1,
            mod_name,
            lines: span,
            file_total: total,
            allowed,
        });
        let _ = file_str;
    }
    out
}

fn max_lines() -> usize {
    std::env::var("INLINE_TEST_MAX_LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_LINES)
}

fn main() -> ExitCode {
    let max = max_lines();
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mut all: Vec<Finding> = Vec::new();
    for scan_root in SCAN_ROOTS {
        let base = root.join(scan_root);
        if !base.exists() {
            continue;
        }
        for entry in WalkDir::new(&base).into_iter().filter_entry(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            !SKIP_DIRS.contains(&name.as_str())
        }) {
            let Ok(e) = entry else { continue };
            if !e.file_type().is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".rs") {
                continue;
            }
            // 已拆分出来的测试文件本身不参与统计
            if name.ends_with("_test.rs") || name.ends_with("_tests.rs") {
                continue;
            }
            all.extend(scan_file(e.path(), &root));
        }
    }

    // 超阈值且未走豁免者 = 真正的违规
    let (exempted, real): (Vec<&Finding>, Vec<&Finding>) = all
        .iter()
        .filter(|f| f.lines > max)
        .partition(|f| f.allowed.is_some_and(|a| a >= f.lines));

    // 统计信息（总是打印，便于观察趋势）
    let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
    for f in &all {
        let name = f.file.to_string_lossy().to_string();
        let e = by_file.entry(name).or_insert(0);
        *e = (*e).max(f.lines);
    }
    let mut sorted: Vec<_> = by_file.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    println!(
        "inline_test_lint: 扫 {} 处内联测试模块（跨 {} 个文件），阈值 {} 行",
        all.len(),
        by_file.len(),
        max
    );
    if let Some(top) = sorted.first() {
        println!("  当前最大：{}（{} 行）", top.0, top.1);
    }

    if real.is_empty() {
        if !exempted.is_empty() {
            println!("  其中 {} 处走 inline-test-allow 豁免", exempted.len());
        }
        println!("inline_test_lint OK: 0 violations");
        return ExitCode::SUCCESS;
    }

    eprintln!();
    for f in &real {
        eprintln!(
            "{}:{}: [inline-test] 内联测试模块 `mod {}` 占 {} 行（文件共 {} 行，{}%）超过阈值 {} 行",
            f.file.display(),
            f.line_no,
            f.mod_name,
            f.lines,
            f.file_total,
            f.ratio() as u32,
            max
        );
        eprintln!(
            "    → 拆出：python3 tools/split_inline_tests.py {}",
            f.file.display()
        );
    }
    eprintln!("\ninline_test_lint FAILED: {} violations", real.len());
    eprintln!(
        "    豁免方式：文件内加注释 `// inline-test-allow: <实测行数>`，或调高 INLINE_TEST_MAX_LINES"
    );
    ExitCode::from(1)
}
