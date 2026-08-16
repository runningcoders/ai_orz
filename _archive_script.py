#!/usr/bin/env python3
"""Archive docs/plan/* and docs/design/* per AGENTS §2.1 rules.

Strips: REQUIRED SUB-SKILL markers, - [ ] checkboxes, cargo test/build commands,
implementation snapshot code blocks (test/function bodies).
Adds Template C tombstone header.
"""

import os
import re
import shutil
from datetime import datetime
from pathlib import Path

BASE = Path("/Users/aman/Technology/rust/ai_orz")
TODAY = datetime.now().strftime("%Y-%m-%d")

KEEP_DESIGN = {
    "sqlx_guide.md",
    "logging_design.md",
    "api_protocol_convention.md",
    "pagination_and_count_convention.md",
    "runtime_design.md",
    "thinking_task_policy_engine_design.md",
    "frontend_architecture.md",
    "ui_design_system.md",
}

DESIGN_STRAYS = {
    "a2a_server_design.md": "design-archive",
    "runtime-domain-roadmap.md": "design-archive",
}

PLAN_STRAYS = {
    "handler_management_api_plan.md": "plan-archive",
    "test_supplement_plan_20260514.md": "plan-archive",
    "frontend_roadmap.md": "plan-archive",
    "todo-archive-2026-08-15.md": "plan-archive",
}


def strip_content(content: str) -> str:
    """Strip checkbox/snapshot code blocks and old tombstones per AGENTS §2.1 table."""
    lines = content.split("\n")
    result = []
    in_code_block = False
    code_block_lines = []
    in_old_tombstone = False
    tombstone_done = False

    i = 0
    while i < len(lines):
        line = lines[i]

        if not tombstone_done and (line.strip().startswith("> 📦") or line.strip().startswith("> 📦 **")):
            in_old_tombstone = True
            tombstone_done = True
            i += 1
            continue

        if in_old_tombstone:
            if line.strip().startswith("> ") or line.strip() == "":
                i += 1
                continue
            else:
                in_old_tombstone = False

        if line.strip().startswith("```"):
            if not in_code_block:
                in_code_block = True
                code_block_lines = [line]
                i += 1
                continue
            else:
                in_code_block = False
                code_block_lines.append(line)
                block_content = "\n".join(code_block_lines)
                lang = code_block_lines[0].strip().lstrip("`").strip()

                if _is_snapshot_code_block(block_content, lang):
                    replacement = _snapshot_replacement(block_content, lang)
                    result.append(replacement)
                    code_block_lines = []
                    i += 1
                    continue
                else:
                    result.extend(code_block_lines)
                    code_block_lines = []
                    i += 1
                    continue

        if in_code_block:
            code_block_lines.append(line)
            i += 1
            continue

        if _is_checkbox_line(line):
            i += 1
            continue

        if _is_required_skill_line(line):
            i += 1
            continue

        if _is_cargo_command_line(line):
            i += 1
            continue

        result.append(line)
        i += 1

    return "\n".join(result)


def _is_checkbox_line(line: str) -> bool:
    stripped = line.strip()
    return bool(re.match(r'^- \[[ xX]\]', stripped))


def _is_required_skill_line(line: str) -> bool:
    return "REQUIRED SUB-SKILL" in line and "For agentic workers" in line


def _is_cargo_command_line(line: str) -> bool:
    stripped = line.strip()
    if re.match(r'^-\s+.*cargo (test|build)', stripped):
        return True
    if stripped.startswith('cargo test') or stripped.startswith('cargo build'):
        return True
    return False


def _is_snapshot_code_block(block_content: str, lang: str) -> bool:
    if lang in ("rust", ""):
        if re.search(r'#\[test\]|#\[tokio::test\]|fn test_', block_content):
            return True
        if re.search(r'#\[ignore\]', block_content):
            return True
        if re.search(r'fn\s+\w+\s*\([^)]*\)\s*\{[^}]*\{', block_content) and len(block_content) > 50:
            brace_count = block_content.count('{')
            if brace_count >= 3:
                return True
    if lang == "bash" or lang == "shell":
        if re.search(r'cargo (test|build|run)', block_content):
            return True
    if lang == "toml":
        return False
    if lang in ("", "text", "markdown", "md", "yaml", "yml", "json"):
        return False
    return False


def _snapshot_replacement(block_content: str, lang: str) -> str:
    if "test" in block_content.lower() or "#[test]" in block_content:
        first_fn = re.search(r'fn\s+(test_\w+)', block_content)
        if first_fn:
            return f"> 测试代码已精简，见源码中对应 `{first_fn.group(1)}` 函数"
        return "> 测试代码已精简，见源码对应测试文件"
    if "cargo" in block_content:
        return "> 命令快照已精简，见源码/CI 配置"
    return "> 实现快照已精简，见源码对应实现"


