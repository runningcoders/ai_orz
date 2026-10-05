#!/usr/bin/env python3
"""把 Rust 文件末尾的内联 #[cfg(test)] mod tests 拆到独立文件。

设计要点（踩过的坑都固化在这里）：
1. 用 `#[path = "xxx_tests.rs"] mod tests;` 而非在 mod.rs 里注册——保住
   `mod tests` 的模块层级，测试里 `use super::*` 仍能看到父模块的私有
   use 导入与私有 helper。仓库先例：models/message_channel.rs、
   pkg/tool_registry/{http,mcp,shell_tool,shell_env}.rs。
2. 只支持「末尾唯一一个顶层 #[cfg(test)]」，遇到多个（如 awakening.rs 的
   tests + vision_resource_tests）直接拒绝并提示，避免静默丢测试。
3. doc comment 里的反引号用 chr(96) 拼接，否则 python 会报
   `SyntaxWarning: invalid escape sequence` 并把反斜杠原样写进文件。
4. 拆完源文件末尾补三行声明；测试体整体去一层 4 空格缩进。

用法：
    python3 tools/split_inline_tests.py <file.rs> [--check]
    # 不加 --check 就直接改写文件；加 --check 只报告可拆分行数
"""

import argparse
import os
import re
import sys

BT = chr(96)  # 反引号，避免 python 字符串转义问题


def find_test_module(lines):
    """返回 (cfg_index, body_start, body_end) —— 末尾唯一一个顶层内联测试模块。"""
    cands = [
        i
        for i, l in enumerate(lines)
        if l.strip() == "#[cfg(test)]" and not l.startswith((" ", "\t"))
    ]
    if not cands:
        return None
    if len(cands) > 1:
        got = [(i + 1, lines[i + 1].strip()) for i in cands]
        raise SystemExit(
            "发现多个顶层 #[cfg(test)]："
            + ", ".join(f"L{ln} {name}" for ln, name in got)
            + "\n→ 本脚本只处理单个；多个请手工拆（先拆末尾的，再重跑）"
        )
    i = cands[-1]
    if i + 1 >= len(lines) or not lines[i + 1].strip().startswith("mod "):
        raise SystemExit(f"L{i+1} 的 #[cfg(test)] 后面不是 mod 声明，手工处理")
    name = re.match(r"mod\s+(\w+)", lines[i + 1].strip()).group(1)

    # 找末尾非空行，必须是配平的 '}'
    end = len(lines) - 1
    while end > 0 and not lines[end].strip():
        end -= 1
    if lines[end].strip() != "}":
        raise SystemExit(f"文件末行是 {lines[end].strip()!r}，不是配平的 '}}'，手工处理")

    # 花括号配平（跳过字符串内的括号：实测对源码注释/字符串够用）
    depth = 0
    for k in range(i + 1, end + 1):
        for ch in lines[k]:
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
        if depth == 0 and k > i + 1:
            return i, i + 2, k, name
    raise SystemExit("花括号未配平，手工处理")


def dedent(body):
    """去一层缩进。

    不能只按「以 4 空格开头」逐行判断——`r#"..."#` 原始字符串里可能有多行
    中文（缩进不规则），按行判定会误报「意外缩进」。这里改用「本行缩进 < 4
    空格 ⇒ 顶格结束」与「多数行的前缀」共同确定基准缩进：

    - 基准 = 除空行外，缩进最小的非零行的宽度（正常就是 4）
    - 缩进 >= 基准的行去掉基准个前导空格（保留多行字符串内部的额外缩进）
    - 缩进 < 基准的行（顶格）原样保留
    """
    nonzero = [
        len(l) - len(l.lstrip(" "))
        for l in body
        if l.strip() and (len(l) - len(l.lstrip(" "))) > 0
    ]
    base = min(nonzero) if nonzero else 4
    out = []
    for l in body:
        if not l.strip():
            out.append(l)
            continue
        indent = len(l) - len(l.lstrip(" "))
        out.append(l[base:] if indent >= base else l)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("path")
    ap.add_argument("--check", action="store_true", help="只报告，不改文件")
    args = ap.parse_args()

    path = args.path
    lines = open(path, encoding="utf-8").readlines()
    found = find_test_module(lines)
    if not found:
        print(f"✗ {path}: 未找到顶层 #[cfg(test)]")
        return 1

    cfg_i, body_s, body_e, mod_name = found
    # body_s..body_e-1 = mod 声明行之后到闭合括号之前（不含）
    # ⚠️ body_e 那行是 **mod tests 自己的闭合括号**，它要变成独立文件里的
    # `mod tests;` 声明，不能进测试文件（否则多一个 `}` → E0xx 编译失败）
    body = dedent(lines[body_s:body_e])

    # 自检：切片必须括号配平（净深度 0），否则说明定位偏了，宁可报错不写坏文件
    depth = 0
    for l in body:
        for ch in l:
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
    if depth != 0:
        raise SystemExit(f"测试体括号未配平（净深度 {depth}），定位可能偏了，请手工检查")
    base = os.path.basename(path)
    # `mod.rs` 拆出的测试不能叫 `mod_tests.rs`（mod_tests 是 cargo 集成测试的
    # 命名习惯，且文件名对不上被测模块）→ 用父目录名兜底
    stem = base[:-3] if base.endswith(".rs") else base
    if stem == "mod":
        stem = os.path.basename(os.path.dirname(path)) or "module"
    target = f"{stem}_tests.rs"
    target_path = os.path.join(os.path.dirname(path), target)

    print(f"{path}")
    print(f"  内联模块: mod {mod_name} (L{cfg_i+1}-L{body_e+1})")
    print(f"  源文件: {len(lines)} → {cfg_i + 1 + 3} 行")
    print(f"  新文件: {target} ({len(body)} 行)")

    if os.path.exists(target_path):
        raise SystemExit(f"目标文件已存在：{target_path}（请手工确认）")
    if args.check:
        return 0

    header = (
        f"//! {mod_name} 单元测试（拆分自 {os.path.basename(path)}）\n"
        f"//!\n"
        f"//! 文件瘦身：原 {len(lines)} 行 → {cfg_i + 1 + 3} 行，测试体 {len(body)} 行。\n"
        f"//! 用 " + BT + "#[path]" + BT + " 而非 mod.rs 注册：保住 " + BT + "mod tests" + BT + " 层级，\n"
        f"//! 测试里 " + BT + "use super::*" + BT + " 仍能看到父模块的私有 use 与私有 helper。\n"
        f"//!\n"
        f"//! 拆分命令见 " + BT + "tools/split_inline_tests.py" + BT + "。\n\n"
    )
    open(target_path, "w", encoding="utf-8").write(header + "".join(body))

    new = lines[:cfg_i]
    while new and not new[-1].strip():
        new.pop()
    # ⚠️ 元素必须自带 \n —— 否则三行会被 writelines 拼成一行
    # （这正是第一版脚本产出 `}#[cfg(test)]#[path=..]mod tests;` 的原因）
    new += [
        "#[cfg(test)]\n",
        f'#[path = "{target}"]\n',
        f"mod {mod_name};\n",
    ]
    open(path, "w", encoding="utf-8").writelines(new)
    print(f"  ✓ 已写出 {target}，源文件已截断")
    return 0


if __name__ == "__main__":
    sys.exit(main())
