---
kind: design
name: HTTP 工具创建表单限定方法白名单为 GET/POST
source: session
category: adr
---

# HTTP 工具创建表单限定方法白名单为 GET/POST

_来源：eb51721 → 8be1663 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
前端 HTTP 工具创建表单的 method 下拉需要限制可选值，避免用户随意提交不受支持的 HTTP 方法。

## 决策驱动
- 安全性
- 实现成本
- 渐进式扩展

## 备选方案
- **仅允许 GET/POST** — 优点：覆盖绝大多数 HTTP 工具场景；实现简单；降低误用风险；缺点：PUT/PATCH/DELETE 等暂不可用
- **开放全部 HTTP 方法** _（已否决）_ — 优点：灵活性高；缺点：后端可能不支持所有方法；增加校验与安全风险

## 决策
将 HTTP 工具表单的 method 字段限制为 GET/POST 两项，PUT/PATCH/DELETE 另行安排支持。

## 影响
当前只能创建 GET/POST 类型的 HTTP 工具；后续扩展需同步更新表单、后端校验及测试。