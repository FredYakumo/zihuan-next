pub mod event_model;
pub mod message;
pub mod natural_language_reply;
pub mod profile;

pub use event_model::*;
pub use profile::*;

/// Opaque handle for the bot adapter, stored in DataValue.
/// The concrete type is `Arc<TokioMutex<BotAdapter>>` in the main crate;
/// it is type-erased here so that `zihuan_node` can hold it without depending on
/// the main crate's `bot_adapter` module.
pub type BotAdapterHandle = std::sync::Arc<dyn std::any::Any + Send + Sync + 'static>;
