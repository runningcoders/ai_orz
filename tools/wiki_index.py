#!/usr/bin/env python3
"""docs/wiki 索引生成与门禁。

背景
----
repowiki（Qoder IDE 插件）已下线，`docs/wiki/` 从「IDE 生成产物」转为**团队手工维护**。
497 篇文档在仓库里没有任何可读索引，导致三个真实问题：
  1. 人不知道「一共有哪些页、哪一类缺页」，只能靠翻目录；
  2. Agent 不知道「改了某块代码该动哪一篇」，只能全量扫描或凭印象；
  3. 新增页面后没有机制提醒补索引，索引一旦手写必然腐坏。

本脚本把索引变成**派生物**（目录即真源），从而让上面三件事都有解：
  1) --write   生成 docs/wiki/INDEX.md（分类 → 页面清单）
  2) --check   校验 INDEX.md 与磁盘一致（门禁：退码 1 表示过期）
  3) --stale   报告文档里引用的代码路径已不存在的条目（诊断，不阻塞）

设计取向
--------
- **只依赖标准库**：零编译、零外部依赖（与 tools/split_inline_tests.py 同风格）。
- **输出确定性**：不写时间戳，篇数/字节数由内容算出 —— 否则 --check 永远失败。
- **不制造第二真源**：INDEX.md 永远是派生物，一条命令即可重建，谁都不用手改。

用法
----
    python3 tools/wiki_index.py --write     # 重建索引
    python3 tools/wiki_index.py --check     # 门禁校验（CI / 钩子）
    python3 tools/wiki_index.py --stale     # 诊断失效的代码路径引用

退出码
------
    0  成功 / 索引最新
    1  --check 发现索引过期
    2  路径缺失等环境错误
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
WIKI_ROOT = REPO_ROOT / "docs" / "wiki"
CONTENT_ROOT = WIKI_ROOT / "zh" / "content"
KNOWLEDGE_ROOT = WIKI_ROOT / "knowledge" / "zh"
INDEX_PATH = WIKI_ROOT / "INDEX.md"

GENERATED_BANNER = (
    "<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；"
    "改内容请改源文件后跑 `make wiki-index` -->"
)

# 根目录散页（不在任何分类子目录下）的归类名
ROOT_BUCKET = "总纲与运维"


# ---------------------------------------------------------------- 采集

def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def extract_title(path: Path, text: str) -> str:
    """取文档标题：优先 YAML frontmatter 的 name 字段，其次首个 H1，最后文件名。

    knowledge 卡用 frontmatter（62/120 张没有 H1），zh/content 长文用 H1。
    """
    if text.startswith("---"):
        end = text.find("\n---", 3)
        if end != -1:
            for line in text[3:end].splitlines():
                stripped = line.strip()
                if stripped.startswith("name:"):
                    value = stripped[len("name:"):].strip().strip("'\"")
                    if value:
                        return value
    for line in text.splitlines():
        if line.startswith("# "):
            value = line[2:].strip()
            if value:
                return value
    return path.stem


def collect_content() -> tuple[dict[str, list[tuple[str, Path, int]]], int, int]:
    """采集 zh/content：返回 (分类 -> [(标题, 路径, 字节)], 总篇数, 总字节)。"""
    buckets: dict[str, list[tuple[str, Path, int]]] = {}
    total_bytes = 0
    paths = sorted(CONTENT_ROOT.rglob("*.md"))
    for path in paths:
        rel = path.relative_to(CONTENT_ROOT)
        bucket = rel.parts[0] if len(rel.parts) > 1 else ROOT_BUCKET
        text = read_text(path)
        size = path.stat().st_size
        total_bytes += size
        buckets.setdefault(bucket, []).append((extract_title(path, text), path, size))
    for items in buckets.values():
        items.sort(key=lambda item: str(item[1]))
    return buckets, len(paths), total_bytes


def collect_knowledge() -> tuple[list[tuple[str, Path, int]], int]:
    """采集 knowledge/zh：返回 ([(标题, 路径, 字节)], 总字节)。"""
    items: list[tuple[str, Path, int]] = []
    total_bytes = 0
    for path in sorted(KNOWLEDGE_ROOT.rglob("*.md")):
        text = read_text(path)
        size = path.stat().st_size
        total_bytes += size
        items.append((extract_title(path, text), path, size))
    return items, total_bytes


# ---------------------------------------------------------------- 渲染

def md_link(target: Path, title: str, base: Path = WIKI_ROOT) -> str:
    """生成相对 WIKI_ROOT 的 Markdown 链接。

    路径用 CommonMark 的尖括号形式 `<...>` 包裹：wiki 路径含中文与空格，
    URL 编码后不可读（`%E6%A0%B8...`），尖括号形式保持路径可读且编辑器/GitHub 均可解析。
    """
    rel = target.relative_to(base).as_posix()
    safe_title = title.replace("[", r"\[").replace("]", r"\]")
    return f"[{safe_title}](<{rel}>)"


def render_index() -> str:
    buckets, content_count, content_bytes = collect_content()
    knowledge, knowledge_bytes = collect_knowledge()
    total_count = content_count + len(knowledge)
    total_bytes = content_bytes + knowledge_bytes

    lines: list[str] = []
    add = lines.append

    add(GENERATED_BANNER)
    add("")
    add("# Wiki 索引")
    add("")
    add("> **自动生成**：生成器 `tools/wiki_index.py` · 门禁 `make wiki-lint` · 重建 `make wiki-index`")
    add(">")
    add("> 覆盖 `docs/wiki/zh/content/`（代码 Wiki 长文）与 `docs/wiki/knowledge/zh/`（RAG 原子知识卡）。")
    add("> 语义入口与维护办法见 [README.md](README.md)；四象限定位见 [../README.md](../README.md)。")
    add("")
    add(
        f"**合计 {total_count} 篇 / {total_bytes:,} B** —— "
        f"长文 {content_count} 篇 / {content_bytes:,} B，知识卡 {len(knowledge)} 张 / {knowledge_bytes:,} B。"
    )
    add("")

    # --- 分类概览 ---
    add("## 分类概览")
    add("")
    add("| 分类 | 篇数 | 字节 | 章节 |")
    add("|---|---:|---:|---|")
    ordered = sorted(buckets.items(), key=lambda kv: (-len(kv[1]), kv[0]))
    for bucket, items in ordered:
        size = sum(item[2] for item in items)
        anchor = bucket.replace(" ", "-")
        add(f"| {bucket} | {len(items)} | {size:,} | [跳转](#{anchor}) |")
    add(f"| RAG 原子知识卡 | {len(knowledge)} | {knowledge_bytes:,} | [跳转](#rag-原子知识卡) |")
    add("")

    # --- 长文分节 ---
    add("## 代码 Wiki 长文")
    add("")
    for bucket, items in sorted(buckets.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        add(f"### {bucket}（{len(items)} 篇）")
        add("")
        add("| # | 页面 | 字节 |")
        add("|---:|---|---:|")
        for idx, (title, path, size) in enumerate(items, start=1):
            add(f"| {idx} | {md_link(path, title)} | {size:,} |")
        add("")

    # --- 知识卡 ---
    add("## RAG 原子知识卡")
    add("")
    add(f"共 {len(knowledge)} 张，按路径排序。标题取 frontmatter `name` 字段（无 H1 的卡同样可索引）。")
    add("")
    add("| # | 卡片 | 字节 |")
    add("|---:|---|---:|")
    for idx, (title, path, size) in enumerate(knowledge, start=1):
        add(f"| {idx} | {md_link(path, title)} | {size:,} |")
    add("")

    return "\n".join(lines)


# ---------------------------------------------------------------- 命令

def cmd_write() -> int:
    if not CONTENT_ROOT.is_dir():
        print(f"[wiki_index] 目录不存在：{CONTENT_ROOT}", file=sys.stderr)
        return 2
    rendered = render_index()
    INDEX_PATH.parent.mkdir(parents=True, exist_ok=True)
    INDEX_PATH.write_text(rendered, encoding="utf-8")
    line_count = rendered.count("\n") + 1
    print(f"[wiki_index] 已写入 {INDEX_PATH.relative_to(REPO_ROOT)}（{line_count} 行 / {len(rendered.encode())} B）")
    return 0


def cmd_check() -> int:
    if not INDEX_PATH.is_file():
        print(
            f"[wiki_index] 索引缺失：{INDEX_PATH.relative_to(REPO_ROOT)}\n"
            f"            请运行：make wiki-index（或 python3 tools/wiki_index.py --write）",
            file=sys.stderr,
        )
        return 1
    expected = render_index()
    actual = INDEX_PATH.read_text(encoding="utf-8")
    if expected == actual:
        _, content_count, _ = collect_content()
        knowledge, _ = collect_knowledge()
        print(f"[wiki_index] OK：索引与目录一致（{content_count} 篇长文 + {len(knowledge)} 张知识卡）")
        return 0

    exp_lines = expected.splitlines()
    act_lines = actual.splitlines()
    diffs: list[str] = []
    for lineno in range(max(len(exp_lines), len(act_lines))):
        left = exp_lines[lineno] if lineno < len(exp_lines) else "<缺失>"
        right = act_lines[lineno] if lineno < len(act_lines) else "<缺失>"
        if left != right:
            diffs.append(f"  L{lineno + 1}\n    - 期望: {left[:160]}\n    + 实况: {right[:160]}")
        if len(diffs) >= 12:
            diffs.append("  ...（更多差异省略）")
            break

    print(
        "[wiki_index] 索引已过期：docs/wiki/INDEX.md 与目录不一致。\n"
        "            多半是新增/删除/改名了 wiki 页面。\n"
        "            修复：make wiki-index（重建后一并提交 INDEX.md）\n"
        "            ---- 差异（磁盘 vs 索引实况）----",
        file=sys.stderr,
    )
    print("\n".join(diffs), file=sys.stderr)
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
    for path in sorted(WIKI_ROOT.rglob("*.md")):
        if path.name in {"INDEX.md", "README.md"}:
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
                missing.setdefault(target, []).append(str(path.relative_to(WIKI_ROOT)))

    rel_path = INDEX_PATH.relative_to(REPO_ROOT)
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


def main() -> int:
    parser = argparse.ArgumentParser(
        description="docs/wiki 索引生成与门禁（目录即真源，INDEX.md 为派生物）",
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--write", action="store_true", help="重建 docs/wiki/INDEX.md")
    group.add_argument("--check", action="store_true", help="校验 INDEX.md 是否与目录一致（门禁）")
    group.add_argument("--stale", action="store_true", help="报告失效的代码路径引用（诊断）")
    args = parser.parse_args()

    if args.check:
        return cmd_check()
    if args.stale:
        return cmd_stale()
    return cmd_write()


if __name__ == "__main__":
    sys.exit(main())
