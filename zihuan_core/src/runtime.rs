use std::sync::OnceLock;
use std::time::Duration;

use tokio::runtime::{Handle, Runtime};
use tokio::task::block_in_place;

/// Shared multi-threaded runtime for synchronous code that needs to await a future while no
/// runtime is entered on the current thread.
fn shared_runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("tokio runtime"))
}

/// Runtime that drives futures which must never run on the caller's thread.
fn escape_runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("block-async-deadline")
            .build()
            .expect("deadline runtime")
    })
}

/// Run an async future from synchronous code.
///
/// If the current thread is already inside a Tokio runtime, the future is
/// executed with `block_in_place` to avoid nesting another runtime.
/// Otherwise, a shared runtime is used so connection drivers survive the call.
///
/// The entered runtime must be multi-threaded: code reached from `future` may
/// call `block_async` again, and `block_in_place` aborts a current-thread
/// runtime with "can call blocking only when running on the multi-threaded
/// runtime".
pub fn block_async<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    if let Ok(handle) = Handle::try_current() {
        block_in_place(|| handle.block_on(future))
    } else {
        shared_runtime().block_on(future)
    }
}

/// Run an async future from synchronous code with a deadline.
///
/// [`block_async`] hands the calling thread to `block_in_place`/`Handle::block_on`, so a future
/// that loses its wakeup parks that thread forever — a stalled context-database write once froze
/// a whole QQ turn that way. This variant drives the future on a dedicated runtime and waits for
/// the result with `recv_timeout`, so the caller always regains control. The future is abandoned
/// when the deadline passes.
pub fn block_async_timeout<F, T>(future: F, timeout: Duration) -> crate::error::Result<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let timed_out = || crate::string_error!("async work did not finish within {timeout:?}");
    // Driven on the escape runtime in both cases: waiting with `recv_timeout` instead of
    // `block_on` means even a future that stalls inside a synchronous call cannot hold the caller.
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    escape_runtime().spawn(async move {
        let _ = sender.send(future.await);
    });
    receiver.recv_timeout(timeout).map_err(|_| timed_out())
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
