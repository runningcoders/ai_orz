//! Builtin tool provider

use dyn_clone::DynClone;
use async_trait::async_trait;
use dyn_clone::clone_trait_object;
use uuid::Uuid;

/// Object-safe, dyn-compatible built-in tool trait - specifically for storing builtins in registry
///
/// All built-in tools are pre-compiled, implement Rig's Tool and this trait for registry storage.
#[async_trait]
pub trait BuiltinTool: Send + Sync + DynClone {
    /// Unique tool ID (UUID as string)
    fn tool_id(&self) -> &'static str;
    /// Tool description
    fn description(&self) -> &'static str;
    /// Get tool name from Rig
    fn name(&self) -> String;
    /// Get tool definition from Rig
    async fn definition(&self, prompt: String) -> rig::completion::ToolDefinition;
    /// Call tool with JSON arguments (already erased to JSON I/O)
    async fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, rig::tool::ToolError>;

    /// Get parsed UUID
    fn id(&self) -> Uuid {
        Uuid::parse_str(self.tool_id()).expect("Invalid tool ID UUID")
    }
}

clone_trait_object!(BuiltinTool);

/// Concrete wrapper for any Rig Tool + metadata that implements BuiltinTool
///
/// Users implement a Rig Tool, then wrap it with this to register.
pub struct RigBuiltinTool<T> {
    inner: T,
    tool_id: &'static str,
    description: &'static str,
}

impl<T> RigBuiltinTool<T>
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
impl<T> BuiltinTool for RigBuiltinTool<T>
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

    fn name(&self) -> String {
        self.inner.name()
    }

    async fn definition(&self, prompt: String) -> rig::completion::ToolDefinition {
        self.inner.definition(prompt).await
    }

    async fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, rig::tool::ToolError> {
        let args = serde_json::from_value(args)?;
        let output = self.inner.call(args).await.map_err(|e| {
            rig::tool::ToolError::ToolCallError(e.into())
        })?;
        Ok(serde_json::to_value(output)?)
    }
}

impl<T> Clone for RigBuiltinTool<T>
where
    T: rig::tool::Tool + Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            tool_id: self.tool_id,
            description: self.description,
        }
    }
}

/// Object-safe, dyn-compatible tool trait with JSON I/O (unified interface for all protocols)
#[async_trait]
pub trait ErasedTool: Send + Sync + DynClone {
    /// Get tool name
    fn name(&self) -> String;
    /// Get tool definition
    async fn definition(&self, prompt: String) -> rig::completion::ToolDefinition;
    /// Call tool with JSON arguments
    async fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, rig::tool::ToolError>;
}

clone_trait_object!(ErasedTool);

/// Type alias for dynamic tool trait object (returned by registry.get())
pub type DynTool = Box<dyn ErasedTool>;

/// Wrapper to convert BuiltinTool to unified ErasedTool
pub struct BuiltinErasedWrapper(pub Box<dyn BuiltinTool>);

#[async_trait]
impl ErasedTool for BuiltinErasedWrapper {
    fn name(&self) -> String {
        self.0.name()
    }

    async fn definition(&self, prompt: String) -> rig::completion::ToolDefinition {
        self.0.definition(prompt).await
    }

    async fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, rig::tool::ToolError> {
        self.0.call(args).await
    }
}

impl Clone for BuiltinErasedWrapper {
    fn clone(&self) -> Self {
        Self(dyn_clone::clone_box(&*self.0))
    }
}
