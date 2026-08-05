//! Agent loop event consumer (AOP sync)
//!
//! Subscribes to `agent.loop` and `agent.think.round` events for logging.
//! Provides visibility into agent lifecycle (start/finish) and per-round think metrics.

use async_trait::async_trait;
use common::error::Result;

use crate::models::events::{AgentLoopEvent, ThinkRoundEvent};
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};

pub struct AgentLoopConsumer;

impl AgentLoopConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentLoopConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for AgentLoopConsumer {
    fn name(&self) -> &str {
        "agent_loop"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![
            EventKind::new("agent.loop"),
            EventKind::new("agent.think.round"),
        ]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let kind = event.get("kind").and_then(|v| v.as_str()).unwrap_or("");

        match kind {
            "agent.loop" => {
                let event: AgentLoopEvent = serde_json::from_value(event).map_err(|e| {
                    common::error::Error::internal(format!(
                        "failed to deserialize AgentLoopEvent: {}",
                        e
                    ))
                })?;
                match event.phase.as_str() {
                    "started" => {
                        tracing::info!(
                            agent_id = %event.agent_id,
                            scene = %event.scene,
                            trace_id = %event.trace_id,
                            "agent loop started"
                        );
                    }
                    "finished" => {
                        tracing::info!(
                            agent_id = %event.agent_id,
                            scene = %event.scene,
                            status = ?event.status,
                            duration_ms = event.duration_ms.unwrap_or(0),
                            "agent loop finished"
                        );
                    }
                    _ => {}
                }
            }
            "agent.think.round" => {
                let event: ThinkRoundEvent = serde_json::from_value(event).map_err(|e| {
                    common::error::Error::internal(format!(
                        "failed to deserialize ThinkRoundEvent: {}",
                        e
                    ))
                })?;
                tracing::debug!(
                    agent_id = %event.agent_id,
                    scene = %event.scene,
                    round = event.round_number,
                    duration_ms = event.duration_ms,
                    tool_calls = event.tool_call_count,
                    "think round"
                );
            }
            _ => {}
        }

        Ok(())
    }
}
