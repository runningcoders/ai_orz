#!/usr/bin/env python3
"""ai_orz target 精准 GC —— 只保留当前构建还能命中的缓存。

背景：Cargo 1.99.1 只回收 ~/.cargo 全局缓存，target/ 里的 build artifacts GC
尚未实现（Cargo Book: "Support for tracking build artifacts is not yet implemented",
cargo#13136）。源码 / RUSTFLAGS / feature 每次变化都生成一整套新指纹 hash 副本，
旧副本永不回收 —— 项目实测 target 涨到 346G。

判据（第一优先，绝不能错杀）：**cargo 自己的 fresh 标记**。
.fingerprint/<crate>-<hash>/ 下只要存在非空输出标记文件（lib-foo、bin-foo、
build-script-build-script-build、run-build-script-build-script-build 等），
就说明 cargo 下次构建会直接命中这份缓存 —— 该 hash 的所有产物（rlib/rmeta/d、
build 目录）一律保留。

⚠️ 不能用 invoked.timestamp 的 mtime 当唯一判据（旧实现就是这么干，这是 bug）：
cargo 只在**真正重新编译**某个单元时才更新它，一个长期命中缓存（永远 fresh）的
crate，它的时间戳会一直停在几个月前。旧规则「每 crate 只留最近 2 份 hash」于是会
把当前有效、但时间戳很旧的那份当成死副本删掉；而删掉 deps 里的 rlib 或 build/
目录后，cargo 必须重编该 crate 并把**所有下游**连锁重编（最小工程实测复现）——
表现就是「每次编译都重写一遍缓存，却永远用不上」。

第二优先（兜底）：时间窗口（默认 7 天）内被使用过的 hash 全留；再按 crate 保最近
若干份。无条件删除 *.rcgu.o（rustc 的 codegen-unit 中间对象，链接完成后不再被引用，
删了不影响 cargo 判定 fresh —— 已实测）；可执行产物（集成测试 / 二进制）不是缓存，
只留「最近 2 小时内链接出的、每个 target 最新 1 份」。

⚠️ 本脚本只做「判定 + 统计 + 生成删除清单」，不自己 unlink：实测逐文件
os.unlink 在 macOS/APFS 上删 5000 个要 20 分钟，而 xargs + rm 批量删 1 万个
只要 5 秒。真正的删除由 scripts/clean_target.sh 用 xargs -0 批量执行。

调用方只有 scripts/clean_target.sh（stale / slim）。可独立 --dry-run 预览。
"""
import argparse
import os
import re
import sys
import time
from collections import defaultdict

HASH_RE = re.compile(r"-([0-9a-f]{16})(?:\.|$)")
SESSION_RE = re.compile(r"^(.+)-([A-Za-z0-9]+)$")
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = os.path.join(REPO_ROOT, "target")
DEFAULT_MANIFEST = os.path.join(TARGET, ".ai_orz_gc_manifest")

_MANIFEST = None


def file_hash(name):
    m = HASH_RE.search(name)
    return m.group(1) if m else None


def crate_of(name, h):
    i = name.rfind("-" + h)
    return name[:i] if i > 0 else name


def dir_size(path):
    total = 0
    count = 0
    stack = [path]
    while stack:
        cur = stack.pop()
        try:
            with os.scandir(cur) as it:
                for e in it:
                    try:
                        if e.is_dir(follow_symlinks=False):
                            stack.append(e.path)
                        else:
                            total += e.stat(follow_symlinks=False).st_size
                            count += 1
                    except OSError:
                        pass
        except OSError:
            pass
    return total, count


def newest_mtime(path):
    best = 0.0
    stack = [path]
    while stack:
        cur = stack.pop()
        try:
            with os.scandir(cur) as it:
                for e in it:
                    try:
                        if e.is_dir(follow_symlinks=False):
                            stack.append(e.path)
                        else:
                            best = max(best, e.stat(follow_symlinks=False).st_mtime)
                    except OSError:
                        pass
        except OSError:
            pass
    return best


