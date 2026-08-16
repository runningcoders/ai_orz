use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let base = Path::new("/Users/aman/Technology/rust/ai_orz");

    // ── DESIGN files to ARCHIVE (37 files) ──
    let design_to_archive: Vec<&str> = vec![
        "a2a_server_architecture_design",
        "agent_loop_engine_design",
        "agent_onboarding_design",
        "attachment_storage",
        "browser_e2e_test_design",
        "builtins_http_tool_design",
        "canvas_rendering_playbook",
        "common-error-type",
        "consumer_architecture",
        "entity_list_query_search_design",
        "event_design",
        "external_agent_design",
        "full_entity_fts5_search_design",
        "generic_builtin_tools_design",
        "handler-tool-registration-macro",
        "intent_aware_two_stage_awaken_design",
        "lark_cli_integration",
        "mcp_tool_design",
        "memory_search_enhancement_design",
        "memory_system_enhancement_design",
        "message_channel_design",
        "message_interaction_design",
        "organization_design",
        "project_design",
        "project_management_design",
        "request_context_design",
        "seed-config-migration",
        "skill_design",
        "skill_system_enhancement_design",
        "stats_module_design",
        "stats_query_design",
        "task_design",
        "task_scheduler_design",
        "testing_guidelines",
        "tool_design",
        "unified-idl-http-handler",
        "vector_search_architecture",
    ];

    // ── DESIGN files to KEEP UNCHANGED (8 files) ──
    let design_to_keep: Vec<&str> = vec![
        "sqlx_guide",
        "logging_design",
        "api_protocol_convention",
        "pagination_and_count_convention",
        "runtime_design",
        "thinking_task_policy_engine_design",
        "frontend_architecture",
        "ui_design_system",
    ];

    // ── STRAYS: old docs/archive/ paths that need to move ──
    let strays: Vec<(&str, &str)> = vec![
        ("docs/archive/a2a_server_design", "docs/archive/design-archive/a2a_server_design"),
        ("docs/archive/runtime-domain-roadmap", "docs/archive/design-archive/runtime-domain-roadmap"),
        ("docs/archive/handler_management_api_plan", "docs/archive/plan-archive/handler_management_api_plan"),
        ("docs/archive/test_supplement_plan_20260514", "docs/archive/plan-archive/test_supplement_plan_20260514"),
        ("docs/archive/frontend_roadmap", "docs/archive/plan-archive/frontend_roadmap"),
        ("docs/archive/todo-archive-2026-08-15", "docs/archive/plan-archive/todo-archive-2026-08-15"),
    ];

    // Build the set of kept design file names for quick lookup
    let keep_set: std::collections::HashSet<&str> = design_to_keep.iter().cloned().collect();

    // Build design replacement map
    let mut design_replacements: Vec<(String, String)> = Vec::new();
    for name in &design_to_archive {
        let old = format!("docs/design/{}.md", name);
        let new = format!("docs/archive/design-archive/{}.md", name);
        design_replacements.push((old, new));
    }

    // Build plan replacement map: docs/plan/X.md → docs/archive/plan-archive/X.md
    // Collect all plan files from docs/plan/ directory
    let plan_dir = base.join("docs/plan");
    let mut plan_files: Vec<String> = Vec::new();
    if plan_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&plan_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "md") {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        plan_files.push(name.to_string());
                    }
                }
            }
        }
    }

    let mut plan_replacements: Vec<(String, String)> = Vec::new();
    for pf in &plan_files {
        let old = format!("docs/plan/{}", pf);
        let new = format!("docs/archive/plan-archive/{}", pf);
        plan_replacements.push((old, new));
    }

    // Special case: 2026-08-15-文档规范与仓库精简.md → 文档规范与仓库精简.md
    let special_old = "docs/plan/2026-08-15-文档规范与仓库精简.md".to_string();
    let special_new = "docs/archive/plan-archive/文档规范与仓库精简.md".to_string();
    plan_replacements.push((special_old, special_new));

    // Also handle references without the date prefix
    let special_old2 = "docs/plan/文档规范与仓库精简.md".to_string();
    let special_new2 = "docs/archive/plan-archive/文档规范与仓库精简.md".to_string();
    plan_replacements.push((special_old2, special_new2));

    // Add stray replacements
    let mut stray_replacements: Vec<(String, String)> = Vec::new();
    for (old, new) in &strays {
        stray_replacements.push((old.to_string(), new.to_string()));
    }

    // Also handle the relative path reference docs/plan/../design/thinking.md
    // This is a weird one - resolve it to docs/design/thinking.md first
    // But since thinking.md is not in the archive, just note it
    // Let me check if "thinking" is a kept file... it's not listed. So it should stay.

    // Collect all .md files under docs/wiki/
    let wiki_root = base.join("docs/wiki");
    let mut md_files: Vec<PathBuf> = Vec::new();
    collect_md_files(&wiki_root, &mut md_files);

    println!("Found {} .md files under docs/wiki/", md_files.len());
    println!("Plan file replacements: {}", plan_replacements.len());
    println!("Design file replacements: {}", design_replacements.len());
    println!("Stray replacements: {}", stray_replacements.len());

    // Process each file
    let mut files_modified = 0;
    let mut total_replacements = 0;
    let mut sample_replacements: Vec<(String, String, String, usize)> = Vec::new();

    for filepath in &md_files {
        let mut content = match fs::read_to_string(filepath) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let original = content.clone();
        let mut file_replacements = 0u32;

        // 1. Apply PLAN replacements (specific filenames, longer matches first)
        let mut plan_sorted = plan_replacements.clone();
        plan_sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (old, new) in &plan_sorted {
            let count = content.matches(old).count();
            if count > 0 {
                // Check: is this a kept design file? (shouldn't happen for plan)
                content = content.replace(old, new);
                file_replacements += count as u32;
                if sample_replacements.len() < 15 {
                    let rel = filepath.strip_prefix(base).unwrap_or(filepath).to_string_lossy().to_string();
                    sample_replacements.push((rel, old.clone(), new.clone(), count));
                }
            }
        }

        // 2. Apply DESIGN replacements (37 specific files, longer matches first)
        let mut design_sorted = design_replacements.clone();
        design_sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (old, new) in &design_sorted {
            let count = content.matches(old).count();
            if count > 0 {
                content = content.replace(old, new);
                file_replacements += count as u32;
                if sample_replacements.len() < 15 {
                    let rel = filepath.strip_prefix(base).unwrap_or(filepath).to_string_lossy().to_string();
                    sample_replacements.push((rel, old.clone(), new.clone(), count));
                }
            }
        }

        // 3. Apply STRAY replacements
        for (old, new) in &stray_replacements {
            let count = content.matches(old).count();
            if count > 0 {
                content = content.replace(old, new);
                file_replacements += count as u32;
                if sample_replacements.len() < 15 {
                    let rel = filepath.strip_prefix(base).unwrap_or(filepath).to_string_lossy().to_string();
                    sample_replacements.push((rel, old.clone(), new.clone(), count));
                }
            }
        }

        // Also handle plan references with anchors (e.g., docs/plan/file.md#L10-L20)
        // These are already handled by the exact match above since the replacement
        // is on docs/plan/file.md which matches the prefix of docs/plan/file.md#L10-L20

        // Also handle plan references without .md extension (e.g., docs/plan/file)
        // These would need separate handling, but from the grep output they all have .md

        if content != original {
            fs::write(filepath, &content).unwrap();
            files_modified += 1;
            total_replacements += file_replacements;
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("Files modified: {}", files_modified);
    println!("Total replacements made: {}", total_replacements);

    println!("\nSample replacements (up to 15):");
    for (rel, old, new, count) in &sample_replacements {
        println!("  {}", rel);
        println!("    {} -> {}  (x{})", old, new, count);
    }

    // ── Check for remaining references ──
    println!("\n{}", "=".repeat(60));
    println!("Checking for remaining old-path references...");

    let mut remaining_plan = std::collections::HashSet::new();
    let mut remaining_design = std::collections::HashSet::new();
    let mut remaining_strays = std::collections::HashSet::new();

    for filepath in &md_files {
        let mut content = match fs::read_to_string(filepath) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in content.lines() {
            // Check for docs/plan/ references (excluding already-archived)
            if line.contains("docs/plan/") && !line.contains("docs/archive/plan-archive/") {
                let mut search_from = 0;
                while let Some(start) = line[search_from..].find("docs/plan/") {
                    let abs_start = search_from + start;
                    let rest = &line[abs_start..];
                    let end = rest.find(|c: char| c == ' ' || c == ')' || c == '>' || c == ']' || c == '"' || c == '\'' || c == '`' || c == '|' || c == '（' || c == '\u{3000}').unwrap_or(rest.len());
                    remaining_plan.insert(rest[..end].to_string());
                    search_from = abs_start + 1;
                }
            }

            // Check for docs/design/ references (excluding kept files and archived)
            if line.contains("docs/design/") && !line.contains("docs/archive/design-archive/") {
                let mut search_from = 0;
                while let Some(start) = line[search_from..].find("docs/design/") {
                    let abs_start = search_from + start;
                    let rest = &line[abs_start..];
                    let end = rest.find(|c: char| c == ' ' || c == ')' || c == '>' || c == ']' || c == '"' || c == '\'' || c == '`' || c == '|' || c == '（' || c == '\u{3000}').unwrap_or(rest.len());
                    let m = rest[..end].to_string();
                    // Check if it's a kept design file
                    let is_kept = keep_set.iter().any(|k| m.contains(&format!("docs/design/{}.md", k)) || m.contains(&format!("docs/design/{}", k)));
                    // Also skip placeholder files and non-existent files
                    let is_placeholder = m.contains("占位") || m.contains("*.md") || m.contains("/**") || m.contains("x_design") || m.contains("xxx.");
                    let is_template = m.contains("*_design.md");
                    if !is_kept && !is_placeholder && !is_template {
                        remaining_design.insert(m);
                    }
                    search_from = abs_start + 1;
                }
            }

            // Check for stray references
            for (s, _) in &strays {
                if line.contains(s) && !line.contains("docs/archive/design-archive/") && !line.contains("docs/archive/plan-archive/") {
                    remaining_strays.insert(s.to_string());
                }
            }
        }
    }

    if !remaining_plan.is_empty() {
        println!("\n  Remaining docs/plan/ references ({}):", remaining_plan.len());
        for r in &remaining_plan {
            println!("    {}", r);
        }
    }

    if !remaining_design.is_empty() {
        println!("\n  Remaining docs/design/ references (non-kept, non-placeholder) ({}):", remaining_design.len());
        for r in &remaining_design {
            println!("    {}", r);
        }
    }

    if !remaining_strays.is_empty() {
        println!("\n  Remaining stray references ({}):", remaining_strays.len());
        for r in &remaining_strays {
            println!("    {}", r);
        }
    }

    if remaining_plan.is_empty() && remaining_design.is_empty() && remaining_strays.is_empty() {
        println!("  All old-path references have been replaced!");
    }
}

fn collect_md_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_md_files(&path, files);
            } else if path.extension().map_or(false, |e| e == "md") {
                files.push(path);
            }
        }
    }
}

fn find_all(line: &str, prefix: &str) -> String {
    // Find the path starting at prefix, ending at whitespace or special chars
    if let Some(start) = line.find(prefix) {
        let rest = &line[start..];
        let end = rest.find(|c: char| c == ' ' || c == ')' || c == '>' || c == ']' || c == '"' || c == '\'' || c == '`' || c == '|' || c == '（' || c == '\u{3000}').unwrap_or(rest.len());
        rest[..end].to_string()
    } else {
        String::new()
    }
}
