import os
import re

base = "/Users/aman/Technology/rust/ai_orz"

# ── DESIGN files to ARCHIVE (37 files) ──
design_to_archive = [
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
]

# ── DESIGN files to KEEP UNCHANGED (8 files) ──
design_to_keep = [
    "sqlx_guide",
    "logging_design",
    "api_protocol_convention",
    "pagination_and_count_convention",
    "runtime_design",
    "thinking_task_policy_engine_design",
    "frontend_architecture",
    "ui_design_system",
]

# ── STRAYS ──
strays = {
    "docs/archive/a2a_server_design": "docs/archive/design-archive/a2a_server_design",
    "docs/archive/runtime-domain-roadmap": "docs/archive/design-archive/runtime-domain-roadmap",
    "docs/archive/handler_management_api_plan": "docs/archive/plan-archive/handler_management_api_plan",
    "docs/archive/test_supplement_plan_20260514": "docs/archive/plan-archive/test_supplement_plan_20260514",
    "docs/archive/frontend_roadmap": "docs/archive/plan-archive/frontend_roadmap",
    "docs/archive/todo-archive-2026-08-15": "docs/archive/plan-archive/todo-archive-2026-08-15",
}

# ── PLAN special case ──
plan_special = {
    "docs/plan/2026-08-15-文档规范与仓库精简.md": "docs/archive/plan-archive/文档规范与仓库精简.md",
}

# Build design replacement map (only for files being archived, NOT for kept ones)
design_map = {}
for name in design_to_archive:
    old = f"docs/design/{name}.md"
    new = f"docs/archive/design-archive/{name}.md"
    design_map[old] = new

# Build plan replacement map: docs/plan/X.md → docs/archive/plan-archive/X.md
# We'll handle this as a blanket prefix replacement after the special case
# But first we need to know which plan files exist
plan_dir = os.path.join(base, "docs/plan")
plan_files = []
if os.path.isdir(plan_dir):
    for f in os.listdir(plan_dir):
        if f.endswith(".md"):
            plan_files.append(f)

# Build plan map
plan_map = {}
for pf in plan_files:
    old = f"docs/plan/{pf}"
    new = f"docs/archive/plan-archive/{pf}"
    plan_map[old] = new

# Apply special case (overrides the generic mapping)
for old, new in plan_special.items():
    plan_map[old] = new

# Build all replacements (ordered: specials first, then generics)
all_replacements = []

# 1. Plan replacements (specific filenames)
for old, new in sorted(plan_map.items(), key=lambda x: -len(x[0])):
    all_replacements.append((old, new))

# 2. Design replacements (37 specific files)
for old, new in sorted(design_map.items(), key=lambda x: -len(x[0])):
    all_replacements.append((old, new))

# 3. Strays
for old, new in sorted(strays.items(), key=lambda x: -len(x[0])):
    all_replacements.append((old, new))

# Also check: are there any references to docs/design/ for kept files
# that also have variants (like no .md extension, or with anchors)?
# We should NOT change kept design files.

# Collect all .md files under docs/wiki/
wiki_dirs = [
    os.path.join(base, "docs/wiki"),
    os.path.join(base, "docs/wiki/knowledge"),
]

md_files = []
for d in wiki_dirs:
    if os.path.isdir(d):
        for root, dirs, files in os.walk(d):
            for f in files:
                if f.endswith(".md"):
                    md_files.append(os.path.join(root, f))

print(f"Found {len(md_files)} .md files under docs/wiki/")
print(f"Plan replacements: {len(plan_map)}")
print(f"Design replacements: {len(design_map)}")
print(f"Stray replacements: {len(strays)}")
print()

# Process each file
files_modified = 0
total_replacements = 0
sample_replacements = []

for filepath in md_files:
    with open(filepath, "r", encoding="utf-8") as f:
        content = f.read()

    original = content
    file_replacements = 0

    # Apply replacements
    for old, new in all_replacements:
        count = content.count(old)
        if count > 0:
            content = content.replace(old, new)
            file_replacements += count
            if len(sample_replacements) < 10:
                rel = os.path.relpath(filepath, base)
                sample_replacements.append((rel, old, new, count))

    if content != original:
        with open(filepath, "w", encoding="utf-8") as f:
            f.write(content)
        files_modified += 1
        total_replacements += file_replacements

print(f"\n{'='*60}")
print(f"Files modified: {files_modified}")
print(f"Total replacements made: {total_replacements}")
print(f"\nSample replacements:")
for rel, old, new, count in sample_replacements:
    print(f"  {rel}")
    print(f"    {old} → {new}  (×{count})")

# Also check for any remaining docs/plan/ or docs/design/ references
print(f"\n{'='*60}")
print("Checking for remaining old-path references...")

remaining_plan = set()
remaining_design = set()
remaining_strays = set()

for filepath in md_files:
    with open(filepath, "r", encoding="utf-8") as f:
        content = f.read()

    for line in content.split("\n"):
        # Check for docs/plan/ references (excluding already-archived ones)
        if "docs/plan/" in line and "docs/archive/plan-archive/" not in line:
            # Extract the path reference
            for m in re.finditer(r'docs/plan/[^\s\)\]\>"\']+', line):
                remaining_plan.add(m.group())

        # Check for docs/design/ references (excluding kept files and archived)
        if "docs/design/" in line and "docs/archive/design-archive/" not in line:
            for m in re.finditer(r'docs/design/[^\s\)\]\>"\']+', line):
                path = m.group()
                # Check if it's a kept design file
                is_kept = any(path.startswith(f"docs/design/{k}") for k in design_to_keep)
                if not is_kept:
                    remaining_design.add(path)

        # Check for stray references (old location)
        for s in strays:
            if s in line and "docs/archive/design-archive/" not in line and "docs/archive/plan-archive/" not in line:
                remaining_strays.add(s)

if remaining_plan:
    print(f"\n  Remaining docs/plan/ references ({len(remaining_plan)}):")
    for r in sorted(remaining_plan):
        print(f"    {r}")

if remaining_design:
    print(f"\n  Remaining docs/design/ references (non-kept, {len(remaining_design)}):")
    for r in sorted(remaining_design):
        print(f"    {r}")

if remaining_strays:
    print(f"\n  Remaining stray references ({len(remaining_strays)}):")
    for r in sorted(remaining_strays):
        print(f"    {r}")

if not remaining_plan and not remaining_design and not remaining_strays:
    print("  ✅ All old-path references have been replaced!")