class Stats:
    def __init__(self):
        self.by_cat = defaultdict(lambda: [0, 0])
        self.kept = [0, 0]
        self.paths = 0

    def deleted(self, cat, size, count):
        self.by_cat[cat][0] += size
        self.by_cat[cat][1] += count

    def keep(self, size, count=1):
        self.kept[0] += size
        self.kept[1] += count


def remove(path, cat, stats, dry, is_dir=False, size=None, count=1):
    """校验 + 统计 + 写入删除清单；不执行删除。"""
    if os.path.commonpath([os.path.abspath(path), TARGET]) != TARGET:
        raise RuntimeError("拒绝删除 target/ 之外的路径: " + path)
    try:
        if size is None:
            if is_dir:
                size, count = dir_size(path)
            else:
                size, count = os.stat(path).st_size, 1
    except OSError:
        return
    stats.deleted(cat, size, count)
    if not dry and _MANIFEST is not None:
        _MANIFEST.write(path + "\0")
        stats.paths += 1


def has_live_marker(path):
    """cargo 的 fresh 判定依赖 .fingerprint/<unit>/ 下的输出标记文件。

    只要存在一个非空、非 json / 非 dep-info 的标记文件（lib-foo、bin-foo、
    build-script-build-script-build、run-build-script-build-script-build 等），
    cargo 下次构建就会直接复用这份缓存。这样的 hash 一律不许删。
    """
    try:
        with os.scandir(path) as it:
            for e in it:
                n = e.name
                if n.endswith(".json") or n.startswith("dep-") or n == "invoked.timestamp":
                    continue
                try:
                    if e.is_file(follow_symlinks=False) and e.stat(follow_symlinks=False).st_size > 0:
                        return True
                except OSError:
                    pass
    except OSError:
        pass
    return False


def collect_fingerprints(fp_dir):
    """返回 (hash -> 最近使用时间, cargo 认为可复用的 hash 集合)。"""
    out = {}
    live = set()
    if not os.path.isdir(fp_dir):
        return out, live
    with os.scandir(fp_dir) as it:
        for e in it:
            if not e.is_dir(follow_symlinks=False):
                continue
            h = file_hash(e.name)
            if not h:
                continue
            ts = os.path.join(e.path, "invoked.timestamp")
            probe = ts if os.path.isfile(ts) else e.path
            try:
                m = os.stat(probe).st_mtime
            except OSError:
                continue
            if m > out.get(h, 0):
                out[h] = m
            if has_live_marker(e.path):
                live.add(h)
    return out, live


def choose_keep(items, fp_times, cutoff, per_crate, live=frozenset()):
    """items: [(hash, crate, fallback_mtime)] -> 要保留的 hash 集合。

    保留 = cargo 仍认为 fresh 的 hash（live，最高优先）∪ 每 crate 最近 per_crate 份
    ∪ 时间窗口内被使用过的全部。
    """
    by_crate = defaultdict(list)
    for h, crate, fallback in items:
        by_crate[crate].append((fp_times.get(h, fallback), h))
    keep = set(live)
    for pairs in by_crate.values():
        pairs.sort(reverse=True)
        keep.update(h for _, h in pairs[:per_crate])
        keep.update(h for m, h in pairs if m >= cutoff)
    return keep


