//! Model Provider 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod call_model;
pub mod create_model_provider;
pub mod delete_model_provider;
pub mod get_model_provider;
pub mod list_model_providers;
pub mod test_connection;
pub mod update_model_provider;

pub use call_model::call_model_handler;
pub use create_model_provider::create_model_provider_handler;
pub use delete_model_provider::delete_model_provider_handler;
pub use get_model_provider::get_model_provider_handler;
pub use list_model_providers::list_model_providers_handler;
pub use test_connection::test_model_provider_connection_handler;
pub use update_model_provider::update_model_provider_handler;
