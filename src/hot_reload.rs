use std::{
    path::{Component, Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use notify::{Config, Event, EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};

use crate::render_scheduler::RenderSessionId;

pub const HOT_RELOAD_DEBOUNCE: Duration = Duration::from_millis(300);
pub const HOT_RELOAD_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
pub const HOT_RELOAD_SLOW_DELAY: Duration = Duration::from_secs(2);
pub const WATCHER_RECOVERY_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

const WATCH_EVENT_CAPACITY: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WatchTarget {
    path: PathBuf,
    parent: PathBuf,
}

impl WatchTarget {
    pub fn new(path: PathBuf) -> Result<Self> {
        let path = absolute_lexical_path(path)?;
        let parent = path
            .parent()
            .context("PDF path has no parent directory")?
            .to_path_buf();
        Ok(Self { path, parent })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn parent(&self) -> &Path {
        &self.parent
    }

    fn matches(&self, path: &Path) -> bool {
        absolute_lexical_path(path.to_path_buf())
            .map(|path| path == self.path)
            .unwrap_or(false)
    }
}

fn absolute_lexical_path(path: PathBuf) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .context("failed to resolve current directory")?
            .join(path)
    };

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WatchSignal {
    Paths(Vec<PathBuf>),
    Reconcile,
    Failed(String),
}

enum WatchBackend {
    Native(RecommendedWatcher),
    Poll(PollWatcher),
}

impl WatchBackend {
    fn watch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Self::Native(watcher) => watcher.watch(path, RecursiveMode::NonRecursive),
            Self::Poll(watcher) => watcher.watch(path, RecursiveMode::NonRecursive),
        }
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Self::Native(watcher) => watcher.unwatch(path),
            Self::Poll(watcher) => watcher.unwatch(path),
        }
    }
}

pub struct PdfWatcher {
    backend: WatchBackend,
    receiver: Receiver<WatchSignal>,
    target: Option<WatchTarget>,
}

impl PdfWatcher {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(WATCH_EVENT_CAPACITY);
        let backend = create_native_watcher(sender.clone())
            .map(WatchBackend::Native)
            .or_else(|native_error| {
                create_poll_watcher(sender).map(WatchBackend::Poll).map_err(
                    |poll_error| {
                        anyhow::anyhow!(
                            "native watcher failed: {native_error}; polling watcher failed: {poll_error}"
                        )
                    },
                )
            })?;

        Ok(Self {
            backend,
            receiver,
            target: None,
        })
    }

    pub fn replace_target(&mut self, target: Option<WatchTarget>) -> Result<()> {
        let old_parent = self.target.as_ref().map(WatchTarget::parent);
        let new_parent = target.as_ref().map(WatchTarget::parent);

        if old_parent != new_parent {
            if let Some(parent) = new_parent {
                self.backend
                    .watch(parent)
                    .with_context(|| format!("failed to watch {}", parent.display()))?;
            }
            if let Some(parent) = old_parent {
                if let Err(error) = self.backend.unwatch(parent) {
                    if let Some(new_parent) = new_parent {
                        let _ = self.backend.unwatch(new_parent);
                    }
                    return Err(error)
                        .with_context(|| format!("failed to stop watching {}", parent.display()));
                }
            }
        }

        self.target = target;
        Ok(())
    }

    pub fn drain(&self) -> Vec<WatchSignal> {
        let mut signals = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(signal) => signals.push(signal),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        signals
    }
}

fn create_native_watcher(sender: SyncSender<WatchSignal>) -> notify::Result<RecommendedWatcher> {
    RecommendedWatcher::new(event_handler(sender), Config::default())
}

fn create_poll_watcher(sender: SyncSender<WatchSignal>) -> notify::Result<PollWatcher> {
    PollWatcher::new(
        event_handler(sender),
        Config::default().with_poll_interval(POLL_INTERVAL),
    )
}

