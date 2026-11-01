#!/usr/bin/env python3
"""docs/modules 知识库索引生成与门禁。

背景
----
文档已按 15 个能力域模块重组，每模块三层：

    A-设计     —— 为什么（设计决策快照）
    B-落地方案 —— 要去哪（规划 / 状态快照）
    C-知识     —— 是什么（代码现状百科长文 + RAG 原子知识卡）

三层各自可读，但没有任何**统一入口**把它们串起来，导致三个真实问题：
  1. 人不知道「一共有哪些页、哪一层缺页」，只能靠翻目录；
  2. Agent 不知道「要实现某模块的需求，该先读哪篇设计、现状在哪篇」，只能全量扫描或凭印象；
  3. 索引一旦手写必然腐坏（模块内 00-INDEX.md 的篇数统计已与磁盘脱节）。

本脚本把索引变成**派生物**（目录即真源），一次重建产出三件东西：

  1) docs/modules/INDEX.md           全库人读索引（A/B/C 三层 + 设计状态）
  2) docs/modules/INDEX.json         机器可读索引（Agent / 工具消费，含 frontmatter 元数据）
  3) docs/modules/<模块>/00-INDEX.md 每模块索引（取代原先手写、会腐坏的那份）

命令
----
    --write   重建上述三类产物
    --check   校验三者与磁盘一致（门禁：退码 1 表示过期）
    --stale   报告文档里引用的代码路径已不存在（诊断，不阻塞）
    --meta    校验 A-设计 / B-落地方案 的 frontmatter 完整性（门禁）

设计取向
--------
- **只依赖标准库**：零编译、零外部依赖（与 tools/split_inline_tests.py 同风格）。
- **输出确定性**：不写时间戳，篇数/字节数由内容算出 —— 否则 --check 永远失败。
- **不制造第二真源**：三类索引都是派生物，一条命令即可重建，谁都不用手改。
- **元数据不瞎猜**：frontmatter 缺失即报错，不替作者补语义（`status: 未标注` 只告警）。

退出码
------
    0  成功 / 索引最新
    1  --check 索引过期，或 --meta 发现 frontmatter 缺失
    2  路径缺失等环境错误
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import NamedTuple

REPO_ROOT = Path(__file__).resolve().parent.parent
# 文档已按能力域模块化：每模块下 A-设计 / B-落地方案 / C-知识 三层
MODULES_ROOT = REPO_ROOT / "docs" / "modules"
INDEX_PATH = MODULES_ROOT / "INDEX.md"
INDEX_JSON_PATH = MODULES_ROOT / "INDEX.json"
MODULE_INDEX_NAME = "00-INDEX.md"

# 顺序即渲染顺序：为什么 → 要去哪 → 是什么
TIERS = ("A-设计", "B-落地方案", "C-知识")
TIER_LABEL = {
    "A-设计": ("A 设计", "design"),
    "B-落地方案": ("B 落地方案", "plan"),
    "C-知识": ("C 知识", "wiki"),
}
# 需要 frontmatter 的层（C-知识 的 RAG 卡另有既定格式，不在此校验范围）
META_TIERS = ("A-设计", "B-落地方案")
REQUIRED_META_KEYS = ("kind", "module", "name", "title", "status")

GENERATED_BANNER = (
    "<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；"
    "改内容请改源文件后跑 `make wiki-index` -->"
)


# ---------------------------------------------------------------- 基础

def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def unquote(value: str) -> str:
    """去掉 YAML 标量两侧的引号；单引号内的 '' 还原为 '。"""
    if len(value) >= 2 and value[0] == value[-1] == "'":
        return value[1:-1].replace("''", "'")
    if len(value) >= 2 and value[0] == value[-1] == '"':
        return value[1:-1].replace('\\"', '"')
    return value


def parse_frontmatter(text: str) -> dict[str, object]:
    """解析极简 YAML frontmatter（只支持本项目用到的标量与标量列表）。

    注意：大量文档以 `---` 作水平分隔线开头，必须校验"有闭合且中间含合法键"，
    否则会把分隔线误当 frontmatter。
    """
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return {}
    end = -1
    for idx in range(1, min(len(lines), 80)):
        if lines[idx].strip() == "---":
            end = idx
            break
    if end == -1:
        return {}

    body = lines[1:end]
    if not any(re.match(r"^[A-Za-z_][\w-]*:", line) for line in body):
        return {}

    data: dict[str, object] = {}
    cur_key: str | None = None
    for line in body:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("- "):
            item = unquote(stripped[2:].strip())
            if cur_key is not None:
                bucket = data.get(cur_key)
                if not isinstance(bucket, list):
                    bucket = []
                    data[cur_key] = bucket
                bucket.append(item)
            continue
        if ":" not in stripped:
            continue
        key, value = stripped.split(":", 1)
        key, value = key.strip(), value.strip()
        if value == "[]":
            data[key] = []
            cur_key = None
        elif value == "":
            data[key] = []
            cur_key = key
        else:
            data[key] = unquote(value)
            cur_key = None
    return data


