use proc_macro::TokenStream;
use quote::quote;
use syn::{AngleBracketedGenericArguments, parse_macro_input};
use syn::{Ident, ItemFn, Lit, LitStr, Meta, MetaNameValue, Type};

/// Derive macro to mark HTTP params and their source locations.
/// This is used together with #[generate_http_handler] to automatically
/// collect which fields come from path/query/body.
#[proc_macro_derive(Params, attributes(param))]
pub fn derive_params(input: TokenStream) -> TokenStream {
    // This derive doesn't generate any code, it just holds the #[param] attributes
    // for #[generate_http_handler] to read.
    TokenStream::new()
}

/// Register a handler function as a built-in tool
///
/// # Usage
/// ```rust
/// use ai_orz_macros::register_handler_tool;
/// use common::api::ListSkillFilesParams;
///
/// async fn list_skill_files_handler(ctx: RequestContext, params: ListSkillFilesParams) -> ::std::result::Result<Value, AppError> {
///     // implementation...
/// }
///
/// #[register_handler_tool(
///     id = "list_skill_files",
///     name = "list_skill_files",
///     description = "List all files in a skill",
///     params = "common::api::ListSkillFilesParams",
/// )]
/// async fn list_skill_files_handler(ctx: RequestContext, params: ListSkillFilesParams) -> ::std::result::Result<Value, AppError> {
///     // implementation...
/// }
/// ```
#[proc_macro_attribute]
pub fn register_handler_tool(args: TokenStream, input: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(input as syn::ItemFn);

    // Parse with a parser that allows trailing commas
    use syn::parse::Parser;
    let mut id = None;
    let mut name = None;
    let mut description = None;
    let mut params_type = None;
    let mut handler_ident: Option<Ident> = None;

    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("id") {
            id = Some(meta.value()?.parse::<LitStr>()?.value());
            Ok(())
        } else if meta.path.is_ident("name") {
            name = Some(meta.value()?.parse::<LitStr>()?.value());
            Ok(())
        } else if meta.path.is_ident("description") {
            description = Some(meta.value()?.parse::<LitStr>()?.value());
            Ok(())
        } else if meta.path.is_ident("params") {
            let s = meta.value()?.parse::<LitStr>()?;
            let ty: Type = syn::parse_str(&s.value()).unwrap();
            params_type = Some(ty);
            Ok(())
        } else {
            Err(meta.error("unexpected argument"))
        }
    });

    parser.parse(args).unwrap();

    fn get_lit_str(expr: &syn::Expr) -> Option<&LitStr> {
        match expr {
            syn::Expr::Lit(syn::ExprLit {
                lit: Lit::Str(s), ..
            }) => Some(s),
            _ => None,
        }
    }

    let id = match id {
        Some(id) => id,
        _ => panic!("Missing `id = \"...\"` attribute"),
    };
    let name = match name {
        Some(name) => name,
        _ => panic!("Missing `name = \"...\"` attribute"),
    };
    let description = match description {
        Some(desc) => desc,
        _ => panic!("Missing `description = \"...\"` attribute"),
    };
    let params_type = match params_type {
        Some(ty) => ty,
        _ => panic!("Missing `params = \"Type\"` attribute"),
    };
    let handler_ident = match handler_ident {
        Some(p) => p.clone(),
        _ => {
            // If not specified, use the function itself
            item_fn.sig.ident.clone()
        }
    };

    // Get the output type from the function signature
    let output = match &item_fn.sig.output {
        syn::ReturnType::Type(_, ty) => ty,
        syn::ReturnType::Default => panic!("Function must return Result<Output, AppError>"),
    };

    // Check if it's Result<Value, AppError> or Result<O, AppError>
    // We need to extract the inner Output type
    let output_ty = extract_output_type(output);
    let is_value_output = is_value_output(output_ty);

    let factory_expanded = if is_value_output {
        quote! {
            Box::new(GenericHandlerFn::new(move |ctx, params| {
                Box::pin(#handler_ident(ctx, params))
            }))
        }
    } else {
        quote! {
            Box::new(GenericTypedHandlerFn::new(move |ctx, params| {
                Box::pin(#handler_ident(ctx, params))
            }))
        }
    };

    // Generate the factory struct and registration
    let factory_ident = syn::Ident::new(
        &format!("{}_FACTORY", id.to_ascii_uppercase()),
        item_fn.sig.ident.span(),
    );

    let expanded = quote! {
        #item_fn

        struct #factory_ident;

        use crate::models::tool::{CoreTool, ToolPo};
        use crate::pkg::tool_registry::BuiltinToolFactory;

        impl Clone for #factory_ident {
            fn clone(&self) -> Self {
                #factory_ident
            }
        }

        impl Copy for #factory_ident {}

        unsafe impl Send for #factory_ident {}
        unsafe impl Sync for #factory_ident {}

        impl BuiltinToolFactory for #factory_ident {
            fn create_po(&self) -> ToolPo {
                use common::enums::tool::{ControlMode, ToolProtocol, ToolStatus};
                let schema = schemars::schema_for!(#params_type);
                let schema_json = serde_json::to_value(&schema).unwrap();
                let mut po = ToolPo::new(
                    #id.to_string(),
                    #name.to_string(),
                    #description.to_string(),
                    ToolProtocol::Builtin,
                    schema_json,
                    None,
                    vec![],
                    None,
                );
                po.status = ToolStatus::Enabled;
                po.control_mode = ControlMode::Auto;
                po
            }

            fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
                use crate::pkg::tool_registry::handler_adapter::*;
                let schema = schemars::schema_for!(#params_type);
                let json = serde_json::to_value(&schema).unwrap();
                let adapter = HandlerToolAdapter::<#params_type>::new(
                    po,
                    json,
                    #factory_expanded,
                );
                Box::new(adapter)
            }
        }

        // Force registration at startup
        const _: () = {
            #[ctor::ctor]
            fn register() {
                let factory = Box::new(#factory_ident);
                crate::pkg::tool_registry::get_registry()
                    .register_builtin_factory(factory);
            }
        };
    };

    expanded.into()
}

