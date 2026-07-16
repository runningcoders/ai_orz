//! ErrorCode definition using the define_error_codes! macro.

use super::define_error_codes;

define_error_codes! {
    general {
        InvalidRequest {
            type: Validation,
            http: 400,
            code: "invalid_request",
        }
        Unauthorized {
            type: Auth,
            http: 401,
            code: "unauthorized",
        }
        Forbidden {
            type: Permission,
            http: 403,
            code: "forbidden",
        }
        ResourceNotFound {
            type: Biz,
            http: 404,
            code: "resource_not_found",
        }
        NotFound {
            type: Biz,
            http: 404,
            code: "not_found",
        }
        ResourceConflict {
            type: Biz,
            http: 409,
            code: "resource_conflict",
        }
        Conflict {
            type: Biz,
            http: 409,
            code: "conflict",
        }
        PayloadTooLarge {
            type: Validation,
            http: 413,
            code: "payload_too_large",
        }
        InvalidToken {
            type: Auth,
            http: 401,
            code: "invalid_token",
        }
        DbQueryFailed {
            type: Db,
            http: 500,
            code: "db_query_failed",
        }
        DbMigrationFailed {
            type: Db,
            http: 500,
            code: "db_migration_failed",
        }
        IoError {
            type: Io,
            http: 500,
            code: "io_error",
        }
        ThirdPartyUnavailable {
            type: Third,
            http: 502,
            code: "third_party_unavailable",
        }
        ThirdPartyError {
            type: Third,
            http: 500,
            code: "third_party_error",
        }
        ToolAutoModeNotSupported {
            type: Tool,
            http: 400,
            code: "tool_auto_mode_not_supported",
        }
        ToolParameterInvalid {
            type: Tool,
            http: 400,
            code: "tool_parameter_invalid",
        }
        ToolExecutionFailed {
            type: Tool,
            http: 500,
            code: "tool_execution_failed",
        }
        NetworkError {
            type: Network,
            http: 503,
            code: "network_error",
        }
        RuntimeAwakenFailed {
            type: Runtime,
            http: 500,
            code: "runtime_awaken_failed",
        }
        ConfigMissing {
            type: Config,
            http: 500,
            code: "config_missing",
        }
        ConfigInvalid {
            type: Config,
            http: 500,
            code: "config_invalid",
        }
        UnsupportedOperation {
            type: Biz,
            http: 400,
            code: "unsupported_operation",
        }
        ChannelPushFailed {
            type: Runtime,
            http: 500,
            code: "channel_push_failed",
        }
        JwtInvalid {
            type: Auth,
            http: 401,
            code: "jwt_invalid",
        }
        EmbeddingProviderSwitchRequired {
            type: Biz,
            http: 409,
            code: "embedding_provider_switch_required",
        }
        RebuildInProgress {
            type: Biz,
            http: 409,
            code: "rebuild_in_progress",
        }
        Internal {
            type: System,
            http: 500,
            code: "internal",
        }
    }
}

pub use generated::ErrorCode;