def extract_title(path: Path, text: str) -> str:
    """取文档标题：title 字段 → name 字段 → 首个 H1 → 文件名。

    A/B 层用 `title`（中文标题），C-知识 RAG 卡用 `name`（长描述），
    两者都没有时才退回 H1 / 文件名 —— 保证既有 C-知识 标题不因本改动漂移。
    """
    meta = parse_frontmatter(text)
    for key in ("title", "name"):
        value = meta.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    for line in text.splitlines():
        if line.startswith("# "):
            value = line[2:].strip()
            if value:
                return value
    return path.stem


def as_list(value: object) -> list[str]:
    if isinstance(value, list):
        return [str(item) for item in value if str(item).strip()]
    if isinstance(value, str) and value.strip():
        return [value.strip()]
    return []


class Entry(NamedTuple):
    tier: str
    title: str
    path: Path
    size: int
    status: str
    scope: list[str]
    related: list[str]


def make_entry(tier: str, path: Path) -> Entry:
    text = read_text(path)
    meta = parse_frontmatter(text)
    status = meta.get("status")
    return Entry(
        tier=tier,
        title=extract_title(path, text),
        path=path,
        size=path.stat().st_size,
        status=status.strip() if isinstance(status, str) else "",
        scope=as_list(meta.get("scope")),
        related=as_list(meta.get("related")),
    )


# ---------------------------------------------------------------- 采集

def collect_all() -> dict[str, dict[str, list[Entry]]]:
    """采集 docs/modules/<模块>/{A-设计,B-落地方案,C-知识}：模块 → 层 → 条目。"""
    modules: dict[str, dict[str, list[Entry]]] = {}
    if not MODULES_ROOT.is_dir():
        return modules
    for mod_dir in sorted(MODULES_ROOT.iterdir()):
        if not mod_dir.is_dir():
            continue
        tiers: dict[str, list[Entry]] = {}
        for tier in TIERS:
            tier_dir = mod_dir / tier
            if not tier_dir.is_dir():
                continue
            entries = [
                make_entry(tier, path)
                for path in sorted(tier_dir.rglob("*.md"))
                if path.name != MODULE_INDEX_NAME
            ]
            if entries:
                tiers[tier] = entries
        if tiers:
            modules[mod_dir.name] = tiers
    return modules


def totals(modules: dict[str, dict[str, list[Entry]]]) -> tuple[int, int]:
    count = sum(len(items) for tiers in modules.values() for items in tiers.values())
    size = sum(e.size for tiers in modules.values() for items in tiers.values() for e in items)
    return count, size


# ---------------------------------------------------------------- 渲染：全局人读索引

def md_link(target: Path, title: str, base: Path) -> str:
    """生成相对 base 的 Markdown 链接。

    路径用 CommonMark 的尖括号形式 `<...>` 包裹：模块路径含中文与空格，
    URL 编码后不可读（`%E6%A0%B8...`），尖括号形式保持路径可读且编辑器/GitHub 均可解析。
    """
    rel = target.relative_to(base).as_posix()
    safe_title = title.replace("[", r"\[").replace("]", r"\]")
    return f"[{safe_title}](<{rel}>)"


def gh_anchor(heading: str) -> str:
    """GitHub 风格锚点：小写、去掉标点、空格转连字符（保留 CJK）。"""
    text = heading.strip().lower()
    text = re.sub(r"[^\w\u4e00-\u9fff \-]", "", text)
    return text.replace(" ", "-")


def tier_table(add, entries: list[Entry], with_status: bool) -> None:
    if with_status:
        add("| # | 页面 | 状态 | 字节 |")
        add("|---:|---|---|---:|")
        for idx, e in enumerate(entries, start=1):
            status = e.status or "—"
            add(f"| {idx} | {md_link(e.path, e.title, MODULES_ROOT)} | {status} | {e.size:,} |")
    else:
        add("| # | 页面 | 字节 |")
        add("|---:|---|---:|")
        for idx, e in enumerate(entries, start=1):
            add(f"| {idx} | {md_link(e.path, e.title, MODULES_ROOT)} | {e.size:,} |")


