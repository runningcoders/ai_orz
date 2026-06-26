//! Error construction helpers.

/// Build a typed [`Error`](crate::error::Error) from an [`ErrorCode`](crate::error::ErrorCode) variant.
#[macro_export]
macro_rules! err {
    // === Special case: inline json field syntax MUST come FIRST to avoid ambiguity ===
    ($variant:ident, $msg:literal, field: { $($k:ident: $v:expr),* $(,)? } $(, $rest:tt)* $(,)?) => {
        {
            let field = serde_json::json!({ $($k: $v),* });
            $crate::err!($variant, $msg, field: field.into() $(, $rest)*)
        }
    };
    ($variant:ident, $msg:literal source: $source:expr $(,)?) => {
        $crate::err!($variant, $msg, source: $source)
    };
    ($variant:ident, $error_type:ident, $msg:literal, field: { $($k:ident: $v:expr),* $(,)? } $(, $rest:tt)* $(,)?) => {
        {
            let field = serde_json::json!({ $($k: $v),* });
            $crate::err!($variant, $error_type, $msg, field: field.into() $(, $rest)*)
        }
    };
    // === Special case: inline json field + source syntax ===
    ($variant:ident, $msg:literal, field: { $($k:ident: $v:expr),* $(,)? }, source: $source:expr $(,)?) => {
        {
            let field = serde_json::json!({ $($k: $v),* });
            $crate::err!($variant, $msg, field: field.into(), source: $source)
        }
    };
    ($variant:ident, $error_type:ident, $msg:literal, field: { $($k:ident: $v:expr),* $(,)? }, source: $source:expr $(,)?) => {
        {
            let field = serde_json::json!({ $($k: $v),* });
            $crate::err!($variant, $error_type, $msg, field: field.into(), source: $source)
        }
    };
    // === Named parameters: field and source MUST come before positional args to avoid ambiguity ===
    // 1. Variant + message + field (auto .with_field)
    ($variant:ident, $msg:literal, field: $field:expr $(,)?) => {
        $crate::error::Error::new(
            $crate::error::ErrorCode::$variant,
            format!($msg),
        ).with_field($field)
    };
    // 2. Variant + error_type + message + field
    ($variant:ident, $error_type:ident, $msg:literal, field: $field:expr $(,)?) => {
        $crate::error::Error::typed(
            $crate::error::ErrorCode::$variant,
            $crate::error::ErrorType::$error_type,
            format!($msg),
        ).with_field($field)
    };
    // 3. Variant + message + field + source
    ($variant:ident, $msg:literal, field: $field:expr, source: $source:expr $(,)?) => {
        $crate::error::Error::new(
            $crate::error::ErrorCode::$variant,
            format!($msg),
        ).with_field($field).with_source($source.into())
    };
    // 4. Variant + error_type + message + field + source
    ($variant:ident, $error_type:ident, $msg:literal, field: $field:expr, source: $source:expr $(,)?) => {
        $crate::error::Error::typed(
            $crate::error::ErrorCode::$variant,
            $crate::error::ErrorType::$error_type,
            format!($msg),
        ).with_field($field).with_source($source.into())
    };
    // 5. Variant + message + source error (auto .with_source)
    ($variant:ident, $msg:literal, source: $source:expr $(,)?) => {
        $crate::error::Error::new(
            $crate::error::ErrorCode::$variant,
            format!($msg),
        ).with_source($source.into())
    };
    // 6. Variant + error_type + message + source
    ($variant:ident, $error_type:ident, $msg:literal, source: $source:expr $(,)?) => {
        $crate::error::Error::typed(
            $crate::error::ErrorCode::$variant,
            $crate::error::ErrorType::$error_type,
            format!($msg),
        ).with_source($source.into())
    };
    // === Basic cases without named parameters ===
    // 1. Just variant + message (format args with literal format string)
    ($variant:ident, $msg:literal $(, $arg:expr)* $(,)?) => {
        $crate::error::Error::new(
            $crate::error::ErrorCode::$variant,
            format!($msg $(, $arg)*),
        )
    };
    // 1a. Variant + pre-formatted message (msg is String/&str)
    ($variant:ident, $msg:expr $(,)?) => {
        $crate::error::Error::new(
            $crate::error::ErrorCode::$variant,
            Into::<String>::into($msg),
        )
    };
    // 2. Variant + error_type + message (format args)
    ($variant:ident, $error_type:ident, $msg:literal $(, $arg:expr)* $(,)?) => {
        $crate::error::Error::typed(
            $crate::error::ErrorCode::$variant,
            $crate::error::ErrorType::$error_type,
            format!($msg $(, $arg)*),
        )
    };
    // 2a. Variant + error_type + pre-formatted message
    ($variant:ident, $error_type:ident, $msg:expr $(,)?) => {
        $crate::error::Error::typed(
            $crate::error::ErrorCode::$variant,
            $crate::error::ErrorType::$error_type,
            Into::<String>::into($msg),
        )
    };
}

