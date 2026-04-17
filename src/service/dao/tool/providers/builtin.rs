//! Builtin tool provider

use async_trait::async_trait;
use dyn_clone::DynClone;
use dyn_clone::clone_trait_object;
use uuid::Uuid;
use rig::tool::Tool;

use super::ErasedTool;

/// Concrete wrapper for a builtin Rig Tool.
///
/// Stores Rig Tool + metadata, implements ErasedTool directly for unified use.
#[async_trait]
pub trait BuiltinTool: ErasedTool + Send + Sync + DynClone {
    /// Unique tool ID (UUID string)
    fn tool_id(&self) -> &'static str;
    /// Tool description for registry/DB
    fn description(&self) -> &'static str;

    /// Get parsed UUID
    fn id(&self) -> Uuid {
        Uuid::parse_str(self.tool_id()).expect("Invalid tool ID UUID")
    }
}

clone_trait_object!(BuiltinTool);

/// Type-erased wrapper for any Rig Tool implementation that implements BuiltinTool + ErasedTool
#[derive(Clone)]
pub struct RigBuiltinWrapper<T> {
    inner: T,
    tool_id: &'static str,
    description: &'static str,
}

impl<T> RigBuiltinWrapper<T>
where
    T: rig::tool::Tool + Clone + Send + Sync + 'static,
    T::Args: for<'de> serde::Deserialize<'de>,
    T::Output: serde::Serialize,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    pub fn new(inner: T, tool_id: &'static str, description: &'static str) -> Self {
        Self { inner, tool_id, description }
    }
}

#[async_trait]
impl<T> BuiltinTool for RigBuiltinWrapper<T>
where
    T: rig::tool::Tool + Clone + Send + Sync + 'static,
    T::Args: for<'de> serde::Deserialize<'de>,
    T::Output: serde::Serialize,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn tool_id(&self) -> &'static str {
        self.tool_id
    }

    fn description(&self) -> &'static str {
        self.description
    }
}

#[async_trait]
impl<T> ErasedTool for RigBuiltinWrapper<T>
where
    T: rig::tool::Tool + Clone + Send + Sync + 'static,
    T::Args: for<'de> serde::Deserialize<'de>,
    T::Output: serde::Serialize,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn name(&self) -> String {
        self.inner.name()
    }

    async fn definition(&self, prompt: String) -> rig::completion::ToolDefinition {
        self.inner.definition(prompt).await
    }

    async fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, rig::tool::ToolError> {
        let args: T::Args = serde_json::from_value(args)?;
        let output = self.inner.call(args).await.map_err(|e| {
            rig::tool::ToolError::ToolCallError(e.into())
        })?;
        Ok(serde_json::to_value(output)?)
    }
}