def render_index() -> str:
    modules = collect_all()
    total_count, total_bytes = totals(modules)

    lines: list[str] = []
    add = lines.append

    add(GENERATED_BANNER)
    add("")
    add("# 模块知识索引（docs/modules）")
    add("")
    add("> **自动生成**：生成器 `tools/wiki_index.py` · 门禁 `make wiki-lint` · 重建 `make wiki-index`")
    add(">")
    add("> 覆盖三个层：`A-设计`（为什么）· `B-落地方案`（要去哪）· `C-知识`（是什么）。")
    add("> 机器可读版见 `INDEX.json`；Agent 实现需求请从 `docs/AGENT_GUIDE.md` 进入。")
    add("")
    add(f"**合计 {total_count} 篇 / {total_bytes:,} B**，分布于 {len(modules)} 个模块。")
    add("")

    add("## 模块概览")
    add("")
    add("| 模块 | A 设计 | B 落地方案 | C 知识 | 合计 | 字节 |")
    add("|---|---:|---:|---:|---:|---:|")
    for module, tiers in sorted(modules.items()):
        counts = [len(tiers.get(tier, [])) for tier in TIERS]
        module_bytes = sum(e.size for items in tiers.values() for e in items)
        add(
            f"| [{module}](#{gh_anchor(module)}) | {counts[0]} | {counts[1]} | {counts[2]} "
            f"| {sum(counts)} | {module_bytes:,} |"
        )
    add("")

    add("## 各模块文档")
    add("")
    for module, tiers in sorted(modules.items()):
        add(f"### {module}")
        add("")
        for tier in TIERS:
            entries = tiers.get(tier)
            if not entries:
                continue
            label, en = TIER_LABEL[tier]
            add(f"#### {label}（{en}，{len(entries)} 篇）")
            add("")
            tier_table(add, entries, with_status=tier in META_TIERS)
            add("")

    return "\n".join(lines)


# ---------------------------------------------------------------- 渲染：机器可读索引

def render_json() -> str:
    modules = collect_all()
    total_count, total_bytes = totals(modules)

    payload_modules = []
    for module, tiers in sorted(modules.items()):
        docs = []
        for tier in TIERS:
            for e in tiers.get(tier, []):
                item: dict[str, object] = {
                    "tier": tier,
                    "path": e.path.relative_to(MODULES_ROOT).as_posix(),
                    "name": e.path.stem,
                    "title": e.title,
                    "bytes": e.size,
                }
                # 只给设计/计划层带元数据，C-知识 保持精简（其 frontmatter 已足够详尽）
                if tier in META_TIERS:
                    item["status"] = e.status or "未标注"
                    item["scope"] = e.scope
                    item["related"] = e.related
                docs.append(item)
        payload_modules.append(
            {
                "id": module,
                "counts": {tier: len(tiers.get(tier, [])) for tier in TIERS},
                "total": sum(len(items) for items in tiers.values()),
                "docs": docs,
            }
        )

    payload = {
        "generated_by": "tools/wiki_index.py",
        "schema": 1,
        "note": (
            "机器可读知识库索引，由 `make wiki-index` 重建，请勿手工编辑。"
            "path 相对 docs/modules/；status 只从文档显式信号推导，未标注表示需人工确认。"
        ),
        "total": total_count,
        "total_bytes": total_bytes,
        "modules": payload_modules,
    }
    return json.dumps(payload, ensure_ascii=False, indent=2) + "\n"


# ---------------------------------------------------------------- 渲染：每模块索引

def render_module_index(module: str, tiers: dict[str, list[Entry]]) -> str:
    mod_dir = MODULES_ROOT / module
    total = sum(len(items) for items in tiers.values())
    counts = " · ".join(
        f"{TIER_LABEL[tier][0]} {len(tiers[tier])}" for tier in TIERS if tiers.get(tier)
    )

    lines: list[str] = []
    add = lines.append
    add(GENERATED_BANNER)
    add("")
    add(f"# {module}")
    add("")
    add("> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。")
    add(">")
    add("> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。")
    add("")
    add(f"本模块共 {total} 篇：{counts}。")
    add("")

    for tier in TIERS:
        entries = tiers.get(tier)
        if not entries:
            continue
        label, en = TIER_LABEL[tier]
        add(f"## {label}（{en}）")
        add("")
        if tier in META_TIERS:
            for e in entries:
                status = e.status or "未标注"
                add(f"- {md_link(e.path, e.title, mod_dir)} — `{status}`")
        else:
            for e in entries:
                add(f"- {md_link(e.path, e.title, mod_dir)}")
        add("")

    return "\n".join(lines)


