//! Builtin tool factory - built-in tools are created from constant definitions

use crate::models::tool::{Tool, ToolPo};
use dyn_clone::DynClone;
use dyn_clone::clone_trait_object;

/// Built-in tool factory - creates tool instance from given ToolPo
/// 
/// Each built-in tool registers a factory that knows how to construct itself given the ToolPo from DB.
/// ToolPo contains configuration that may have been customized by the user (name/description.
pub trait BuiltinToolFactory: DynClone + Send + Sync {
    /// Unique built-in tool identifier - matches the id/name in DB
    fn id(&self) -> &'static str;
    /// Human readable name - used for default ToolPo
    fn name(&self) -> &'static str;
    /// Description - used for default ToolPo
    fn description(&self) -> &'static str;
    /// Create a tool instance given the ToolPo from DB
    /// ToolPo is from database - factory injects configuration from DB
    fn create(&self, po: ToolPo) -> Box<dyn Tool>;
}

clone_trait_object!(BuiltinToolFactory);