fn event_handler(
    sender: SyncSender<WatchSignal>,
) -> impl FnMut(notify::Result<Event>) + Send + 'static {
    move |event| {
        let signal = match event {
            Ok(event) if matches!(event.kind, EventKind::Access(_)) => return,
            Ok(event) if event.paths.is_empty() => WatchSignal::Reconcile,
            Ok(event) => WatchSignal::Paths(event.paths),
            Err(error) => WatchSignal::Failed(error.to_string()),
        };
        let _ = sender.try_send(signal);
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HotReloadPhase {
    Idle,
    Debouncing,
    Preparing {
        session_id: RenderSessionId,
        revision: u64,
    },
    Failed {
        revision: u64,
    },
}

#[derive(Debug)]
pub struct HotReloadState {
    target: Option<WatchTarget>,
    revision: u64,
    phase: HotReloadPhase,
    deadline: Option<Instant>,
    manual_open_pending: bool,
}

impl Default for HotReloadState {
    fn default() -> Self {
        Self {
            target: None,
            revision: 0,
            phase: HotReloadPhase::Idle,
            deadline: None,
            manual_open_pending: false,
        }
    }
}

impl HotReloadState {
    pub fn target(&self) -> Option<&WatchTarget> {
        self.target.as_ref()
    }

    pub fn phase(&self) -> HotReloadPhase {
        self.phase
    }

    pub fn replace_target(&mut self, target: WatchTarget) {
        self.target = Some(target);
        self.revision = self.revision.wrapping_add(1);
        self.phase = HotReloadPhase::Idle;
        self.deadline = None;
        self.manual_open_pending = false;
    }

    pub fn begin_manual_open(&mut self) {
        self.manual_open_pending = true;
        self.revision = self.revision.wrapping_add(1);
        self.phase = HotReloadPhase::Idle;
        self.deadline = None;
    }

    pub fn finish_manual_open_without_replacement(&mut self) {
        self.manual_open_pending = false;
    }

    pub fn observe(&mut self, signal: &WatchSignal, now: Instant) -> bool {
        if self.manual_open_pending || !self.is_relevant(signal) {
            return false;
        }

        self.revision = self.revision.wrapping_add(1);
        if !matches!(self.phase, HotReloadPhase::Preparing { .. }) {
            self.phase = HotReloadPhase::Debouncing;
            self.deadline = Some(now + HOT_RELOAD_DEBOUNCE);
        }
        true
    }

    pub fn due_revision(&self, now: Instant) -> Option<u64> {
        (self.phase == HotReloadPhase::Debouncing
            && self.deadline.is_some_and(|deadline| now >= deadline))
        .then_some(self.revision)
    }

    pub fn begin_preparing(&mut self, session_id: RenderSessionId, revision: u64) -> bool {
        if self.phase != HotReloadPhase::Debouncing || self.revision != revision {
            return false;
        }
        self.phase = HotReloadPhase::Preparing {
            session_id,
            revision,
        };
        self.deadline = None;
        true
    }

    pub fn accepts_prepared(&self, session_id: RenderSessionId, revision: u64) -> bool {
        self.phase
            == HotReloadPhase::Preparing {
                session_id,
                revision,
            }
            && self.revision == revision
            && !self.manual_open_pending
    }

    pub fn finish_preparing(
        &mut self,
        session_id: RenderSessionId,
        revision: u64,
        succeeded: bool,
        now: Instant,
    ) -> bool {
        if self.phase
            != (HotReloadPhase::Preparing {
                session_id,
                revision,
            })
        {
            return false;
        }

        if self.revision != revision {
            self.phase = HotReloadPhase::Debouncing;
            self.deadline = Some(now + HOT_RELOAD_DEBOUNCE);
        } else if succeeded {
            self.phase = HotReloadPhase::Idle;
            self.deadline = None;
        } else {
            self.phase = HotReloadPhase::Failed { revision };
            self.deadline = None;
        }
        true
    }

    fn is_relevant(&self, signal: &WatchSignal) -> bool {
        let Some(target) = self.target.as_ref() else {
            return false;
        };
        match signal {
            WatchSignal::Paths(paths) => paths.iter().any(|path| target.matches(path)),
            WatchSignal::Reconcile => true,
            WatchSignal::Failed(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> WatchTarget {
        WatchTarget::new(PathBuf::from("/slides/deck.pdf")).unwrap()
    }

    fn changed(path: &str) -> WatchSignal {
        WatchSignal::Paths(vec![PathBuf::from(path)])
    }

    #[test]
    fn target_normalizes_relative_components_without_resolving_the_file() {
        let target = WatchTarget::new(PathBuf::from("/slides/work/../deck.pdf")).unwrap();

        assert_eq!(target.path(), Path::new("/slides/deck.pdf"));
        assert_eq!(target.parent(), Path::new("/slides"));
    }

    #[test]
    fn relevant_event_starts_debounce() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());

        assert!(state.observe(&changed("/slides/deck.pdf"), now));
        assert_eq!(state.phase(), HotReloadPhase::Debouncing);
        assert_eq!(state.due_revision(now), None);
        assert_eq!(state.due_revision(now + HOT_RELOAD_DEBOUNCE), Some(2));
    }

    #[test]
    fn unrelated_sibling_event_is_ignored() {
        let mut state = HotReloadState::default();
        state.replace_target(target());

        assert!(!state.observe(&changed("/slides/notes.txt"), Instant::now()));
        assert_eq!(state.phase(), HotReloadPhase::Idle);
    }

    #[test]
    fn event_burst_extends_debounce_and_coalesces_revision() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.observe(&changed("/slides/deck.pdf"), now);
        state.observe(
            &changed("/slides/deck.pdf"),
            now + Duration::from_millis(200),
        );

        assert_eq!(state.due_revision(now + HOT_RELOAD_DEBOUNCE), None);
        assert_eq!(
            state.due_revision(now + Duration::from_millis(500)),
            Some(3)
        );
    }

    #[test]
    fn reconcile_signal_is_relevant_for_active_target() {
        let mut state = HotReloadState::default();
        state.replace_target(target());

        assert!(state.observe(&WatchSignal::Reconcile, Instant::now()));
    }

    #[test]
    fn new_event_during_prepare_makes_candidate_stale() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.observe(&changed("/slides/deck.pdf"), now);
        let revision = state.due_revision(now + HOT_RELOAD_DEBOUNCE).unwrap();
        assert!(state.begin_preparing(RenderSessionId(3), revision));

        state.observe(&changed("/slides/deck.pdf"), now + HOT_RELOAD_DEBOUNCE);

        assert!(!state.accepts_prepared(RenderSessionId(3), revision));
        assert!(state.finish_preparing(
            RenderSessionId(3),
            revision,
            true,
            now + HOT_RELOAD_DEBOUNCE
        ));
        assert_eq!(state.phase(), HotReloadPhase::Debouncing);
    }

    #[test]
    fn failed_prepare_waits_for_later_event() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.observe(&changed("/slides/deck.pdf"), now);
        let revision = state.due_revision(now + HOT_RELOAD_DEBOUNCE).unwrap();
        state.begin_preparing(RenderSessionId(4), revision);

        assert!(state.finish_preparing(
            RenderSessionId(4),
            revision,
            false,
            now + HOT_RELOAD_DEBOUNCE
        ));
        assert_eq!(state.phase(), HotReloadPhase::Failed { revision });

        assert!(state.observe(&changed("/slides/deck.pdf"), now + Duration::from_secs(1)));
        assert_eq!(state.phase(), HotReloadPhase::Debouncing);
    }

    #[test]
    fn manual_open_suppresses_old_target_events_until_it_finishes() {
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.begin_manual_open();

        assert!(!state.observe(&changed("/slides/deck.pdf"), Instant::now()));

        state.finish_manual_open_without_replacement();
        assert!(state.observe(&changed("/slides/deck.pdf"), Instant::now()));
    }
}
