//! 一次性迁移：默认 dry-run；--apply 才写盘

use ai_orz_tools::{collect_target_files, migrate_content};
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let apply = std::env::args().any(|a| a == "--apply");
    let files = collect_target_files();
    let mut total = 0usize;
    let mut touched = 0usize;
    for f in &files {
        let Ok(c) = fs::read_to_string(f) else {
            continue;
        };
        let (new, n) = migrate_content(&c);
        if n == 0 {
            continue;
        }
        touched += 1;
        total += n;
        if apply {
            let _ = fs::write(f, &new);
            println!("APPLIED {} ({} replacements)", f.display(), n);
        } else {
            println!("WOULD  {} ({} replacements)", f.display(), n);
        }
    }
    println!(
        "\n{} mode: {} replacements across {} files",
        if apply { "APPLY" } else { "DRY-RUN" },
        total,
        touched
    );
    ExitCode::SUCCESS
}
