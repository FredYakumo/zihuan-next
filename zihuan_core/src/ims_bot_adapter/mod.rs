pub mod logging;
pub mod models;
pub mod natural_language_reply;

/// Opaque handle for the bot adapter, stored in DataValue.
/// The concrete type is `Arc<TokioMutex<BotAdapter>>` in the main crate;
/// it is type-erased here so that downstream crates can hold it without
/// depending on a concrete adapter implementation.
pub type BotAdapterHandle = std::sync::Arc<dyn std::any::Any + Send + Sync + 'static>;
