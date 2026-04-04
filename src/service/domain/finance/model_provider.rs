//! Model Provider 管理实现
//!
//! 放在这里保持和 hr/agent.rs 相同的结构
//! 实际业务实现已经在 finance/mod.rs 的 trait default 中完成了

use crate::error::AppError;
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dal::model_provider::ModelProviderDalTrait;
use std::sync::Arc;

// 这里不需要额外代码了，因为所有实现都已经在 finance/mod.rs 中完成
// 保留这个文件只是为了保持目录结构一致

// 这里只需要空文件，因为 trait 实现已经在 mod.rs 中完成
