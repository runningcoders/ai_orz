//! #[derive(LogFields)] 过程宏实现
//!
//! 扫描结构体中标注了 `#[log_field]` 的字段，自动生成 `LogFields` trait 实现。
//! 生成的 `create_log_span` 方法会创建包含所有标注字段的 tracing span。
//!
//! 类型处理：
//! - `Option<String>` → `self.field.as_deref().unwrap_or("")`
//! - `String` → `self.field.as_str()`
//! - 其他 → `&self.field`（依赖 Display trait）

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// 判断字段类型是否是 `Option<String>`
fn is_option_string(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            if seg.ident == "Option" {
                return true;
            }
        }
    }
    false
}

/// 判断字段类型是否是 `String`
fn is_string(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            return seg.ident == "String";
        }
    }
    false
}

/// 实际实现逻辑
pub fn derive_log_fields(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let mut span_fields = Vec::new();

    if let syn::Data::Struct(s) = input.data {
        for field in &s.fields {
            let has_log_field = field.attrs.iter().any(|attr| attr.path().is_ident("log_field"));
            if !has_log_field {
                continue;
            }
            if let Some(ident) = &field.ident {
                let field_name = ident.to_string();
                let ty = &field.ty;
                if is_option_string(ty) {
                    span_fields.push(quote! {
                        #field_name = %self.#ident.as_deref().unwrap_or("")
                    });
                } else if is_string(ty) {
                    span_fields.push(quote! {
                        #field_name = %self.#ident.as_str()
                    });
                } else {
                    span_fields.push(quote! {
                        #field_name = %self.#ident
                    });
                }
            }
        }
    }

    let expanded = quote! {
        impl crate::pkg::logging::LogFields for #name {
            fn create_log_span(&self, operation: &str, level: tracing::Level) -> tracing::Span {
                match level {
                    tracing::Level::ERROR => tracing::error_span!(
                        "request",
                        #(#span_fields,)*
                        operation = %operation
                    ),
                    tracing::Level::WARN => tracing::warn_span!(
                        "request",
                        #(#span_fields,)*
                        operation = %operation
                    ),
                    tracing::Level::INFO => tracing::info_span!(
                        "request",
                        #(#span_fields,)*
                        operation = %operation
                    ),
                    tracing::Level::DEBUG => tracing::debug_span!(
                        "request",
                        #(#span_fields,)*
                        operation = %operation
                    ),
                    tracing::Level::TRACE => tracing::trace_span!(
                        "request",
                        #(#span_fields,)*
                        operation = %operation
                    ),
                }
            }
        }
    };

    expanded.into()
}
