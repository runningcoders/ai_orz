use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventKind(pub &'static str);

impl EventKind {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }
}

pub trait Event: Send + Sync + Clone + Serialize + DeserializeOwned + 'static {
    fn kind(&self) -> EventKind;
    fn id(&self) -> &str;
    fn order_key(&self) -> &str {
        ""
    }
    fn priority(&self) -> u8 {
        0
    }
    fn created_at(&self) -> i64 {
        0
    }
}