/// Return early with a typed [`Error`](crate::error::Error).
#[macro_export]
macro_rules! bail_err {
    // === Special case: inline json field syntax ===
    ($variant:ident, $msg:literal, field: { $($k:ident: $v:expr),* $(,)? } $(, $rest:tt)* $(,)?) => {
        return Err($crate::err!($variant, $msg, field: { $($k: $v),* } $(, $rest)*));
    };
    ($variant:ident, $msg:literal source: $source:expr $(,)?) => {
        return Err($crate::err!($variant, $msg, source: $source));
    };
    ($variant:ident, $error_type:ident, $msg:literal, field: { $($k:ident: $v:expr),* $(,)? } $(, $rest:tt)* $(,)?) => {
        return Err($crate::err!($variant, $error_type, $msg, field: { $($k: $v),* } $(, $rest)*));
    };
    // === Special case: inline json field + source ===
    ($variant:ident, $msg:literal, field: { $($k:ident: $v:expr),* $(,)? }, source: $source:expr $(,)?) => {
        return Err($crate::err!($variant, $msg, field: { $($k: $v),* }, source: $source));
    };
    ($variant:ident, $error_type:ident, $msg:literal, field: { $($k:ident: $v:expr),* $(,)? }, source: $source:expr $(,)?) => {
        return Err($crate::err!($variant, $error_type, $msg, field: { $($k: $v),* }, source: $source));
    };
    // === Named parameters first ===
    ($variant:ident, $msg:literal, field: $field:expr $(,)?) => {
        return Err($crate::err!($variant, $msg, field: $field));
    };
    ($variant:ident, $error_type:ident, $msg:literal, field: $field:expr $(,)?) => {
        return Err($crate::err!($variant, $error_type, $msg, field: $field));
    };
    ($variant:ident, $msg:literal, field: $field:expr, source: $source:expr $(,)?) => {
        return Err($crate::err!($variant, $msg, field: $field, source: $source));
    };
    ($variant:ident, $error_type:ident, $msg:literal, field: $field:expr, source: $source:expr $(,)?) => {
        return Err($crate::err!($variant, $error_type, $msg, field: $field, source: $source));
    };
    ($variant:ident, $msg:literal, source: $source:expr $(,)?) => {
        return Err($crate::err!($variant, $msg, source: $source));
    };
    ($variant:ident, $error_type:ident, $msg:literal, source: $source:expr $(,)?) => {
        return Err($crate::err!($variant, $error_type, $msg, source: $source));
    };
    // === Basic cases ===
    ($variant:ident, $msg:literal $(, $arg:expr)* $(,)?) => {
        return Err($crate::err!($variant, $msg $(, $arg)*));
    };
    ($variant:ident, $msg:expr $(,)?) => {
        return Err($crate::err!($variant, $msg));
    };
    ($variant:ident, $error_type:ident, $msg:literal $(, $arg:expr)* $(,)?) => {
        return Err($crate::err!($variant, $error_type, $msg $(, $arg)*));
    };
    ($variant:ident, $error_type:ident, $msg:expr $(,)?) => {
        return Err($crate::err!($variant, $error_type, $msg));
    };
}

