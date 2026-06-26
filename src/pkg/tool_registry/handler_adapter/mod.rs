//! Handler to CoreTool adapter - adapt existing axum Handler to CoreTool
//!
//! This module provides:
//! 1. `HandlerToolAdapter` - adapts a handler function to CoreTool trait
//! 2. `register_handler_tool!` macro - register a handler as a built-in tool
//!
//! Design idea:
//! - Existing handlers already have complete permission checking, parameter parsing,
//!   error handling, business logic. We can reuse them directly as tools.
//! - LLM can call the same behavior that user can call via HTTP API.
//! - This keeps behavior consistency between user and LLM calls.

pub mod macros;

use common::error::{Error, Result};
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use async_trait::async_trait;
use dyn_clone::DynClone;
use futures_util::Future;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::marker::PhantomData;
use std::pin::Pin;

/// Trait for handler functions that can be adapted to CoreTool
///
/// This trait is implemented for handler functions that:
/// - Take `RequestContext` + parsed parameters
/// - Return `Result<T>`
/// - `T` is `Serialize` for JSON response
#[async_trait]
pub trait HandlerFn<Params>: Send + Sync + DynClone
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    async fn call(&self, ctx: RequestContext, params: Params) -> Result<Value>;
}

// Clone implementation for boxed HandlerFn
dyn_clone::clone_trait_object!(<Params> HandlerFn<Params> where Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static);

/// Generic implementation for any Fn that matches the signature
#[derive(Clone)]
pub struct GenericHandlerFn<Params, F>
where
    F: Fn(RequestContext, Params) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    f: F,
    _phantom: PhantomData<Params>,
}

impl<Params, F> GenericHandlerFn<Params, F>
where
    F: Fn(RequestContext, Params) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<Params, F> HandlerFn<Params> for GenericHandlerFn<Params, F>
where
    F: Fn(RequestContext, Params) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    async fn call(&self, ctx: RequestContext, params: Params) -> Result<Value> {
        (self.f)(ctx, params).await
    }
}

/// Specialization for functions that return a concrete type (not Value)
/// The macro will automatically wrap it to serialize the result to Value
#[derive(Clone)]
pub struct GenericTypedHandlerFn<Params, Output, F>
where
    F: Fn(RequestContext, Params) -> Pin<Box<dyn Future<Output = Result<Output>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
    Output: Serialize + Send + Sync + Clone + 'static,
{
    f: F,
    _phantom: PhantomData<(Params, Output)>,
}

impl<Params, Output, F> GenericTypedHandlerFn<Params, Output, F>
where
    F: Fn(RequestContext, Params) -> Pin<Box<dyn Future<Output = Result<Output>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
    Output: Serialize + Send + Sync + Clone + 'static,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<Params, Output, F> HandlerFn<Params> for GenericTypedHandlerFn<Params, Output, F>
where
    F: Fn(RequestContext, Params) -> Pin<Box<dyn Future<Output = Result<Output>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
    Output: Serialize + Send + Sync + Clone + 'static,
{
    async fn call(&self, ctx: RequestContext, params: Params) -> Result<Value> {
        let output = (self.f)(ctx, params).await?;
        let value = serde_json::to_value(output)?;
        Ok(value)
    }
}

/// Adapter that converts a Handler to CoreTool
///
/// # Example
/// ```rust,ignore
/// let adapter = HandlerToolAdapter::new(
///     po,
///     parameters_schema,
///     inner,
/// );
/// ```
#[derive(Clone)]
pub struct HandlerToolAdapter<Params>
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    po: ToolPo,
    parameters_schema: Value,
    inner: Box<dyn HandlerFn<Params>>,
}

impl<Params> HandlerToolAdapter<Params>
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    pub fn new(po: ToolPo, parameters_schema: Value, inner: Box<dyn HandlerFn<Params>>) -> Self {
        Self {
            po,
            parameters_schema,
            inner,
        }
    }
}

#[async_trait]
impl<Params> CoreTool for HandlerToolAdapter<Params>
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    async fn call(&self, mut ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse JSON args to Params type
        let params: Params = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => {
                return Err(ToolError::JsonError(e).into());
            }
        };

        match self.inner.call(ctx, params).await {
            Ok(result) => Ok(result),
            Err(app_error) => Err(ToolError::ToolCallError(Box::new(app_error)).into()),
        }
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// Builder for creating HandlerToolAdapter
#[allow(dead_code)]
pub struct HandlerToolBuilder {
    id: String,
    name: String,
    description: String,
    parameters_schema: Value,
}

impl HandlerToolBuilder {
    pub fn new(id: String, name: String, description: String) -> Self {
        // Default: generate empty parameters schema, can be overridden
        let parameters_schema = json!({
            "type": "object",
            "properties": {},
            "required": []
        });

        Self {
            id,
            name,
            description,
            parameters_schema,
        }
    }

    pub fn with_parameters_schema(mut self, schema: Value) -> Self {
        self.parameters_schema = schema;
        self
    }

    pub fn build<Params, F>(self, f: F) -> (ToolPo, Box<dyn HandlerFn<Params>>)
    where
        F: Fn(
                RequestContext,
                Params,
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>>
            + Send
            + Sync
            + Clone
            + 'static,
        Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
    {
        use common::enums::tool::{ControlMode, ToolProtocol, ToolStatus};
use common::error::Result;

        let mut po = ToolPo::new(
            self.id.clone(),
            self.name.clone(),
            self.description.clone(),
            ToolProtocol::Builtin,
            serde_json::Value::Null,              // config
            Some(self.parameters_schema.clone()), // parameters_schema is already Value
            vec![],                               // tags
            None,                                 // creator
        );
        po.status = ToolStatus::Enabled;
        po.control_mode = ControlMode::Auto;

        let inner = Box::new(GenericHandlerFn::new(f));

        (po, inner)
    }
}

/// Helper: convert common::error::Error to ToolError
pub fn app_error_to_tool_error(e: common::error::Error) -> ToolError {
    ToolError::ToolCallError(e.to_string().into())
}

// Re-export the macro
pub use macros::register_handler_tool;