# ---------------------------------------------------------------- 命令

def expected_artifacts() -> dict[Path, str]:
    """所有派生产物 → 期望内容。--write 与 --check 共用，保证同一真源。"""
    modules = collect_all()
    artifacts: dict[Path, str] = {
        INDEX_PATH: render_index(),
        INDEX_JSON_PATH: render_json(),
    }
    for module, tiers in modules.items():
        artifacts[MODULES_ROOT / module / MODULE_INDEX_NAME] = render_module_index(module, tiers)
    return artifacts


def cmd_write() -> int:
    if not MODULES_ROOT.is_dir():
        print(f"[wiki_index] 目录不存在：{MODULES_ROOT}", file=sys.stderr)
        return 2
    modules = collect_all()
    total_count, _ = totals(modules)

    artifacts = expected_artifacts()
    for path, content in artifacts.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    print(
        f"[wiki_index] 已写入 {len(artifacts)} 个文件"
        f"（INDEX.md + INDEX.json + {len(modules)} 个 00-INDEX.md）"
    )
    print(f"[wiki_index] 合计 {total_count} 篇，{len(modules)} 个模块")
    return 0


def cmd_check() -> int:
    if not INDEX_PATH.is_file():
        print(
            f"[wiki_index] 索引缺失：{INDEX_PATH.relative_to(REPO_ROOT)}\n"
            f"            请运行：make wiki-index（或 python3 tools/wiki_index.py --write）",
            file=sys.stderr,
        )
        return 1

    artifacts = expected_artifacts()
    stale_files: list[tuple[Path, str]] = []
    missing_files: list[Path] = []

    for path, expected in artifacts.items():
        if not path.is_file():
            missing_files.append(path)
            continue
        actual = path.read_text(encoding="utf-8")
        if actual != expected:
            stale_files.append((path, expected))

    if not stale_files and not missing_files:
        modules = collect_all()
        count, _ = totals(modules)
        print(f"[wiki_index] OK：索引与目录一致（{count} 篇，{len(modules)} 个模块）")
        return 0

    if missing_files:
        print("[wiki_index] 缺失产物：", file=sys.stderr)
        for path in missing_files:
            print(f"  - {path.relative_to(REPO_ROOT)}", file=sys.stderr)
    if stale_files:
        print("[wiki_index] 索引已过期（内容与目录不一致）：", file=sys.stderr)
        for path, expected in stale_files[:12]:
            print(f"  - {path.relative_to(REPO_ROOT)}", file=sys.stderr)
            exp_lines, act_lines = expected.splitlines(), path.read_text(encoding="utf-8").splitlines()
            for lineno in range(max(len(exp_lines), len(act_lines))):
                left = exp_lines[lineno] if lineno < len(exp_lines) else "<缺失>"
                right = act_lines[lineno] if lineno < len(act_lines) else "<缺失>"
                if left != right:
                    print(f"      L{lineno + 1}\n        - 期望: {left[:140]}\n        + 实况: {right[:140]}")
                    break
        if len(stale_files) > 12:
            print(f"  ...（另有 {len(stale_files) - 12} 个）", file=sys.stderr)

    print(
        "\n[wiki_index] 多半是新增/删除/改名了模块文档。\n"
        "            修复：make wiki-index（重建后一并提交 INDEX.md / INDEX.json / 各 00-INDEX.md）",
        file=sys.stderr,
    )
    return 1


CODE_PATH_RE = re.compile(
    r"`((?:src|common|frontend|tools|scripts|migrations|tests)/[A-Za-z0-9_\-./]+)`"
)
CITE_LINK_RE = re.compile(r"\[[^\]]*\]\(((?:src|common|frontend|tools|scripts|migrations|tests)/[^)]+)\)")
PLACEHOLDER_RE = re.compile(r"(xxx|XXX|foo|bar|your_|\.\.\.)")


