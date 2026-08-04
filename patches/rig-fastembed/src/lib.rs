//! Stub crate for rig-fastembed.
//!
//! The real rig-fastembed provides vector store integration for fastembed.
//! This project does not enable rig's `fastembed` feature (it has its own
//! direct fastembed dependency), so this stub exists solely to satisfy
//! cargo's version resolver without pulling in a conflicting fastembed 4.x.
