//! CI 门禁：cargo run -p ai-orz-tools --bin docs_lint

use ai_orz_tools::{collect_target_files, lint_content};
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let files = collect_target_files();
    let mut total = 0usize;
    for f in &files {
        if let Ok(c) = fs::read_to_string(f) {
            for v in lint_content(f, &c) {
                eprintln!(
                    "{}:{}: [{}] ...{}... | {}",
                    v.file.display(),
                    v.line_no,
                    v.rule,
                    v.snippet,
                    v.help
                );
                total += 1;
            }
        }
    }
    if total > 0 {
        eprintln!(
            "\ndocs_lint FAILED: {total} violations in {} files",
            files.len()
        );
        ExitCode::from(1)
    } else {
        println!("docs_lint OK: {} files, 0 violations", files.len());
        ExitCode::SUCCESS
    }
}
