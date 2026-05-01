use tokio::task::block_in_place;

/// Run an async future from synchronous code.
///
/// If the current thread is already inside a Tokio runtime, the future is
/// executed with `block_in_place` to avoid nesting another runtime.
/// Otherwise, a lightweight runtime is created on demand.
pub fn block_async<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(future))
    } else {
        tokio::runtime::Runtime::new()
            .expect("tokio runtime")
            .block_on(future)
    }
}