/// Ensure condition is true, otherwise return error.
#[macro_export]
macro_rules! ensure_err {
    ($cond:expr, $variant:ident, $msg:literal $(, $arg:expr)* $(,)?) => {
        if !$cond {
            $crate::bail_err!($variant, $msg $(, $arg)*);
        }
    };
    ($cond:expr, $variant:ident, $error_type:ident, $msg:literal $(, $arg:expr)* $(,)?) => {
        if !$cond {
            $crate::bail_err!($variant, $error_type, $msg $(, $arg)*);
        }
    };
    ($cond:expr, $variant:ident, $msg:literal $(, $arg:expr)*, source: $source:expr $(,)?) => {
        if !$cond {
            $crate::bail_err!($variant, $msg $(, $arg)*, source: $source);
        }
    };
    ($cond:expr, $variant:ident, $error_type:ident, $msg:literal $(, $arg:expr)*, source: $source:expr $(,)?) => {
        if !$cond {
            $crate::bail_err!($variant, $error_type, $msg $(, $arg)*, source: $source);
        }
    };
    ($cond:expr, $variant:ident, $msg:literal $(, $arg:expr)*, field: $field:expr $(,)?) => {
        if !$cond {
            $crate::bail_err!($variant, $msg $(, $arg)*, field: $field);
        }
    };
    ($cond:expr, $variant:ident, $error_type:ident, $msg:literal $(, $arg:expr)*, field: $field:expr $(,)?) => {
        if !$cond {
            $crate::bail_err!($variant, $error_type, $msg $(, $arg)*, field: $field);
        }
    };
    ($cond:expr, $variant:ident, $msg:literal $(, $arg:expr)*, field: $field:expr, source: $source:expr $(,)?) => {
        if !$cond {
            $crate::bail_err!($variant, $msg $(, $arg)*, field: $field, source: $source);
        }
    };
    ($cond:expr, $variant:ident, $error_type:ident, $msg:literal $(, $arg:expr)*, field: $field:expr, source: $source:expr $(,)?) => {
        if !$cond {
            $crate::bail_err!($variant, $error_type, $msg $(, $arg)*, field: $field, source: $source);
        }
    };
}

/// Define error codes with error type and http status code.
///
/// Generates an enum ErrorCode with all variants and helper methods.
#[macro_export]
macro_rules! define_error_codes {
    (
        $(
            $category:ident {
                $(
                    $variant:ident {
                        type: $type:ident,
                        http: $http:expr,
                        code: $code:literal,
                    }
                )+
            }
        )+
    ) => {
        #[allow(missing_docs)]
        pub mod generated {
            use serde::{Serialize, Deserialize};

            #[allow(missing_docs)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
            pub enum ErrorCode {
                $(
                    $(
                        #[allow(missing_docs)]
                        $variant,
                    )+
                )+
            }

            impl ErrorCode {
                /// Get the error type (broad category).
                pub fn error_type(&self) -> $crate::error::ErrorType {
                    match self {
                        $(
                            $(
                                ErrorCode::$variant => $crate::error::ErrorType::$type,
                            )+
                        )+
                    }
                }

                /// Get the HTTP status code.
                pub fn http_status(&self) -> u16 {
                    match self {
                        $(
                            $(
                                ErrorCode::$variant => $http,
                            )+
                        )+
                    }
                }

                /// Get the machine-readable error code string.
                pub fn code_str(&self) -> &'static str {
                    match self {
                        $(
                            $(
                                ErrorCode::$variant => $code,
                            )+
                        )+
                    }
                }
            }
        }
        pub use generated::*;
    };
}
