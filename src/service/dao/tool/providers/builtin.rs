//! Builtin tool provider

use anyhow::{anyhow, Result};
use crate::models::tool::ToolPo;
use crate::service::dao::tool::providers::GLOBAL_BUILTIN_REGISTRY;
use serde_json::Value;
use dyn_clone::DynClone;
use rig::tool::Tool;
use rig::completion::ToolDefinition;
use async_trait::async_trait;
use dyn_clone::clone_trait_object;
use uuid::Uuid;

/// Extension trait for built-in tools.
///
/// Builtin tools must implement this trait to provide:
/// - Constant unique `TOOL_ID` (UUID string) for upsert deduplication
/// - Constant `DESCRIPTION` for tool center display
pub trait BuiltinTool: rig::tool::Tool + Clone + Sized + 'static {
    /// Unique tool ID (UUID string), must be constant and unique across all builtin tools
    const TOOL_ID: &'static str;
    /// Human-readable description for tool center
    const DESCRIPTION: &'static str;

    /// Get parsed UUID from constant
    fn tool_id(&self) -> Uuid {
        Uuid::parse_str(Self::TOOL_ID).expect("Invalid TOOL_ID UUID string")
    }

    /// Get tool name from Rig Tool trait
    fn tool_name(&self) -> &'static str {
        Self::NAME
    }

    /// Get description from constant
    fn tool_description(&self) -> &'static str {
        Self::DESCRIPTION
    }
}

/// Object-safe, dyn-compatible tool trait with JSON I/O
#[async_trait]
pub trait ErasedTool: Send + Sync + DynClone {
    /// Get tool name
    fn name(&self) -> String;
    /// Get tool definition
    async fn definition(&self, prompt: String) -> ToolDefinition;
    /// Call tool with JSON arguments (already deserialized from string)
    async fn call(&self, args: Value) -> Result<Value, rig::tool::ToolError>;
}

clone_trait_object!(ErasedTool);

/// Type alias for dynamic tool trait object (what we cache)
pub type DynTool = Box<dyn ErasedTool>;

/// Wrapper that converts an arbitrary Rig Tool to our ErasedTool
pub struct RigToolWrapper<T> {
    inner: T,
}

impl<T> RigToolWrapper<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<T> ErasedTool for RigToolWrapper<T>
where
    T: Tool + Clone + Send + Sync + 'static,
    T::Args: for<'de> serde::Deserialize<'de>,
    T::Output: serde::Serialize,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn name(&self) -> String {
        self.inner.name()
    }

    async fn definition(&self, prompt: String) -> ToolDefinition {
        self.inner.definition(prompt).await
    }

    async fn call(&self, args: Value) -> Result<Value, rig::tool::ToolError> {
        // serde_json::Error already implements From
        let args = serde_json::from_value(args)?;
        let output = self.inner.call(args).await.map_err(|e| {
            rig::tool::ToolError::ToolCallError(e.into())
        })?;
        // serde_json::Error already implements From
        let value = serde_json::to_value(output)?;
        Ok(value)
    }
}

impl<T> Clone for RigToolWrapper<T>
where
    T: Tool + Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Build a builtin tool from ToolPo
pub fn build(po: &ToolPo) -> Result<DynTool> {
    let registry = GLOBAL_BUILTIN_REGISTRY.get()
        .ok_or_else(|| anyhow!("Builtin registry not initialized"))?;

    registry.get(&po.name)
        .ok_or_else(|| anyhow!("Builtin tool '{}' not registered", po.name))
}
