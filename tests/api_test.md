# ai_orz API 集成测试

## 环境
- 服务地址: http://localhost:3000
- 请求 Header:
  - `X-User-Id: test-admin`
  - `X-User-Name: 测试管理员`

---

## 测试用例 1: 创建 Agent
```bash
curl -X POST http://localhost:3000/api/v1/agents \
  -H "Content-Type: application/json" \
  -H "X-User-Id: test-admin" \
  -d '{
    "name": "CodeAssistant",
    "role": "worker",
    "capabilities": ["coding", "debugging"],
    "soul": "I am a helpful coding assistant"
  }'
```

## 测试用例 2: 获取 Agent 列表
```bash
curl -X GET http://localhost:3000/api/v1/agents \
  -H "X-User-Id: test-admin"
```

## 测试用例 3: 获取单个 Agent (需要替换 id)
```bash
curl -X GET http://localhost:3000/api/v1/agents/<AGENT_ID> \
  -H "X-User-Id: test-admin"
```

## 测试用例 4: 更新 Agent
```bash
curl -X PUT http://localhost:3000/api/v1/agents/<AGENT_ID> \
  -H "Content-Type: application/json" \
  -H "X-User-Id: test-admin" \
  -d '{
    "name": "CodeAssistant Pro",
    "role": "senior-worker",
    "capabilities": ["coding", "debugging", "architecture"],
    "soul": "I am an advanced coding assistant"
  }'
```

## 测试用例 5: 删除 Agent
```bash
curl -X DELETE http://localhost:3000/api/v1/agents/<AGENT_ID> \
  -H "X-User-Id: test-admin"
```

## 健康检查
```bash
curl -X GET http://localhost:3000/health
```
