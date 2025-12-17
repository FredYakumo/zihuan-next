pub mod brain;

/// Base trait for all event-driven agents.
///
/// An agent consumes an event and produces an output/decision.
///
pub trait Agent: Send + Sync {
	type Event;
	type Output;

	fn name(&self) -> &'static str {
		"agent"
	}

	fn on_event(&self, event: &Self::Event) -> Self::Output;
}