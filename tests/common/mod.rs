//! Shared test infrastructure for HTTP API integration tests.
//!
//! Provides:
//! - `init_full_test_env` — full DAO/DAL/Domain initialization (extracted from a2a pattern)
//! - `TestApp` — wraps `axum::Router` with typed HTTP request helpers
//! - `factories` — test data factories returning business entities
//! - `assertions` — common API response assertions

pub mod app;
pub mod assertions;
pub mod env;
pub mod factories;

pub use app::TestApp;
pub use assertions::{assert_api_error, assert_api_ok};
pub use env::init_full_test_env;
