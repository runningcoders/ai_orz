//! Builtin tool provider

use rig::tool::ToolDyn;
use uuid::Uuid;
use dyn_clone::DynClone;
use dyn_clone::clone_trait_object;

/// Built-in tool trait - adds metadata on top of Rig's ToolDyn.
/// Users implement this trait directly for their tools.
pub trait BuiltinTool: ToolDyn + DynClone + Send + Sync {
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
