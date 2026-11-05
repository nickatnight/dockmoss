use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Runs `read` on a thread of its own and waits at most `timeout` for its
/// answer. `None` when it does not answer in time, or its thread cannot
/// start; a late read finishes on its own and its answer is dropped.
pub fn read_within<T: Send + 'static>(
    timeout: Duration,
    read: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (reply, replied) = mpsc::channel();
    thread::Builder::new()
        .name("bounded-read".into())
        .spawn(move || {
            if reply.send(read()).is_err() {
                tracing::debug!("a bounded read answered after its caller stopped waiting");
            }
        })
        .inspect_err(|error| tracing::warn!(%error, "bounded read not started"))
        .ok()?;
    replied.recv_timeout(timeout).ok()
}

#[cfg(test)]
mod tests;