def gc_deps(deps, fp_times, cutoff, per_crate, stats, dry, live=frozenset(), safe=False):
    files = []
    items = []
    exe_items = []
    with os.scandir(deps) as it:
        for e in it:
            if not e.is_file(follow_symlinks=False):
                continue
            try:
                st = e.stat(follow_symlinks=False)
                m, size = st.st_mtime, st.st_size
            except OSError:
                continue
            h = file_hash(e.name)
            is_exe = ("." not in e.name) and bool(st.st_mode & 0o111)
            files.append((e.name, e.path, h, size, is_exe))
            if not e.name.endswith(".rcgu.o") and h:
                crate = crate_of(e.name, h)
                items.append((h, crate, m))
                if is_exe:
                    exe_items.append((h, crate, m))
    keep = choose_keep(items, fp_times, cutoff, per_crate, live)
    # 可执行产物（集成测试 / 二进制）不是缓存，只是链接结果：每个 target 只留最新 1 份，
    # 且不吃「时间窗口内全保留」的宽限 —— 否则换了 profile / feature 之后，一整批
    # 700M 的旧测试二进制会一直留在盘上（实测 47 个共 32G），当前构建永不命中。
    # 删了最多触发一次重链接（分钟级），不会重编依赖。
    # 只有「最近 2 小时内链接出来的」才有保留价值：刚跑完 cargo test 马上再编译时
    # 省一次重链。更早的一律删（含换了 profile / feature 后留下的整批孤儿二进制）。
    exe_cutoff = time.time() - 2 * 3600
    exe_recent = [it for it in exe_items if it[2] >= exe_cutoff]
    exe_keep = choose_keep(exe_recent, fp_times, float("inf"), 1)
    for name, path, h, size, is_exe in files:
        if name.endswith(".rcgu.o"):
            remove(path, "rcgu.o（codegen 中间对象）", stats, dry, size=size)
        elif h is None:
            stats.keep(size)  # 不带指纹的文件由 Cargo 自己管理，不动
        elif is_exe and h not in exe_keep:
            remove(path, "旧测试/可执行副本", stats, dry, size=size)
        elif safe:
            # 安全模式（编译前自动调用）：rlib/rmeta/.d 一律不碰 —— 删掉任何一个都可能
            # 让 cargo 重编该 crate 并连锁重编全部下游（见文件头注）。
            stats.keep(size)
        elif h not in keep:
            remove(path, "deps 历史指纹副本", stats, dry, size=size)
        else:
            stats.keep(size)


def gc_build(build_dir, fp_times, cutoff, per_crate, stats, dry, live=frozenset()):
    dirs = []
    items = []
    with os.scandir(build_dir) as it:
        for e in it:
            if not e.is_dir(follow_symlinks=False):
                continue
            h = file_hash(e.name)
            if not h:
                continue
            try:
                m = e.stat(follow_symlinks=False).st_mtime
            except OSError:
                m = 0
            dirs.append((e.path, h))
            items.append((h, crate_of(e.name, h), m))
    keep = choose_keep(items, fp_times, cutoff, per_crate, live)
    for path, h in dirs:
        if h not in keep:
            remove(path, "build 历史 out 目录", stats, dry, is_dir=True)
        else:
            size, _ = dir_size(path)
            stats.keep(size)


def gc_incremental(inc_dir, stats, dry):
    """incremental 是可再生缓存：每个 crate 只留最新 1 个 session。"""
    by_crate = defaultdict(list)
    with os.scandir(inc_dir) as it:
        for e in it:
            if not e.is_dir(follow_symlinks=False):
                continue
            m = SESSION_RE.match(e.name)
            crate = m.group(1) if m else e.name
            try:
                mt = e.stat(follow_symlinks=False).st_mtime
            except OSError:
                mt = 0
            by_crate[crate].append((mt, e.path))
    for pairs in by_crate.values():
        pairs.sort(reverse=True)
        for _, path in pairs[1:]:
            remove(path, "incremental 旧 session", stats, dry, is_dir=True)
        if pairs:
            size, _ = dir_size(pairs[0][1])
            stats.keep(size)


def gc_oneoff(target, cutoff, stats, dry, force_oneoff=False):
    """一次性工具 target：超过窗口没用过就整个删（下次对应命令自动重建）。"""
    for name in ("llvm-cov-target", "tarpaulin", "dx"):
        path = os.path.join(target, name)
        if not os.path.isdir(path):
            continue
        mt = newest_mtime(path)
        if force_oneoff or (mt and mt < cutoff):
            remove(path, "闲置工具 target/" + name, stats, dry, is_dir=True)
    for name in ("sqlx-tmp", "tmp"):
        path = os.path.join(target, name)
        if not os.path.isdir(path):
            continue
        with os.scandir(path) as it:
            for e in it:
                remove(e.path, "临时目录 " + name, stats, dry,
                       is_dir=e.is_dir(follow_symlinks=False))


