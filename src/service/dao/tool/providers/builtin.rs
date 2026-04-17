//! Builtin tool provider

use dyn_clone::DynClone;
use async_trait::async_trait;
use dyn_clone::clone_trait_object;
use uuid::Uuid;

/// Built-in tool trait - object-safe for registry storage
///
/// All built-in tools must implement this trait.
#[async_trait]
pub trait BuiltinTool: Send + Sync + DynClone {
    /// Unique tool ID
    fn id(&self) -> Uuid;
    /// Tool name (from Rig Tool)
    fn name(&self) -> &'static str;
    /// Human-readable description
    fn description(&self) -> &'static str;
    /// Wrap into DynTool (unified erasured type) for general use
    fn wrap(self: Box<Self>) -> DynTool;
}

clone_trait_object!(BuiltinTool);

/// Default implementation wrapping for any Rig Tool that implements BuiltinTool extension
pub struct DefaultBuiltinTool<T> {
    inner: T,
    id: Uuid,
    name: &'static str,
    description: &'static str,
}

impl<T> DefaultBuiltinTool<T>
where
    T: rig::tool::Tool + Clone + Send + Sync + 'static,
    T::Args: for<'de> serde::Deserialize<'de>,
    T::Output: serde::Serialize,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    pub fn new(inner: T, tool_id: &'static str, name: &'static str, description: &'static str) -> Self {
        let id = Uuid::parse_str(tool_id).expect("Invalid tool ID UUID");
        Self { inner, id, name, description }
    }
}

#[async_trait]
impl<T> BuiltinTool for DefaultBuiltinTool<T>
where
    T: rig::tool::Tool + Clone + Send + Sync + 'static,
    T::Args: for<'de> serde::Deserialize<'de>,
    T::Output: serde::Serialize,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn id(&self) -> Uuid {
        self.id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        self.description
    }

    fn wrap(self: Box<Self>) -> DynTool {
        Box::new(RigToolWrapper::new(self.inner))
    }
}

impl<T> Clone for DefaultBuiltinTool<T>
where
    T: rig::tool::Tool + Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            id: self.id,
            name: self.name,
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
        let args = serde_json::from_value(args)?;
        let output = self.inner.call(args).await.map_err(|e| {
            rig::tool::ToolError::ToolCallError(e.into())
        })?;
        let value = serde_json::to_value(output)?;
        Ok(value)
    }
}

impl<T> Clone for RigToolWrapper<T>
where
    T: rig::tool::Tool + Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
