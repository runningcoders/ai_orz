//! Model Provider 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod create_model_provider;
pub mod delete_model_provider;
pub mod get_model_provider;
pub mod list_model_providers;
pub mod update_model_provider;
pub mod test_connection;
pub mod call_model;

pub use create_model_provider::{create_model_provider, CreateModelProviderRequest, CreateModelProviderResponse};
pub use delete_model_provider::delete_model_provider;
pub use get_model_provider::{get_model_provider, GetModelProviderResponse};
pub use list_model_providers::{list_model_providers, ModelProviderListItem};
pub use update_model_provider::{update_model_provider, UpdateModelProviderRequest, UpdateModelProviderResponse};
pub use test_connection::{test_model_provider_connection, TestModelProviderConnectionRequest, TestModelProviderConnectionResponse};
pub use call_model::{call_model, CallModelRequest, CallModelResponse};