def add_tombstone(content: str, source_name: str, new_path: str) -> str:
    """Add Template C tombstone header."""
    tombstone = f"> 📦 归档标记（{TODAY}）：归档冻结。保留原因：{source_name} 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。\n"
    if content.startswith("# "):
        newline_idx = content.index("\n")
        rest = content[newline_idx + 1:]
        return content[:newline_idx + 1] + "\n" + tombstone + rest
    return tombstone + "\n" + content


def process_file(src: Path, dst: Path, source_type: str) -> dict:
    """Process a single file: read → strip → add tombstone → write → delete old."""
    original = src.read_text(encoding="utf-8")
    stripped = strip_content(original)
    source_name = src.stem
    final = add_tombstone(stripped, source_name, str(dst))

    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(final, encoding="utf-8")
    src.unlink()

    return {
        "from": str(src.relative_to(BASE)),
        "to": str(dst.relative_to(BASE)),
        "lines_before": len(original.split("\n")),
        "lines_after": len(final.split("\n")),
    }


def main():
    base = BASE
    archive_dir = base / "docs" / "archive"
    plan_dir = base / "docs" / "plan"
    design_dir = base / "docs" / "design"

    plan_archive = archive_dir / "plan-archive"
    design_archive = archive_dir / "design-archive"

    plan_archive.mkdir(parents=True, exist_ok=True)
    design_archive.mkdir(parents=True, exist_ok=True)

    moved = []

    # --- 1. Process docs/plan/* ---
    plan_files = sorted(plan_dir.glob("*.md"))
    for src in plan_files:
        if src.name.startswith("2026-08-15-"):
            new_name = src.name[len("2026-08-15-"):]
        else:
            new_name = src.name
        dst = plan_archive / new_name
        info = process_file(src, dst, "plan")
        info["category"] = "plan→plan-archive"
        moved.append(info)
        print(f"  [plan] {src.name} → {new_name}")

    # --- 2. Process docs/design/* (except keep list) ---
    design_files = sorted(design_dir.glob("*.md"))
    for src in design_files:
        if src.name in KEEP_DESIGN:
            print(f"  [KEEP] {src.name}")
            continue
        dst = design_archive / src.name
        info = process_file(src, dst, "design")
        info["category"] = "design→design-archive"
        moved.append(info)
        print(f"  [design] {src.name}")

    # --- 3. Move root-level archive strays ---
    for fname, target_sub in {**DESIGN_STRAYS, **PLAN_STRAYS}.items():
        src = archive_dir / fname
        if not src.exists():
            print(f"  [SKIP] {fname} not found in archive root")
            continue
        target_dir = archive_dir / target_sub
        dst = target_dir / fname
        target_dir.mkdir(parents=True, exist_ok=True)
        info = process_file(src, dst, "stray")
        info["category"] = f"stray→{target_sub}"
        moved.append(info)
        print(f"  [stray] {fname} → {target_sub}/{fname}")

    # --- 4. Verify ---
    remaining_archive_root = [
        f.name for f in archive_dir.iterdir()
        if f.is_file() and not f.name.startswith(".")
    ]
    remaining_plan = [
        f.name for f in plan_dir.iterdir()
        if f.is_file() and f.suffix == ".md" and not f.name.startswith(".")
    ]

    print("\n=== VERIFICATION ===")
    if remaining_archive_root:
        print(f"  ⚠️  docs/archive/ 根目录残留文件: {remaining_archive_root}")
    else:
        print(f"  ✅  docs/archive/ 根目录已清空")

    if remaining_plan:
        print(f"  ⚠️  docs/plan/ 残留文件: {remaining_plan}")
    else:
        print(f"  ✅  docs/plan/ 已清空")

    # --- 5. Summary ---
    print(f"\n=== SUMMARY ===")
    print(f"  Total files moved: {len(moved)}")
    print(f"  Plan files → plan-archive: {sum(1 for m in moved if m['category'].startswith('plan'))}")
    print(f"  Design files → design-archive: {sum(1 for m in moved if m['category'].startswith('design'))}")
    print(f"  Archive strays → sub-dirs: {sum(1 for m in moved if m['category'].startswith('stray'))}")

    print("\n  Full manifest:")
    for m in moved:
        print(f"    [{m['category']}] {m['from']} → {m['to']} ({m['lines_before']}→{m['lines_after']} lines)")


if __name__ == "__main__":
    main()