def cmd_stale() -> int:
    """报告文档引用的代码路径已不存在 —— 只诊断不阻塞（示例占位会命中，需人工判断）。"""
    missing: dict[str, list[str]] = {}
    checked = 0
    for path in sorted(MODULES_ROOT.rglob("*.md")):
        if path.name in ("INDEX.md", MODULE_INDEX_NAME):
            continue
        text = read_text(path)
        refs = set(CODE_PATH_RE.findall(text)) | set(CITE_LINK_RE.findall(text))
        for ref in refs:
            # 去掉 `#L12-L34` 行号锚点与尾随标点：锚点不是路径的一部分
            target = ref.split("#", 1)[0].rstrip(".,;:)`")
            if not target or PLACEHOLDER_RE.search(target):
                continue
            checked += 1
            probe = REPO_ROOT / target
            if not probe.exists():
                missing.setdefault(target, []).append(str(path.relative_to(MODULES_ROOT)))

    print(f"[wiki_index] 代码路径引用体检：检查 {checked} 条（不含示例占位语义的名字）")
    if not missing:
        print("[wiki_index] 未发现失效引用。")
        return 0

    print(f"[wiki_index] 失效引用 {len(missing)} 个目标（涉及 {sum(len(v) for v in missing.values())} 处文档）：")
    for target in sorted(missing):
        pages = missing[target]
        print(f"  - {target}  ←  {len(pages)} 篇（如 {pages[0]}）")
    print("\n[wiki_index] 提示：本项为诊断输出，不参与门禁；修正后重跑确认归零。")
    return 0


def cmd_meta() -> int:
    """校验 A-设计 / B-落地方案 的 frontmatter 完整性（门禁）。

    - 缺 frontmatter 或缺必填键 → 退码 1（新增设计/计划文档必须带元数据）
    - `status: 未标注` → 仅告警（文档里没有可推导的显式信号，需人工确认）
    """
    bad: list[tuple[str, str]] = []
    unmarked: list[str] = []
    ok = 0

    for tier in META_TIERS:
        for path in sorted(MODULES_ROOT.glob(f"*/{tier}/**/*.md")):
            if path.name == MODULE_INDEX_NAME:
                continue
            rel = path.relative_to(MODULES_ROOT).as_posix()
            meta = parse_frontmatter(read_text(path))
            if not meta:
                bad.append((rel, "缺 frontmatter"))
                continue
            missing_keys = [k for k in REQUIRED_META_KEYS if not str(meta.get(k) or "").strip()]
            if missing_keys:
                bad.append((rel, "缺字段：" + ", ".join(missing_keys)))
                continue
            if str(meta.get("status")).strip() == "未标注":
                unmarked.append(rel)
            else:
                ok += 1

    total = ok + len(unmarked) + len(bad)
    print(f"[wiki_index] 元数据体检：{total} 篇设计/计划文档（规范 {ok} · 未标注 {len(unmarked)} · 违规 {len(bad)}）")

    if unmarked:
        print(f"\n[wiki_index] status 未标注 {len(unmarked)} 篇（不阻塞，但 Agent 会当作需人工确认）：")
        for rel in unmarked[:20]:
            print(f"  - {rel}")
        if len(unmarked) > 20:
            print(f"  ...（另有 {len(unmarked) - 20} 篇）")

    if bad:
        print(f"\n[wiki_index] ✗ frontmatter 不合规 {len(bad)} 篇：", file=sys.stderr)
        for rel, reason in bad:
            print(f"  - {rel} —— {reason}", file=sys.stderr)
        print(
            "\n            修复：按 docs/README.md「元数据规范」补 frontmatter；"
            "字段模板见任一 A-设计 文档头部。",
            file=sys.stderr,
        )
        return 1

    print("[wiki_index] ✓ 元数据全部合规。")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="docs/modules 知识库索引生成与门禁（目录即真源，索引为派生物）",
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--write", action="store_true", help="重建 INDEX.md / INDEX.json / 各 00-INDEX.md")
    group.add_argument("--check", action="store_true", help="校验索引与目录一致（门禁）")
    group.add_argument("--stale", action="store_true", help="报告失效的代码路径引用（诊断）")
    group.add_argument("--meta", action="store_true", help="校验 A-设计/B-落地方案 frontmatter（门禁）")
    args = parser.parse_args()

    if args.check:
        return cmd_check()
    if args.stale:
        return cmd_stale()
    if args.meta:
        return cmd_meta()
    return cmd_write()


if __name__ == "__main__":
    sys.exit(main())
