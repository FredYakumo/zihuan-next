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
        tokio::runtime::Runtime::new().expect("tokio runtime").block_on(future)
    }
}

/// Run blocking work that may create and drop its own Tokio runtime outside the active runtime
/// context.
///
/// `reqwest::blocking` (in debug builds) builds a temporary current-thread runtime per request
/// to assert the caller is not blocking an async thread. Dropping that runtime while already
/// inside an entered runtime panics with "Cannot drop a runtime in a context where blocking is
/// not allowed". Blocking code reached from an async chain (for example the QQ brain's LLM call
/// under `block_async`) therefore has to run on its own thread.
pub fn run_outside_async_runtime<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|scope| {
            scope
                .spawn(f)
                .join()
                .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
        })
    } else {
        f()
    }
}
