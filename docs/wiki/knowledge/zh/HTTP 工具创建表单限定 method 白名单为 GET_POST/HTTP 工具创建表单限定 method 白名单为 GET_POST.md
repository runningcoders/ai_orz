---
kind: design
name: HTTP 工具创建表单限定 method 白名单为 GET/POST
source: session
category: adr
scope:
    - 'frontend/src/pages/settings.rs'
source_files:
    - docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/工具管理/HTTP工具创建界面.md
---

# HTTP 工具创建表单限定 method 白名单为 GET/POST

_来源：a756890 → eb51721 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
前端 tools 页面新增创建入口时，需要限制 HTTP 方法以避免误配危险动词；PUT/PATCH/DELETE 尚未在前端有对应配置项。

## 决策驱动
- 安全性
- 表单字段数量控制
- 后续扩展可增量添加

## 备选方案
- **放开所有 HTTP 方法** _（已否决）_ — 优点：灵活性高；缺点：前端无校验，易误配 PUT/DELETE 等危险方法
- **仅允许 GET/POST** — 优点：最小攻击面，覆盖绝大多数 HTTP 工具场景；缺点：PATCH/DELETE 需后续迭代补充

## 决策
表单 method 下拉固定为 GET/POST 两项，PUT/PATCH/DELETE 暂不开放，留待后续另行安排。

## 影响
短期内无法通过该表单创建 PATCH/DELETE 类型的 HTTP 工具，需走其他途径或等待后续版本；降低了误用风险。