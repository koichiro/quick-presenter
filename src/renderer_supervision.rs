//! Deadline supervision independent of blocking pipe IO and native execution.
use crate::renderer_limits::{Operation, MEMORY_POLL_INTERVAL};
use std::{
    process::Child,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Timeout,
    MemoryLimit,
    Crash,
    Protocol,
    Spawn,
}
#[derive(Debug)]
pub struct HelperFailure {
    pub kind: FailureKind,
    pub operation: Operation,
}
impl std::fmt::Display for HelperFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Renderer helper failed: operation={:?} cause={:?}",
            self.operation, self.kind
        )
    }
}
impl std::error::Error for HelperFailure {}

pub(crate) fn terminate(child: &Mutex<Child>) {
    let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
    let _ = child.kill();
    if child.wait().is_err() {
        tracing::warn!("renderer helper reap failed");
    }
}

#[derive(Default)]
struct State {
    deadline: Option<(Instant, Operation)>,
    failure: Option<FailureKind>,
    stopped: bool,
}
pub struct Watchdog {
    state: Arc<(Mutex<State>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
impl Watchdog {
    pub fn new(child: Arc<Mutex<Child>>) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker_state = Arc::clone(&state);
        let thread = std::thread::spawn(move || {
            let (lock, wake) = &*worker_state;
            loop {
                let mut current = lock.lock().unwrap_or_else(|e| e.into_inner());
                if current.stopped {
                    break;
                }
                let expired = current
                    .deadline
                    .is_some_and(|(deadline, _)| Instant::now() >= deadline);
                let memory_exceeded = crate::renderer_resources::memory_exceeded(&child);
                if expired || memory_exceeded {
                    current.failure = Some(if memory_exceeded {
                        FailureKind::MemoryLimit
                    } else {
                        FailureKind::Timeout
                    });
                    current.deadline = None;
                    drop(current);
                    terminate(&child);
                    break;
                }
                let wait = current
                    .deadline
                    .map(|(deadline, _)| {
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(MEMORY_POLL_INTERVAL)
                    })
                    .unwrap_or(MEMORY_POLL_INTERVAL);
                drop(
                    wake.wait_timeout(current, wait)
                        .unwrap_or_else(|e| e.into_inner()),
                );
            }
        });
        Self {
            state,
            thread: Some(thread),
        }
    }
    pub fn arm(&self, operation: Operation) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        state.deadline = Some((Instant::now() + operation.deadline(), operation));
        wake.notify_one();
    }
    pub fn finish(&self) -> Option<FailureKind> {
        let mut state = self.state.0.lock().unwrap_or_else(|e| e.into_inner());
        if state
            .deadline
            .is_some_and(|(deadline, _)| Instant::now() >= deadline)
        {
            state.failure = Some(FailureKind::Timeout);
        }
        state.deadline = None;
        state.failure
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        let (lock, wake) = &*self.state;
        lock.lock().unwrap_or_else(|e| e.into_inner()).stopped = true;
        wake.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_reply_cannot_disarm_an_expired_deadline() {
        let watchdog = Watchdog {
            state: Arc::new((
                Mutex::new(State {
                    deadline: Some((
                        Instant::now() - std::time::Duration::from_secs(1),
                        Operation::Open,
                    )),
                    ..State::default()
                }),
                Condvar::new(),
            )),
            thread: None,
        };
        assert_eq!(watchdog.finish(), Some(FailureKind::Timeout));
        assert_eq!(watchdog.finish(), Some(FailureKind::Timeout));
    }
    #[test]
    fn successful_reply_disarms_the_deadline() {
        let watchdog = Watchdog {
            state: Arc::new((
                Mutex::new(State {
                    deadline: Some((
                        Instant::now() + std::time::Duration::from_secs(1),
                        Operation::Open,
                    )),
                    ..State::default()
                }),
                Condvar::new(),
            )),
            thread: None,
        };
        assert_eq!(watchdog.finish(), None);
        assert!(watchdog.state.0.lock().unwrap().deadline.is_none());
        assert_eq!(
            HelperFailure {
                kind: FailureKind::Protocol,
                operation: Operation::VisibleRender
            }
            .to_string(),
            "Renderer helper failed: operation=VisibleRender cause=Protocol"
        );
    }
}
