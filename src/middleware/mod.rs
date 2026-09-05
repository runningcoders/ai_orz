//! 中间件模块

pub mod a2a_auth;
pub mod api_notice;
pub mod federation_identity;
pub mod jwt_auth;
pub mod request_context;
pub mod require_role;

pub use a2a_auth::a2a_auth_middleware;
pub use api_notice::api_notice_middleware;
pub use federation_identity::{FederationIdentity, resolve_federation_identity};
pub use jwt_auth::jwt_auth_middleware;
pub use request_context::request_context_middleware;
pub use require_role::require_role_middleware;
