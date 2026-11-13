//! Handler: PUT /api/v1/model-providers/{id} - Update model provider configuration

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateModelProviderRequest, UpdateModelProviderResponse};
use common::enums::ModelProviderStatus;
use common::error::Result;

use crate::enrich_ctx;

/// Update an existing model provider configuration (name, credentials, model name)
#[register_handler_tool(
    id = "update_model_provider",
    name = "Update Provider Config",
    description = "Update a model provider's name, model_name, api_key, base_url, status, or context lengths; only provided fields change. base_url cannot be cleared (except FastEmbed local embedding): the backend has no default domain, so an empty value makes every request fail. For chat (non-embedding) models max_context_length cannot be cleared (it backs the context-compaction threshold). Editing config never triggers a vector rebuild; to change the active embedding model, switch embedding providers explicitly (that path rebuilds vectors). Fails if not found.",
    params = "common::api::UpdateModelProviderRequest",
    tags = "admin"
)]
#[generate_http_handler]
pub async fn update_model_provider(
    ctx: RequestContext,
    params: UpdateModelProviderRequest,
) -> Result<UpdateModelProviderResponse> {
    let mut provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("ModelProvider {} not found", params.id))
        })?;

    let ctx = enrich_ctx!(&ctx, &provider);

    // 编辑配置**不触发**向量重建：向量空间的重建只由显式切换 Embedding Provider
    // （switch handler）负责。编辑 model_name 也可能只是同一个模型的别名/版本写法，
    // 按字段变化就全量重建会产生大量无意义的重建任务。

    // Update fields
    // 记录本次请求是否真的动了 Base URL / provider_type：只有这类改动才需要校验
    // base_url 非空，避免存量「无 base_url」的 provider 连禁用/改名都被挡住。
    let base_url_touched = params.base_url.is_some();
    let provider_type_changed = params.provider_type.is_some();
    if let Some(name) = params.name {
        provider.po.name = name;
    }
    if let Some(provider_type) = params.provider_type {
        provider.po.provider_type = provider_type;
    }
    if let Some(model_name) = params.model_name {
        provider.po.model_name = model_name;
    }
    if let Some(api_key) = params.api_key {
        provider.po.api_key = api_key;
    }
    if let Some(base_url) = params.base_url {
        provider.po.base_url = Some(base_url);
    }
    if let Some(description) = params.description {
        provider.po.description = Some(description);
    }
    if let Some(status) = params.status {
        provider.po.status = ModelProviderStatus::from_i32(status);
    }
    // 对话类模型的上下文长度**不允许清除**（与创建时必填同源）：
    // 清成 None 会让 ContextOverflowPolicy 退化为 threshold=0（恒不命中），
    // Agent 永不压缩 —— 这条静默降级路径必须在入口挡住。
    // Embedding 无对话上下文概念，允许清除。
    if !provider.po.capability.is_embedding() && params.max_context_length.is_some_and(|v| v <= 0) {
        return Err(common::error::Error::bad_request(
            "max_context_length 不能清除：对话类模型必须保留上下文窗口长度，\
             否则压缩触发阈值失效，Agent 将永远不会压缩上下文",
        ));
    }

    // Base URL **不允许清空**（与创建时必填同源，FastEmbed 纯本地嵌入除外）：
    // 出站解析已无默认域名兜底，空值会让该 provider 的所有请求在运行时失败。
    // 仅当本次请求触碰 base_url 或 provider_type 时校验——否则存量空 base_url 的
    // provider 连「禁用/改名」这类无关操作都会被误挡（禁用是故障时的紧急动作）。
    if (base_url_touched || provider_type_changed)
        && provider.po.provider_type != common::enums::ProviderType::FastEmbed
        && provider
            .po
            .base_url
            .as_deref()
            .is_none_or(|v| v.trim().is_empty())
    {
        return Err(common::error::Error::bad_request(
            "base_url 不能为空：不同付费渠道/代理网关的域名不同，\
             请显式填写模型服务根地址（如 https://<你的网关域名>/v1）",
        ));
    }

    // 上下文长度配置 partial update：None 不修改，Some(0) 清除，Some(n>0) 设置
    if params.max_context_length.is_some() || params.recommended_context_length.is_some() {
        provider.po.update_config(|cfg| {
            if let Some(v) = params.max_context_length {
                cfg.max_context_length = if v > 0 { Some(v) } else { None };
            }
            if let Some(v) = params.recommended_context_length {
                cfg.recommended_context_length = if v > 0 { Some(v) } else { None };
            }
        });
    }
    // 访问模式 partial update（方案 §2.2：None=不变更，无清除语义）
    if let Some(mode) = params.access_mode {
        provider.po.update_config(|cfg| {
            cfg.access_mode = Some(mode);
        });
    }
    // 批5：vision 能力位 partial update（None=不变更；编辑 UI 恒传当前选值）
    if let Some(vision) = params.supports_vision {
        provider.po.update_config(|cfg| {
            cfg.supports_vision = Some(vision);
        });
    }
    // 协议口味 partial update（None=不变更；编辑 UI 恒传当前选值）
    if let Some(flavor) = params.api_flavor {
        provider.po.update_config(|cfg| {
            cfg.api_flavor = Some(flavor);
        });
    }
    // 思考强度**整体替换**（None=不启用）：本字段要能清空回退到「不下发」，
    // 无法沿用其余字段的 partial update 语义；编辑 UI 恒传当前选值。
    provider.po.update_config(|cfg| {
        cfg.thinking_intensity = params.thinking_intensity;
    });
    // Update modified_by and updated_at
    provider.po.modified_by = ctx.uid();
    provider.po.updated_at = common::constants::utils::current_timestamp_ms();

    domain()
        .model_provider_manage()
        .update_model_provider(ctx.clone(), &provider)
        .await?;

    let config = provider.po.config();
    Ok(UpdateModelProviderResponse {
        id: provider.po.id.clone(),
        name: provider.po.name.clone(),
        provider_type: provider.po.provider_type,
        capability: provider.po.capability,
        model_name: provider.po.model_name.clone(),
        base_url: if provider.po.base_url.as_ref().is_none_or(|d| d.is_empty()) {
            None
        } else {
            provider.po.base_url.clone()
        },
        description: if provider
            .po
            .description
            .as_ref()
            .is_none_or(|d| d.is_empty())
        {
            None
        } else {
            provider.po.description.clone()
        },
        status: provider.po.status as i32,
        updated_at: provider.po.updated_at,
        max_context_length: config.max_context_length,
        recommended_context_length: config.recommended_context_length,
    })
}