def find_profiles(target):
    """找所有含 deps/ 的 profile 目录（target 下两层）。"""
    out = []
    with os.scandir(target) as it:
        for p in it:
            if not p.is_dir(follow_symlinks=False):
                continue
            if os.path.isdir(os.path.join(p.path, "deps")):
                out.append(p.path)
                continue
            try:
                with os.scandir(p.path) as sub:
                    for q in sub:
                        if q.is_dir(follow_symlinks=False) and os.path.isdir(
                                os.path.join(q.path, "deps")):
                            out.append(q.path)
            except OSError:
                pass
    return out


def main():
    global _MANIFEST
    ap = argparse.ArgumentParser(description="ai_orz target 精准 GC")
    ap.add_argument("--dry-run", action="store_true", help="只报告，不生成删除清单")
    ap.add_argument("--keep-days", type=float, default=7,
                    help="时间窗口内被使用过的指纹全部保留（默认 7 天）")
    ap.add_argument("--keep-per-crate", type=int, default=2,
                    help="每个 crate 额外保留最近几份历史指纹（默认 2）")
    ap.add_argument("--force-oneoff", action="store_true",
                    help="无条件删除 llvm-cov-target / tarpaulin / dx")
    ap.add_argument("--safe", action="store_true",
                    help="安全模式（编译前钩子用）：不删 deps 产物与 build 目录，只清 rcgu.o / old 测试二进制 / incremental / 闲置 target")
    ap.add_argument("--manifest", default=DEFAULT_MANIFEST,
                    help="NUL 分隔的删除清单路径（由 clean_target.sh 批量消费）")
    args = ap.parse_args()

    if not os.path.isdir(TARGET):
        print("target/ 不存在，无需清理")
        return 0

    per_crate = max(1, args.keep_per_crate)
    cutoff = time.time() - max(0.0, args.keep_days) * 86400
    stats = Stats()
    started = time.time()

    if not args.dry_run:
        _MANIFEST = open(args.manifest, "w", encoding="utf-8", newline="")
    try:
        for profile in find_profiles(TARGET):
            fp_times, live = collect_fingerprints(os.path.join(profile, ".fingerprint"))
            deps = os.path.join(profile, "deps")
            if os.path.isdir(deps):
                gc_deps(deps, fp_times, cutoff, per_crate, stats, args.dry_run, live, args.safe)
            build = os.path.join(profile, "build")
            if os.path.isdir(build) and not args.safe:
                gc_build(build, fp_times, cutoff, per_crate, stats, args.dry_run, live)
            inc = os.path.join(profile, "incremental")
            if os.path.isdir(inc):
                gc_incremental(inc, stats, args.dry_run)
        gc_oneoff(TARGET, cutoff, stats, args.dry_run, args.force_oneoff)
    finally:
        if _MANIFEST is not None:
            _MANIFEST.close()
            _MANIFEST = None

    G = 1073741824.0
    total_size = sum(v[0] for v in stats.by_cat.values())
    total_count = sum(v[1] for v in stats.by_cat.values())
    prefix = "[dry-run] " if args.dry_run else ""
    print("%starget GC: 待回收 %.2f GiB / %d 个文件；保留热缓存 %.2f GiB / %d 个文件" % (
        prefix, total_size / G, total_count, stats.kept[0] / G, stats.kept[1]))
    for cat, (size, count) in sorted(stats.by_cat.items(), key=lambda kv: -kv[1][0]):
        if count:
            print("  %-32s %9.2f GiB  %8d 个" % (cat, size / G, count))
    if not args.dry_run:
        print("  删除清单 %d 条 → %s" % (stats.paths, args.manifest))
    print("  %-32s %.1f 秒" % ("扫描耗时", time.time() - started))
    return 0


if __name__ == "__main__":
    sys.exit(main())