fn extract_output_type(ty: &Type) -> &Type {
    // We expect: Result<Output, AppError> (old) or Result<Output> (new with common::error::Result type alias)
    match ty {
        Type::Path(type_path) => {
            if let Some(last_segment) = type_path.path.segments.last() {
                match &last_segment.arguments {
                    syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
                        args,
                        ..
                    }) => {
                        // If 1 argument: it's Result<Output> -> return the only argument
                        // If 2 arguments: it's Result<Output, AppError> -> return the first argument
                        if let Some(syn::GenericArgument::Type(output)) = args.first() {
                            return output;
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    panic!("Expected return type to be Result<Output> or Result<Output, AppError>");
}

fn is_value_output(ty: &Type) -> bool {
    // Check if it's serde_json::Value
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if segments.len() == 1 && segments[0] == "Value" {
                return true;
            }
            if segments.len() == 2 && segments[0] == "serde_json" && segments[1] == "Value" {
                return true;
            }
            false
        }
        _ => false,
    }
}

/// Generate axum HTTP handler from a core handler function
///
/// # Usage
/// ```rust
/// use ai_orz_macros::{register_handler_tool, generate_http_handler};
/// use common::api::ListSkillFilesParams;
/// use crate::error::AppError;
/// use crate::pkg::RequestContext;
///
/// #[register_handler_tool(...)]
/// #[generate_http_handler]
/// pub async fn list_skill_files(
///     ctx: RequestContext,
///     params: ListSkillFilesParams,
/// ) -> ::std::result::Result<ListSkillFilesResponse, AppError> {
///     // implementation...
/// }
/// ```
///
/// The generated handler will be named `{function_name}_handler`.
/// Path fields are automatically detected from `#[path]` attribute on the
/// parameter struct fields.
#[proc_macro_attribute]
pub fn generate_http_handler(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(input as ItemFn);

    // Extract information from the function
    let core_ident = &item_fn.sig.ident;
    let handler_ident = syn::Ident::new(&format!("{}_handler", core_ident), core_ident.span());

    // Get the params type from the function signature
    // Expected: (ctx: RequestContext, params: Params)
    let params_ty = if item_fn.sig.inputs.len() == 2 {
        match item_fn.sig.inputs[1] {
            syn::FnArg::Typed(ref pat_ty) => &pat_ty.ty,
            _ => panic!("Expected second parameter to be params: Params"),
        }
    } else {
        panic!(
            "Expected function signature: async fn name(ctx: RequestContext, params: Params) -> ::std::result::Result<Output, AppError>"
        );
    };

    // Get the output type
    let output_ty = match &item_fn.sig.output {
        syn::ReturnType::Type(_, ty) => extract_output_type(ty),
        syn::ReturnType::Default => panic!("Function must return Result<Output, AppError>"),
    };

    // Get the type path to the params struct
    let params_ty_path = match &**params_ty {
        Type::Path(type_path) => type_path.path.clone(),
        _ => panic!("Params must be a named struct type"),
    };

    // Collect #[path_param] and #[query_param] annotations from the struct definition
    let (path_fields, query_fields) = collect_path_and_query_fields_from_type(params_ty_path);
    let path_idents: Vec<Ident> = path_fields.iter().map(|(ident, _)| ident.clone()).collect();
    let path_types: Vec<syn::Type> = path_fields.iter().map(|(_, ty)| ty.clone()).collect();
    let has_path = !path_idents.is_empty();
    let has_query = !query_fields.is_empty();
    let query_idents: Vec<Ident> = query_fields
        .iter()
        .map(|(ident, _)| ident.clone())
        .collect();

    // Generate the handler code
    let expanded = match (has_path, has_query) {
        (true, true) => {
            let path_tuple = quote! {
                ( #( #path_idents, )* )
            };
            let path_ty_tuple = quote! {
                ( #( #path_types, )* )
            };
            let assign_paths = quote! {
                #(
                    params.#path_idents = #path_idents;
                )*
            };
            let assign_queries = quote! {
                #(
                    params.#query_idents = query.#query_idents;
                )*
            };

            quote! {
                #item_fn

                pub async fn #handler_ident(
                    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                    axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                    axum::extract::Query(query): axum::extract::Query<#params_ty>,
                    axum::Json(mut params): axum::Json<#params_ty>,
                ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                    // Priority: path > query > body, so assign in that order
                    #assign_queries
                    #assign_paths
                    let result = #core_ident(ctx, params).await?;
                    Ok(axum::Json(common::api::ApiResponse::success(result)))
                }
            }
        }
        (true, false) => {
            let path_tuple = quote! {
                ( #( #path_idents, )* )
            };
            let path_ty_tuple = quote! {
                ( #( #path_types, )* )
            };
            let assign_paths = quote! {
                #(
                    params.#path_idents = #path_idents;
                )*
            };

            quote! {
                #item_fn

                pub async fn #handler_ident(
                    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                    axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                    axum::Json(mut params): axum::Json<#params_ty>,
                ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                    #assign_paths
                    let result = #core_ident(ctx, params).await?;
                    Ok(axum::Json(common::api::ApiResponse::success(result)))
                }
            }
        }
        (false, true) => {
            quote! {
                #item_fn

                pub async fn #handler_ident(
                    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                    axum::extract::Query(params): axum::extract::Query<#params_ty>,
                ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                    let result = #core_ident(ctx, params).await?;
                    Ok(axum::Json(common::api::ApiResponse::success(result)))
                }
            }
        }
        (false, false) => {
            // No path, no query - all in body
            quote! {
                #item_fn

                pub async fn #handler_ident(
                    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                    axum::Json(params): axum::Json<#params_ty>,
                ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                    let result = #core_ident(ctx, params).await?;
                    Ok(axum::Json(common::api::ApiResponse::success(result)))
                }
            }
        }
    };

    expanded.into()
}
/// Collect field ids that have #[param(source = "path")] or #[param(source = "query")] attribute by parsing the source file
fn collect_path_and_query_fields_from_type(
    path: syn::Path,
) -> (Vec<(Ident, Type)>, Vec<(Ident, Type)>) {
    // Get the last segment (the type name)
    let type_name = path.segments.last().unwrap().ident.to_string();

    // Get the project root from current environment
    let workspace_root = std::env::var("CARGO_WORKSPACE_DIR").unwrap_or_else(|_| ".".to_string());

    // Search for the file that contains this type
    // common/src/api/**/*.rs
    let pattern = format!("{}/**/*.rs", workspace_root);

    for entry in glob::glob(&pattern).unwrap() {
        if let Ok(path) = entry {
            let content = std::fs::read_to_string(path).ok();
            if let Some(content) = content {
                let syntax = syn::parse_file(&content).ok();
                if let Some(syntax) = syntax {
                    for item in &syntax.items {
                        if let syn::Item::Struct(item_struct) = item {
                            if item_struct.ident.to_string() == type_name {
                                // Found it! Collect fields with #[param(source = "...")] attribute
                                let mut path_fields = Vec::new();
                                let mut query_fields = Vec::new();
                                for field in &item_struct.fields {
                                    for attr in &field.attrs {
                                        if attr.path().is_ident("param") {
                                            // Parse #[param(source = "path")]
                                            if let Meta::NameValue(MetaNameValue {
                                                path: _,
                                                value,
                                                ..
                                            }) = &attr.meta
                                            {
                                                if let Ok(lit_str) = match value {
                                                    syn::Expr::Lit(syn::ExprLit {
                                                        lit: Lit::Str(s),
                                                        ..
                                                    }) => Ok(s),
                                                    _ => Err(()),
                                                } {
                                                    match lit_str.value().as_str() {
                                                        "path" => {
                                                            if let Some(ident) = &field.ident {
                                                                path_fields.push((
                                                                    ident.clone(),
                                                                    field.ty.clone(),
                                                                ));
                                                            }
                                                        }
                                                        "query" => {
                                                            if let Some(ident) = &field.ident {
                                                                query_fields.push((
                                                                    ident.clone(),
                                                                    field.ty.clone(),
                                                                ));
                                                            }
                                                        }
                                                        "body" => {
                                                            // body is default, no need to collect
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                return (path_fields, query_fields);
                            }
                        }
                    }
                }
            }
        }
    }

    // If not found, panic
    panic!("Could not find struct type {} in source files", type_name);
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i != 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}
