# AI Orz 发布包 - 常用命令统一入口
# 用法：make <命令>（make 或 make help 查看全部）
# 所有命令委托给 script/ 下的脚本执行，逻辑只在一处

.DEFAULT_GOAL := help
.PHONY: help start stop restart status logs

help: ## 显示本帮助
	@echo "AI Orz 常用命令："
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}'

start: ## 后台启动服务（浏览器访问 http://localhost:3000）
	./script/start.sh

stop: ## 停止服务
	./script/stop.sh

restart: ## 重启服务
	./script/restart.sh

status: ## 查看服务运行状态
	./script/status.sh

logs: ## 实时查看运行日志
	./script/logs.sh
