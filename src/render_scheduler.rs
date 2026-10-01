use std::{collections::HashSet, path::PathBuf};
use std::{
    collections::VecDeque,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
};

use crate::{
    errors::speaker_notes_warning,
    notes::SpeakerNotes,
    pdf::PdfDocumentState,
    rendering::{actual_render_bytes, RenderPurpose, RenderRequest, RenderedPagePixels},
};

const MAX_PENDING_RENDER_COMMANDS: usize = 64;
const MAX_PENDING_RENDER_EVENTS: usize = 64;

#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RenderSessionId(pub u64);

#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RenderJobId(pub u64);

#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RenderPriority {
    Background,
    Warm,
    VisibleAux,
    BlockingVisible,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RenderWorkerLifecycle {
    Running,
    ShutdownRequested,
    Stopped,
    Failed,
}

#[derive(Debug)]
pub enum RenderCommand {
    Open {
        session_id: RenderSessionId,
        path: PathBuf,
    },
    RenderPage {
        session_id: RenderSessionId,
        request: RenderRequest,
        priority: RenderPriority,
    },
    ExtractSpeakerNotes {
        session_id: RenderSessionId,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum RenderEvent {
    Opened {
        session_id: RenderSessionId,
        title: String,
        page_count: u32,
        status_text: String,
    },
    SpeakerNotesLoaded {
        session_id: RenderSessionId,
        notes: SpeakerNotes,
        status_text: String,
    },
    OpenFailed {
        session_id: RenderSessionId,
        message: String,
    },
    PageRendered {
        session_id: RenderSessionId,
        job_id: RenderJobId,
        request: RenderRequest,
        page: RenderedPagePixels,
    },
    PageFailed {
        session_id: RenderSessionId,
        job_id: RenderJobId,
        request: RenderRequest,
        message: String,
    },
    WorkerFailed {
        session_id: Option<RenderSessionId>,
        message: String,
    },
}

#[derive(Debug)]
struct RenderSchedulingPolicy;

impl RenderSchedulingPolicy {
    fn accepts_command_after_shutdown(command: &RenderCommand) -> bool {
        matches!(command, RenderCommand::Shutdown)
    }

    fn command_pop_index(controls: &VecDeque<RenderCommand>) -> Option<usize> {
        controls
            .iter()
            .position(|command| matches!(command, RenderCommand::Shutdown))
            .or_else(|| {
                controls
                    .iter()
                    .position(|command| matches!(command, RenderCommand::Open { .. }))
            })
            .or_else(|| (!controls.is_empty()).then_some(0))
    }

    fn open_retains_control(command: &RenderCommand) -> bool {
        matches!(command, RenderCommand::Shutdown)
    }

    fn work_priority_for_key(key: QueueKey, requested: RenderPriority) -> RenderPriority {
        match key {
            QueueKey::Page { .. } => requested,
            QueueKey::SpeakerNotes { .. } => Self::speaker_notes_priority(),
        }
    }

    fn speaker_notes_priority() -> RenderPriority {
        RenderPriority::Background
    }

    fn work_pop_key(priority: RenderPriority, sequence: u64) -> (RenderPriority, u64) {
        (priority, u64::MAX.saturating_sub(sequence))
    }

    fn work_drop_key(priority: RenderPriority, sequence: u64) -> (RenderPriority, u64) {
        (priority, sequence)
    }

    fn event_key(event: &RenderEvent) -> Option<RenderEventKey> {
        match event {
            RenderEvent::Opened { session_id, .. } | RenderEvent::OpenFailed { session_id, .. } => {
                Some(RenderEventKey::Open(*session_id))
            }
            RenderEvent::SpeakerNotesLoaded { session_id, .. } => {
                Some(RenderEventKey::SpeakerNotes(*session_id))
            }
            RenderEvent::PageRendered {
                session_id,
                request,
                ..
            }
            | RenderEvent::PageFailed {
                session_id,
                request,
                ..
            } => Some(RenderEventKey::Page {
                session_id: *session_id,
                request: *request,
            }),
            RenderEvent::WorkerFailed { .. } => None,
        }
    }

    fn event_drop_score(event: &RenderEvent) -> u8 {
        match event {
            RenderEvent::PageRendered { request, .. } | RenderEvent::PageFailed { request, .. } => {
                match request.purpose {
                    RenderPurpose::Thumbnail => 5,
                    RenderPurpose::NextPreview => 4,
                    RenderPurpose::CurrentSlide => 3,
                }
            }
            RenderEvent::SpeakerNotesLoaded { .. } => 2,
            RenderEvent::Opened { .. } | RenderEvent::OpenFailed { .. } => 1,
            RenderEvent::WorkerFailed { .. } => 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct RenderSessionTracker {
    next_session: u64,
    committed_session: Option<RenderSessionId>,
    pending_open_session: Option<RenderSessionId>,
}

impl RenderSessionTracker {
    pub fn begin_open_session(&mut self) -> RenderSessionId {
        self.next_session = self.next_session.wrapping_add(1);
        let session_id = RenderSessionId(self.next_session);
        self.pending_open_session = Some(session_id);
        session_id
    }

    pub fn current_session(&self) -> Option<RenderSessionId> {
        self.committed_session
    }

    pub fn accepts(&self, session_id: RenderSessionId) -> bool {
        self.committed_session == Some(session_id)
    }

    pub fn accepts_pending_open(&self, session_id: RenderSessionId) -> bool {
        self.pending_open_session == Some(session_id)
    }

    pub fn commit_pending_open(&mut self, session_id: RenderSessionId) -> bool {
        if !self.accepts_pending_open(session_id) {
            return false;
        }

        self.committed_session = Some(session_id);
        self.pending_open_session = None;
        true
    }

    pub fn clear_pending_open(&mut self, session_id: RenderSessionId) -> bool {
        if !self.accepts_pending_open(session_id) {
            return false;
        }

        self.pending_open_session = None;
        true
    }

    pub fn mark_worker_failed(&mut self, session_id: Option<RenderSessionId>) -> bool {
        let failure_matches = match session_id {
            Some(session_id) => {
                self.committed_session == Some(session_id)
                    || self.pending_open_session == Some(session_id)
            }
            None => true,
        };

        if !failure_matches {
            return false;
        }

        self.committed_session = None;
        self.pending_open_session = None;
        true
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
enum QueueKey {
    Page {
        session_id: RenderSessionId,
        request: RenderRequest,
    },
    SpeakerNotes {
        session_id: RenderSessionId,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RenderWork {
    Page(RenderRequest),
    SpeakerNotes,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct QueuedWork {
    key: QueueKey,
    job_id: RenderJobId,
    priority: RenderPriority,
    sequence: u64,
}

#[derive(Debug, Default)]
pub struct RenderQueue {
    next_job: u64,
    next_sequence: u64,
    queued_keys: HashSet<QueueKey>,
    items: Vec<QueuedWork>,
}

impl RenderQueue {
    pub fn push(
        &mut self,
        session_id: RenderSessionId,
        request: RenderRequest,
        priority: RenderPriority,
    ) -> Option<RenderJobId> {
        let key = QueueKey::Page {
            session_id,
            request,
        };

        if self.queued_keys.contains(&key) {
            self.raise_priority(key, priority);
            return None;
        }

        self.next_job = self.next_job.wrapping_add(1);
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let job_id = RenderJobId(self.next_job);
        self.queued_keys.insert(key);
        self.items.push(QueuedWork {
            key,
            job_id,
            priority: RenderSchedulingPolicy::work_priority_for_key(key, priority),
            sequence: self.next_sequence,
        });

        Some(job_id)
    }

    pub fn push_speaker_notes(&mut self, session_id: RenderSessionId) -> Option<RenderJobId> {
        let key = QueueKey::SpeakerNotes { session_id };

        if self.queued_keys.contains(&key) {
            return None;
        }

        self.next_job = self.next_job.wrapping_add(1);
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let job_id = RenderJobId(self.next_job);
        self.queued_keys.insert(key);
        self.items.push(QueuedWork {
            key,
            job_id,
            priority: RenderSchedulingPolicy::speaker_notes_priority(),
            sequence: self.next_sequence,
        });

        Some(job_id)
    }

    fn pop(&mut self) -> Option<(RenderJobId, RenderSessionId, RenderWork)> {
        let index = self.next_item_index()?;
        let item = self.items.swap_remove(index);
        self.queued_keys.remove(&item.key);
        Some((item.job_id, item.key.session_id(), item.key.work()))
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.queued_keys.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn raise_priority(&mut self, key: QueueKey, priority: RenderPriority) {
        if let Some(item) = self.items.iter_mut().find(|item| item.key == key) {
            item.priority = item.priority.max(priority);
        }
    }

    fn next_item_index(&self) -> Option<usize> {
        self.items
            .iter()
            .enumerate()
            .max_by_key(|(_, item)| {
                RenderSchedulingPolicy::work_pop_key(item.priority, item.sequence)
            })
            .map(|(index, _)| index)
    }
}

impl QueueKey {
    fn session_id(self) -> RenderSessionId {
        match self {
            Self::Page { session_id, .. } | Self::SpeakerNotes { session_id } => session_id,
        }
    }

    fn work(self) -> RenderWork {
        match self {
            Self::Page { request, .. } => RenderWork::Page(request),
            Self::SpeakerNotes { .. } => RenderWork::SpeakerNotes,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct PendingWorkCommand {
    key: QueueKey,
    priority: RenderPriority,
    sequence: u64,
}

#[derive(Debug)]
struct PendingRenderCommands {
    next_sequence: u64,
    controls: VecDeque<RenderCommand>,
    work_keys: HashSet<QueueKey>,
    works: Vec<PendingWorkCommand>,
    work_capacity: usize,
}

impl PendingRenderCommands {
    fn new(work_capacity: usize) -> Self {
        Self {
            next_sequence: 0,
            controls: VecDeque::new(),
            work_keys: HashSet::new(),
            works: Vec::new(),
            work_capacity,
        }
    }

    fn is_empty(&self) -> bool {
        self.controls.is_empty() && self.works.is_empty()
    }

    fn push(&mut self, command: RenderCommand) {
        if self.has_shutdown() && !RenderSchedulingPolicy::accepts_command_after_shutdown(&command)
        {
            return;
        }

        match command {
            RenderCommand::Open { session_id, path } => {
                self.clear_work();
                self.controls
                    .retain(RenderSchedulingPolicy::open_retains_control);
                self.controls
                    .push_back(RenderCommand::Open { session_id, path });
            }
            RenderCommand::RenderPage {
                session_id,
                request,
                priority,
            } => {
                self.push_work(
                    QueueKey::Page {
                        session_id,
                        request,
                    },
                    priority,
                );
            }
            RenderCommand::ExtractSpeakerNotes { session_id } => {
                self.push_work(
                    QueueKey::SpeakerNotes { session_id },
                    RenderSchedulingPolicy::speaker_notes_priority(),
                );
            }
            RenderCommand::Shutdown => {
                self.controls.clear();
                self.clear_work();
                self.controls.push_back(RenderCommand::Shutdown);
            }
        }
    }

    fn pop(&mut self) -> Option<RenderCommand> {
        if let Some(index) = RenderSchedulingPolicy::command_pop_index(&self.controls) {
            return self.controls.remove(index);
        }

        let index = self.next_work_index()?;
        let work = self.works.swap_remove(index);
        self.work_keys.remove(&work.key);
        Some(work.into_command())
    }

    fn push_work(&mut self, key: QueueKey, priority: RenderPriority) {
        if self.work_keys.contains(&key) {
            self.raise_work_priority(key, priority);
            return;
        }

        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.work_keys.insert(key);
        self.works.push(PendingWorkCommand {
            key,
            priority: RenderSchedulingPolicy::work_priority_for_key(key, priority),
            sequence: self.next_sequence,
        });
        self.enforce_work_capacity();
    }

    fn clear_work(&mut self) {
        self.works.clear();
        self.work_keys.clear();
    }

    fn raise_work_priority(&mut self, key: QueueKey, priority: RenderPriority) {
        if let Some(work) = self.works.iter_mut().find(|work| work.key == key) {
            work.priority = work.priority.max(priority);
        }
    }

    fn enforce_work_capacity(&mut self) {
        while self.works.len() > self.work_capacity {
            let Some(index) = self.work_drop_candidate_index() else {
                break;
            };
            let removed = self.works.swap_remove(index);
            self.work_keys.remove(&removed.key);
        }
    }

    fn next_work_index(&self) -> Option<usize> {
        self.works
            .iter()
            .enumerate()
            .max_by_key(|(_, work)| {
                RenderSchedulingPolicy::work_pop_key(work.priority, work.sequence)
            })
            .map(|(index, _)| index)
    }

    fn work_drop_candidate_index(&self) -> Option<usize> {
        self.works
            .iter()
            .enumerate()
            .min_by_key(|(_, work)| {
                RenderSchedulingPolicy::work_drop_key(work.priority, work.sequence)
            })
            .map(|(index, _)| index)
    }

    fn has_shutdown(&self) -> bool {
        self.controls
            .iter()
            .any(|command| matches!(command, RenderCommand::Shutdown))
    }
}

impl Default for PendingRenderCommands {
    fn default() -> Self {
        Self::new(MAX_PENDING_RENDER_COMMANDS)
    }
}

impl PendingWorkCommand {
    fn into_command(self) -> RenderCommand {
        match self.key {
            QueueKey::Page {
                session_id,
                request,
            } => RenderCommand::RenderPage {
                session_id,
                request,
                priority: self.priority,
            },
            QueueKey::SpeakerNotes { session_id } => {
                RenderCommand::ExtractSpeakerNotes { session_id }
            }
        }
    }
}

#[derive(Debug, Default)]
struct RenderCommandMailbox {
    pending: Mutex<PendingRenderCommands>,
    available: Condvar,
}

impl RenderCommandMailbox {
    fn send(&self, command: RenderCommand) {
        let mut pending = self
            .pending
            .lock()
            .expect("render command mailbox poisoned");
        pending.push(command);
        self.available.notify_one();
    }

    fn recv(&self) -> Option<RenderCommand> {
        let mut pending = self
            .pending
            .lock()
            .expect("render command mailbox poisoned");
        while pending.is_empty() {
            pending = self
                .available
                .wait(pending)
                .expect("render command mailbox poisoned");
        }
        pending.pop()
    }

    fn try_recv(&self) -> Option<RenderCommand> {
        self.pending
            .lock()
            .expect("render command mailbox poisoned")
            .pop()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RenderEventKey {
    Open(RenderSessionId),
    SpeakerNotes(RenderSessionId),
    Page {
        session_id: RenderSessionId,
        request: RenderRequest,
    },
}

#[derive(Debug)]
struct PendingRenderEvents {
    events: VecDeque<RenderEvent>,
    capacity: usize,
}

impl PendingRenderEvents {
    fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::new(),
            capacity,
        }
    }

    fn push(&mut self, event: RenderEvent) {
        if self.capacity == 0 {
            return;
        }

        if let Some(key) = RenderSchedulingPolicy::event_key(&event) {
            if let Some(existing) = self
                .events
                .iter()
                .position(|queued| RenderSchedulingPolicy::event_key(queued) == Some(key))
            {
                self.events[existing] = event;
                return;
            }
        }

        if self.events.len() >= self.capacity && !self.make_room_for(&event) {
            return;
        }

        self.events.push_back(event);
    }

    fn drain(&mut self) -> Vec<RenderEvent> {
        self.events.drain(..).collect()
    }

    fn make_room_for(&mut self, event: &RenderEvent) -> bool {
        let incoming = RenderSchedulingPolicy::event_drop_score(event);
        let Some((index, score)) = self
            .events
            .iter()
            .enumerate()
            .max_by_key(|(_, event)| RenderSchedulingPolicy::event_drop_score(event))
            .map(|(index, event)| (index, RenderSchedulingPolicy::event_drop_score(event)))
        else {
            return true;
        };

        if score >= incoming {
            self.events.remove(index);
            true
        } else {
            false
        }
    }
}

impl Default for PendingRenderEvents {
    fn default() -> Self {
        Self::new(MAX_PENDING_RENDER_EVENTS)
    }
}

#[derive(Debug, Default)]
struct RenderEventMailbox {
    pending: Mutex<PendingRenderEvents>,
}

impl RenderEventMailbox {
    fn send(&self, event: RenderEvent) {
        self.pending
            .lock()
            .expect("render event mailbox poisoned")
            .push(event);
    }

    fn drain(&self) -> Vec<RenderEvent> {
        self.pending
            .lock()
            .expect("render event mailbox poisoned")
            .drain()
    }
}

#[derive(Debug, Default)]
struct RenderCancellation {
    active_session: AtomicU64,
    shutdown: AtomicBool,
}

impl RenderCancellation {
    fn activate(&self, session_id: RenderSessionId) {
        self.active_session.store(session_id.0, Ordering::Release);
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.active_session.store(0, Ordering::Release);
    }

    fn current_session(&self) -> Option<RenderSessionId> {
        let session_id = self.active_session.load(Ordering::Acquire);
        (session_id != 0).then_some(RenderSessionId(session_id))
    }

    fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    fn is_cancelled(&self, session_id: RenderSessionId) -> bool {
        self.is_shutdown() || self.active_session.load(Ordering::Acquire) != session_id.0
    }
}

trait RenderWorkerDocument {
    fn title(&self) -> String;
    fn page_count(&self) -> u32;
    fn render_page_pixels(&self, request: RenderRequest) -> anyhow::Result<RenderedPagePixels>;
    fn speaker_notes_for_page_cancellable(
        &self,
        page_index: u32,
        is_cancelled: &dyn Fn() -> bool,
    ) -> anyhow::Result<Option<Vec<(u32, String)>>>;
}

impl RenderWorkerDocument for PdfDocumentState {
    fn title(&self) -> String {
        self.title()
    }

    fn page_count(&self) -> u32 {
        self.page_count()
    }

    fn render_page_pixels(&self, request: RenderRequest) -> anyhow::Result<RenderedPagePixels> {
        render_page_pixels(self, request)
    }

    fn speaker_notes_for_page_cancellable(
        &self,
        page_index: u32,
        is_cancelled: &dyn Fn() -> bool,
    ) -> anyhow::Result<Option<Vec<(u32, String)>>> {
        self.speaker_notes_for_page_cancellable(page_index, is_cancelled)
    }
}

struct SpeakerNotesExtraction {
    session_id: RenderSessionId,
    next_page_index: u32,
    page_count: u32,
    page_notes: Vec<(u32, String)>,
}

struct RenderWorkerState<D> {
    document: Option<D>,
    active_session: Option<RenderSessionId>,
    notes_extraction: Option<SpeakerNotesExtraction>,
    queue: RenderQueue,
    shutdown: bool,
}

impl<D> Default for RenderWorkerState<D> {
    fn default() -> Self {
        Self {
            document: None,
            active_session: None,
            notes_extraction: None,
            queue: RenderQueue::default(),
            shutdown: false,
        }
    }
}

/// Serializes PDFium document access through one render worker.
///
/// The UI thread owns this scheduler handle and communicates through command and
/// event mailboxes. The loaded `PdfDocumentState` remains worker-local so current
/// slide rendering, next-page previews, thumbnails, preloading, and speaker-note
/// extraction do not access PDFium concurrently.
pub struct RenderScheduler {
    command_mailbox: Arc<RenderCommandMailbox>,
    event_mailbox: Arc<RenderEventMailbox>,
    cancellation: Arc<RenderCancellation>,
    lifecycle: Arc<Mutex<RenderWorkerLifecycle>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl RenderScheduler {
    /// Starts the single render worker that owns PDFium document access.
    pub fn start() -> Self {
        let command_mailbox = Arc::new(RenderCommandMailbox::default());
        let event_mailbox = Arc::new(RenderEventMailbox::default());
        let cancellation = Arc::new(RenderCancellation::default());
        let lifecycle = Arc::new(Mutex::new(RenderWorkerLifecycle::Running));
        let worker_command_mailbox = Arc::clone(&command_mailbox);
        let worker_event_mailbox = Arc::clone(&event_mailbox);
        let worker_cancellation = Arc::clone(&cancellation);
        let worker_lifecycle = Arc::clone(&lifecycle);
        let worker = thread::spawn(move || {
            run_render_worker_guarded(
                worker_command_mailbox,
                worker_event_mailbox,
                worker_cancellation,
                worker_lifecycle,
                render_worker,
            )
        });

        Self {
            command_mailbox,
            event_mailbox,
            cancellation,
            lifecycle,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn open(&self, session_id: RenderSessionId, path: PathBuf) {
        self.send(RenderCommand::Open { session_id, path });
    }

    pub fn render_page(
        &self,
        session_id: RenderSessionId,
        request: RenderRequest,
        priority: RenderPriority,
    ) {
        self.send(RenderCommand::RenderPage {
            session_id,
            request,
            priority,
        });
    }

    pub fn extract_speaker_notes(&self, session_id: RenderSessionId) {
        self.send(RenderCommand::ExtractSpeakerNotes { session_id });
    }

    pub fn drain_events(&self) -> Vec<RenderEvent> {
        self.event_mailbox.drain()
    }

    pub fn lifecycle(&self) -> RenderWorkerLifecycle {
        *self.lifecycle.lock().expect("render lifecycle poisoned")
    }

    pub fn request_shutdown(&self) {
        let should_send_shutdown = {
            let mut lifecycle = self.lifecycle.lock().expect("render lifecycle poisoned");
            match *lifecycle {
                RenderWorkerLifecycle::Running => {
                    *lifecycle = RenderWorkerLifecycle::ShutdownRequested;
                    true
                }
                RenderWorkerLifecycle::ShutdownRequested => true,
                RenderWorkerLifecycle::Stopped | RenderWorkerLifecycle::Failed => false,
            }
        };

        if should_send_shutdown {
            self.cancellation.shutdown();
            self.command_mailbox.send(RenderCommand::Shutdown);
        }
    }

    pub fn can_be_replaced(&self) -> bool {
        if !matches!(
            self.lifecycle(),
            RenderWorkerLifecycle::Stopped | RenderWorkerLifecycle::Failed
        ) {
            return false;
        }

        self.worker
            .lock()
            .expect("render worker handle poisoned")
            .as_ref()
            .is_none_or(JoinHandle::is_finished)
    }

    pub fn try_join_finished_worker(&self) -> bool {
        let mut worker = self.worker.lock().expect("render worker handle poisoned");
        let Some(handle) = worker.as_ref() else {
            return true;
        };
        if !handle.is_finished() {
            return false;
        }

        let handle = worker
            .take()
            .expect("render worker handle should exist after finished check");
        let _ = handle.join();
        true
    }

    #[cfg(test)]
    pub(crate) fn without_worker_for_test() -> Self {
        Self::without_worker_with_lifecycle_for_test(RenderWorkerLifecycle::Stopped)
    }

    #[cfg(test)]
    pub(crate) fn without_worker_with_lifecycle_for_test(lifecycle: RenderWorkerLifecycle) -> Self {
        Self {
            command_mailbox: Arc::new(RenderCommandMailbox::default()),
            event_mailbox: Arc::new(RenderEventMailbox::default()),
            cancellation: Arc::new(RenderCancellation::default()),
            lifecycle: Arc::new(Mutex::new(lifecycle)),
            worker: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub(crate) fn drain_commands_for_test(&self) -> Vec<RenderCommand> {
        let mut commands = Vec::new();
        while let Some(command) = self.command_mailbox.try_recv() {
            commands.push(command);
        }
        commands
    }

    #[cfg(test)]
    fn send_event_for_test(&self, event: RenderEvent) {
        self.event_mailbox.send(event);
    }

    fn send(&self, command: RenderCommand) {
        self.command_mailbox.send(command);
    }
}

impl Drop for RenderScheduler {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

fn run_render_worker_guarded(
    command_mailbox: Arc<RenderCommandMailbox>,
    event_mailbox: Arc<RenderEventMailbox>,
    cancellation: Arc<RenderCancellation>,
    lifecycle: Arc<Mutex<RenderWorkerLifecycle>>,
    worker: impl FnOnce(Arc<RenderCommandMailbox>, Arc<RenderEventMailbox>, Arc<RenderCancellation>),
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        worker(
            Arc::clone(&command_mailbox),
            Arc::clone(&event_mailbox),
            Arc::clone(&cancellation),
        );
    }));

    if cancellation.is_shutdown() {
        *lifecycle.lock().expect("render lifecycle poisoned") = RenderWorkerLifecycle::Stopped;
        return;
    }

    let message = match result {
        Ok(()) => "Render worker stopped unexpectedly.".to_owned(),
        Err(payload) => panic_payload_message(payload.as_ref())
            .map(|message| format!("Render worker failed: {message}"))
            .unwrap_or_else(|| "Render worker failed unexpectedly.".to_owned()),
    };

    *lifecycle.lock().expect("render lifecycle poisoned") = RenderWorkerLifecycle::Failed;
    event_mailbox.send(RenderEvent::WorkerFailed {
        session_id: cancellation.current_session(),
        message,
    });
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> Option<String> {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
}

fn render_worker(
    command_mailbox: Arc<RenderCommandMailbox>,
    event_mailbox: Arc<RenderEventMailbox>,
    cancellation: Arc<RenderCancellation>,
) {
    // Keep the PDF document worker-local. Render events carry only metadata,
    // pixels, and errors back to the UI thread.
    let mut state = RenderWorkerState::<PdfDocumentState>::default();

    while !state.shutdown {
        if let Some(command) = command_mailbox.recv() {
            handle_command(
                command,
                &mut state,
                &event_mailbox,
                &cancellation,
                PdfDocumentState::open,
            );
        }

        drain_pending_commands(
            &command_mailbox,
            &mut state,
            &event_mailbox,
            &cancellation,
            PdfDocumentState::open,
        );

        while !state.shutdown {
            if !process_next_work(&mut state, &event_mailbox, &cancellation) {
                break;
            }

            drain_pending_commands(
                &command_mailbox,
                &mut state,
                &event_mailbox,
                &cancellation,
                PdfDocumentState::open,
            );
        }
    }
}

fn process_next_work<D: RenderWorkerDocument>(
    state: &mut RenderWorkerState<D>,
    event_mailbox: &RenderEventMailbox,
    cancellation: &Arc<RenderCancellation>,
) -> bool {
    let Some((job_id, session_id, work)) = state.queue.pop() else {
        return false;
    };

    if state.active_session != Some(session_id) || cancellation.is_cancelled(session_id) {
        return true;
    }

    match work {
        RenderWork::Page(request) => {
            render_page_on_worker(
                state.document.as_ref(),
                session_id,
                job_id,
                request,
                event_mailbox,
                cancellation,
            );
        }
        RenderWork::SpeakerNotes => {
            process_speaker_notes_batch_on_worker(state, session_id, event_mailbox, cancellation);
        }
    }

    true
}

fn drain_pending_commands<D: RenderWorkerDocument>(
    command_mailbox: &RenderCommandMailbox,
    state: &mut RenderWorkerState<D>,
    event_mailbox: &RenderEventMailbox,
    cancellation: &Arc<RenderCancellation>,
    open_document: fn(PathBuf) -> anyhow::Result<D>,
) {
    while let Some(command) = command_mailbox.try_recv() {
        handle_command(command, state, event_mailbox, cancellation, open_document);
        if state.shutdown {
            break;
        }
    }
}

fn handle_command<D: RenderWorkerDocument>(
    command: RenderCommand,
    state: &mut RenderWorkerState<D>,
    event_mailbox: &RenderEventMailbox,
    cancellation: &Arc<RenderCancellation>,
    open_document: fn(PathBuf) -> anyhow::Result<D>,
) {
    match command {
        RenderCommand::Open { session_id, path } => {
            state.queue.clear();
            state.notes_extraction = None;
            open_document_on_worker(
                session_id,
                path,
                state,
                event_mailbox,
                cancellation,
                open_document,
            );
        }
        RenderCommand::RenderPage {
            session_id,
            request,
            priority,
        } => {
            state.queue.push(session_id, request, priority);
        }
        RenderCommand::ExtractSpeakerNotes { session_id } => {
            state.queue.push_speaker_notes(session_id);
        }
        RenderCommand::Shutdown => {
            cancellation.shutdown();
            state.shutdown = true;
            state.notes_extraction = None;
            state.queue.clear();
        }
    }
}

fn open_document_on_worker<D: RenderWorkerDocument>(
    session_id: RenderSessionId,
    path: PathBuf,
    state: &mut RenderWorkerState<D>,
    event_mailbox: &RenderEventMailbox,
    cancellation: &Arc<RenderCancellation>,
    open_document: fn(PathBuf) -> anyhow::Result<D>,
) {
    match open_document(path) {
        Ok(doc) => {
            let title = doc.title();
            let page_count = doc.page_count();
            state.document = Some(doc);
            state.active_session = Some(session_id);
            cancellation.activate(session_id);
            event_mailbox.send(RenderEvent::Opened {
                session_id,
                title,
                page_count,
                status_text: "Ready".to_owned(),
            });
        }
        Err(err) => {
            event_mailbox.send(RenderEvent::OpenFailed {
                session_id,
                message: format!("{err:#}"),
            });
        }
    }
}

fn render_page_on_worker<D: RenderWorkerDocument>(
    document: Option<&D>,
    session_id: RenderSessionId,
    job_id: RenderJobId,
    request: RenderRequest,
    event_mailbox: &RenderEventMailbox,
    cancellation: &RenderCancellation,
) {
    let event = match document {
        Some(document) => match document.render_page_pixels(request) {
            Ok(page) => RenderEvent::PageRendered {
                session_id,
                job_id,
                request,
                page,
            },
            Err(err) => RenderEvent::PageFailed {
                session_id,
                job_id,
                request,
                message: err.to_string(),
            },
        },
        None => RenderEvent::PageFailed {
            session_id,
            job_id,
            request,
            message: "missing open PDF for render request".to_owned(),
        },
    };

    if !cancellation.is_cancelled(session_id) {
        event_mailbox.send(event);
    }
}

fn process_speaker_notes_batch_on_worker<D: RenderWorkerDocument>(
    state: &mut RenderWorkerState<D>,
    session_id: RenderSessionId,
    event_mailbox: &RenderEventMailbox,
    cancellation: &RenderCancellation,
) {
    if state.document.is_none() {
        return;
    }

    if state
        .notes_extraction
        .as_ref()
        .is_none_or(|extraction| extraction.session_id != session_id)
    {
        let page_count = state
            .document
            .as_ref()
            .map(RenderWorkerDocument::page_count)
            .unwrap_or(0);
        state.notes_extraction = Some(SpeakerNotesExtraction {
            session_id,
            next_page_index: 0,
            page_count,
            page_notes: Vec::new(),
        });
    }

    let Some(extraction) = state.notes_extraction.as_ref() else {
        return;
    };

    if extraction.next_page_index >= extraction.page_count {
        finish_speaker_notes_extraction(state, session_id, event_mailbox, cancellation);
        return;
    }

    let page_index = extraction.next_page_index;
    let page_notes = match state
        .document
        .as_ref()
        .expect("speaker notes extraction requires an open document")
        .speaker_notes_for_page_cancellable(page_index, &|| cancellation.is_cancelled(session_id))
    {
        Ok(Some(page_notes)) => page_notes,
        Ok(None) => return,
        Err(err) => {
            state.notes_extraction = None;
            if !cancellation.is_cancelled(session_id) {
                event_mailbox.send(RenderEvent::SpeakerNotesLoaded {
                    session_id,
                    notes: SpeakerNotes::empty(),
                    status_text: speaker_notes_warning(&err).text().to_owned(),
                });
            }
            return;
        }
    };

    let Some(extraction) = state.notes_extraction.as_mut() else {
        return;
    };

    extraction.page_notes.extend(page_notes);
    extraction.next_page_index = extraction.next_page_index.saturating_add(1);

    if extraction.next_page_index >= extraction.page_count {
        finish_speaker_notes_extraction(state, session_id, event_mailbox, cancellation);
    } else if !cancellation.is_cancelled(session_id) {
        state.queue.push_speaker_notes(session_id);
    }
}

fn finish_speaker_notes_extraction<D>(
    state: &mut RenderWorkerState<D>,
    session_id: RenderSessionId,
    event_mailbox: &RenderEventMailbox,
    cancellation: &RenderCancellation,
) {
    let Some(extraction) = state.notes_extraction.take() else {
        return;
    };

    if extraction.session_id != session_id {
        return;
    }

    let notes = SpeakerNotes::from_page_notes(extraction.page_notes);
    if !cancellation.is_cancelled(session_id) {
        event_mailbox.send(RenderEvent::SpeakerNotesLoaded {
            session_id,
            notes,
            status_text: "Ready".to_owned(),
        });
    }
}

fn render_page_pixels(
    document: &PdfDocumentState,
    request: RenderRequest,
) -> anyhow::Result<RenderedPagePixels> {
    let aspect_ratio = document.page_aspect_ratio(request.page_index)?;
    let pixels = document.render_page_pixels(request.page_index, request.width)?;

    Ok(RenderedPagePixels {
        estimated_bytes: actual_render_bytes(&pixels),
        pixels,
        aspect_ratio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::RenderPurpose;
    use anyhow::{bail, Result};
    use slint::{Rgba8Pixel, SharedPixelBuffer};

    #[derive(Debug)]
    struct FakeDocument {
        title: String,
        page_count: u32,
        render_fails: bool,
        notes: FakeNotes,
    }

    #[derive(Debug)]
    enum FakeNotes {
        Loaded(SpeakerNotes),
        Empty,
        Fails,
    }

    impl RenderWorkerDocument for FakeDocument {
        fn title(&self) -> String {
            self.title.clone()
        }

        fn page_count(&self) -> u32 {
            self.page_count
        }

        fn render_page_pixels(&self, request: RenderRequest) -> Result<RenderedPagePixels> {
            if self.render_fails {
                bail!("fake render failed");
            }

            let pixels = SharedPixelBuffer::<Rgba8Pixel>::new(request.width as u32, 1);
            Ok(RenderedPagePixels {
                estimated_bytes: actual_render_bytes(&pixels),
                pixels,
                aspect_ratio: request.width as f32,
            })
        }

        fn speaker_notes_for_page_cancellable(
            &self,
            page_index: u32,
            is_cancelled: &dyn Fn() -> bool,
        ) -> Result<Option<Vec<(u32, String)>>> {
            if is_cancelled() {
                return Ok(None);
            }

            match &self.notes {
                FakeNotes::Loaded(notes) => {
                    let page_number = page_index + 1;
                    Ok(Some(
                        notes
                            .note_for_page_number(page_number)
                            .map(|note| vec![(page_number, note.to_owned())])
                            .unwrap_or_default(),
                    ))
                }
                FakeNotes::Empty => Ok(Some(Vec::new())),
                FakeNotes::Fails => bail!("fake notes failed"),
            }
        }
    }

    fn open_fake_document(path: PathBuf) -> Result<FakeDocument> {
        match path.to_string_lossy().as_ref() {
            "fail.pdf" => bail!("fake open failed"),
            "render-fail.pdf" => Ok(FakeDocument {
                title: "render-fail.pdf".to_owned(),
                page_count: 3,
                render_fails: true,
                notes: FakeNotes::Empty,
            }),
            "notes.pdf" => Ok(FakeDocument {
                title: "notes.pdf".to_owned(),
                page_count: 3,
                render_fails: false,
                notes: FakeNotes::Loaded(SpeakerNotes::from_page_notes([(
                    2,
                    "Presenter note".to_owned(),
                )])),
            }),
            "notes-fail.pdf" => Ok(FakeDocument {
                title: "notes-fail.pdf".to_owned(),
                page_count: 3,
                render_fails: false,
                notes: FakeNotes::Fails,
            }),
            _ => Ok(FakeDocument {
                title: "deck.pdf".to_owned(),
                page_count: 3,
                render_fails: false,
                notes: FakeNotes::Empty,
            }),
        }
    }

    fn fake_worker_parts() -> (
        RenderWorkerState<FakeDocument>,
        RenderEventMailbox,
        Arc<RenderCancellation>,
    ) {
        (
            RenderWorkerState::default(),
            RenderEventMailbox::default(),
            Arc::new(RenderCancellation::default()),
        )
    }

    fn handle_fake_command(
        command: RenderCommand,
        state: &mut RenderWorkerState<FakeDocument>,
        event_mailbox: &RenderEventMailbox,
        cancellation: &Arc<RenderCancellation>,
    ) {
        handle_command(
            command,
            state,
            event_mailbox,
            cancellation,
            open_fake_document,
        );
    }

    fn request(page_index: u32, purpose: RenderPurpose) -> RenderRequest {
        RenderRequest {
            page_index,
            width: 100,
            purpose,
        }
    }

    fn pop_page_command(
        command: RenderCommand,
    ) -> (RenderSessionId, RenderRequest, RenderPriority) {
        match command {
            RenderCommand::RenderPage {
                session_id,
                request,
                priority,
            } => (session_id, request, priority),
            other => panic!("expected render page command, got {other:?}"),
        }
    }

    fn pop_speaker_notes_command(command: RenderCommand) -> RenderSessionId {
        match command {
            RenderCommand::ExtractSpeakerNotes { session_id } => session_id,
            other => panic!("expected speaker notes command, got {other:?}"),
        }
    }

    fn drain_single_event(event_mailbox: &RenderEventMailbox) -> RenderEvent {
        let events = event_mailbox.drain();
        assert_eq!(events.len(), 1);
        events.into_iter().next().unwrap()
    }

    fn process_until_single_event(
        state: &mut RenderWorkerState<FakeDocument>,
        event_mailbox: &RenderEventMailbox,
        cancellation: &Arc<RenderCancellation>,
    ) -> RenderEvent {
        for _ in 0..16 {
            assert!(process_next_work(state, event_mailbox, cancellation));
            let events = event_mailbox.drain();
            if let Some(event) = events.into_iter().next() {
                return event;
            }
        }

        panic!("expected render event after processing queued work");
    }

    #[test]
    fn session_tracker_accepts_only_committed_session() {
        let mut tracker = RenderSessionTracker::default();
        let first = tracker.begin_open_session();
        let second = tracker.begin_open_session();

        assert!(!tracker.accepts(first));
        assert!(!tracker.accepts(second));
        assert!(!tracker.commit_pending_open(first));
        assert!(tracker.commit_pending_open(second));
        assert!(!tracker.accepts_pending_open(second));
        assert!(tracker.accepts(second));
        assert_eq!(tracker.current_session(), Some(second));
    }

    #[test]
    fn session_tracker_worker_failure_clears_committed_and_pending_sessions() {
        let mut tracker = RenderSessionTracker::default();
        let current = tracker.begin_open_session();
        assert!(tracker.commit_pending_open(current));
        let pending = tracker.begin_open_session();

        assert!(tracker.mark_worker_failed(Some(current)));

        assert_eq!(tracker.current_session(), None);
        assert!(!tracker.accepts_pending_open(pending));
    }

    #[test]
    fn session_tracker_ignores_worker_failure_for_unknown_session() {
        let mut tracker = RenderSessionTracker::default();
        let current = tracker.begin_open_session();
        assert!(tracker.commit_pending_open(current));

        assert!(!tracker.mark_worker_failed(Some(RenderSessionId(99))));

        assert_eq!(tracker.current_session(), Some(current));
    }

    #[test]
    fn queue_pops_highest_priority_first() {
        let session = RenderSessionId(1);
        let mut queue = RenderQueue::default();
        let thumbnail = request(2, RenderPurpose::Thumbnail);
        let current = request(1, RenderPurpose::CurrentSlide);
        let preview = request(3, RenderPurpose::NextPreview);

        queue.push(session, thumbnail, RenderPriority::Background);
        queue.push(session, current, RenderPriority::BlockingVisible);
        queue.push(session, preview, RenderPriority::VisibleAux);

        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(current));
        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(preview));
        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(thumbnail));
        assert!(queue.is_empty());
    }

    #[test]
    fn queue_preserves_fifo_order_within_priority() {
        let session = RenderSessionId(1);
        let mut queue = RenderQueue::default();
        let first = request(1, RenderPurpose::Thumbnail);
        let second = request(2, RenderPurpose::Thumbnail);

        queue.push(session, first, RenderPriority::Background);
        queue.push(session, second, RenderPriority::Background);

        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(first));
        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(second));
    }

    #[test]
    fn duplicate_request_is_not_queued_twice_but_can_raise_priority() {
        let session = RenderSessionId(1);
        let mut queue = RenderQueue::default();
        let current = request(1, RenderPurpose::CurrentSlide);
        let other = request(2, RenderPurpose::CurrentSlide);

        assert!(queue
            .push(session, current, RenderPriority::Background)
            .is_some());
        assert!(queue
            .push(session, other, RenderPriority::VisibleAux)
            .is_some());
        assert!(queue
            .push(session, current, RenderPriority::BlockingVisible)
            .is_none());

        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(current));
        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(other));
        assert!(queue.is_empty());
    }

    #[test]
    fn speaker_notes_job_runs_after_visible_render_work() {
        let session = RenderSessionId(1);
        let mut queue = RenderQueue::default();
        let current = request(1, RenderPurpose::CurrentSlide);

        queue.push_speaker_notes(session);
        queue.push(session, current, RenderPriority::BlockingVisible);
        queue.push_speaker_notes(session);

        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(current));
        assert_eq!(queue.pop().unwrap().2, RenderWork::SpeakerNotes);
        assert!(queue.is_empty());
    }

    #[test]
    fn cancellation_tracks_superseded_sessions() {
        let cancellation = RenderCancellation::default();
        let first = RenderSessionId(1);
        let second = RenderSessionId(2);

        cancellation.activate(first);
        assert!(!cancellation.is_cancelled(first));

        cancellation.activate(second);
        assert!(cancellation.is_cancelled(first));
        assert!(!cancellation.is_cancelled(second));

        cancellation.shutdown();
        assert!(cancellation.is_cancelled(second));
    }

    #[test]
    fn command_mailbox_coalesces_duplicate_render_work_and_raises_priority() {
        let session = RenderSessionId(1);
        let current = request(1, RenderPurpose::CurrentSlide);
        let mut pending = PendingRenderCommands::new(8);

        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: current,
            priority: RenderPriority::Background,
        });
        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: current,
            priority: RenderPriority::BlockingVisible,
        });

        assert_eq!(pending.works.len(), 1);
        let (popped_session, popped_request, popped_priority) =
            pop_page_command(pending.pop().unwrap());
        assert_eq!(popped_session, session);
        assert_eq!(popped_request, current);
        assert_eq!(popped_priority, RenderPriority::BlockingVisible);
        assert!(pending.is_empty());
    }

    #[test]
    fn command_mailbox_uses_worker_queue_work_order_policy() {
        let session = RenderSessionId(1);
        let thumbnail = request(2, RenderPurpose::Thumbnail);
        let current = request(1, RenderPurpose::CurrentSlide);
        let preview = request(3, RenderPurpose::NextPreview);
        let mut pending = PendingRenderCommands::new(8);
        let mut queue = RenderQueue::default();

        for (request, priority) in [
            (thumbnail, RenderPriority::Background),
            (current, RenderPriority::BlockingVisible),
            (preview, RenderPriority::VisibleAux),
        ] {
            pending.push(RenderCommand::RenderPage {
                session_id: session,
                request,
                priority,
            });
            queue.push(session, request, priority);
        }
        pending.push(RenderCommand::ExtractSpeakerNotes {
            session_id: session,
        });
        queue.push_speaker_notes(session);

        assert_eq!(pop_page_command(pending.pop().unwrap()).1, current);
        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(current));
        assert_eq!(pop_page_command(pending.pop().unwrap()).1, preview);
        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(preview));
        assert_eq!(pop_page_command(pending.pop().unwrap()).1, thumbnail);
        assert_eq!(queue.pop().unwrap().2, RenderWork::Page(thumbnail));
        assert_eq!(pop_speaker_notes_command(pending.pop().unwrap()), session);
        assert_eq!(queue.pop().unwrap().2, RenderWork::SpeakerNotes);
    }

    #[test]
    fn command_mailbox_drops_low_priority_work_when_capacity_is_reached() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderCommands::new(2);
        let background = request(1, RenderPurpose::Thumbnail);
        let warm = request(2, RenderPurpose::CurrentSlide);
        let visible = request(3, RenderPurpose::CurrentSlide);

        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: background,
            priority: RenderPriority::Background,
        });
        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: warm,
            priority: RenderPriority::Warm,
        });
        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: visible,
            priority: RenderPriority::BlockingVisible,
        });

        assert_eq!(pending.works.len(), 2);
        assert!(!pending.work_keys.contains(&QueueKey::Page {
            session_id: session,
            request: background,
        }));

        assert_eq!(pop_page_command(pending.pop().unwrap()).1, visible);
        assert_eq!(pop_page_command(pending.pop().unwrap()).1, warm);
        assert!(pending.is_empty());
    }

    #[test]
    fn command_mailbox_drops_oldest_equal_priority_work_when_capacity_is_reached() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderCommands::new(2);
        let first_thumbnail = request(1, RenderPurpose::Thumbnail);
        let second_thumbnail = request(2, RenderPurpose::Thumbnail);
        let warm = request(3, RenderPurpose::CurrentSlide);

        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: first_thumbnail,
            priority: RenderPriority::Background,
        });
        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: second_thumbnail,
            priority: RenderPriority::Background,
        });
        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: warm,
            priority: RenderPriority::Warm,
        });

        assert_eq!(pending.works.len(), 2);
        assert!(!pending.work_keys.contains(&QueueKey::Page {
            session_id: session,
            request: first_thumbnail,
        }));
        assert_eq!(pop_page_command(pending.pop().unwrap()).1, warm);
        assert_eq!(pop_page_command(pending.pop().unwrap()).1, second_thumbnail);
        assert!(pending.is_empty());
    }

    #[test]
    fn command_mailbox_open_replaces_stale_pending_work() {
        let first_session = RenderSessionId(1);
        let second_session = RenderSessionId(2);
        let mut pending = PendingRenderCommands::new(8);

        pending.push(RenderCommand::RenderPage {
            session_id: first_session,
            request: request(1, RenderPurpose::Thumbnail),
            priority: RenderPriority::Background,
        });
        pending.push(RenderCommand::Open {
            session_id: second_session,
            path: PathBuf::from("new.pdf"),
        });

        assert!(pending.works.is_empty());
        match pending.pop().unwrap() {
            RenderCommand::Open { session_id, path } => {
                assert_eq!(session_id, second_session);
                assert_eq!(path, PathBuf::from("new.pdf"));
            }
            other => panic!("expected open command, got {other:?}"),
        }
        assert!(pending.is_empty());
    }

    #[test]
    fn command_mailbox_shutdown_clears_backlog_and_ignores_later_commands() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderCommands::new(8);

        pending.push(RenderCommand::Open {
            session_id: session,
            path: PathBuf::from("deck.pdf"),
        });
        pending.push(RenderCommand::RenderPage {
            session_id: session,
            request: request(1, RenderPurpose::CurrentSlide),
            priority: RenderPriority::BlockingVisible,
        });
        pending.push(RenderCommand::Shutdown);
        pending.push(RenderCommand::Open {
            session_id: RenderSessionId(2),
            path: PathBuf::from("ignored.pdf"),
        });

        assert!(pending.works.is_empty());
        assert_eq!(pending.controls.len(), 1);
        assert!(matches!(pending.pop().unwrap(), RenderCommand::Shutdown));
        assert!(pending.is_empty());
    }

    #[test]
    fn command_mailbox_shutdown_after_backlog_discards_pending_work() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderCommands::new(8);

        for page_index in 0..4 {
            pending.push(RenderCommand::RenderPage {
                session_id: session,
                request: request(page_index, RenderPurpose::CurrentSlide),
                priority: RenderPriority::Warm,
            });
        }
        pending.push(RenderCommand::ExtractSpeakerNotes {
            session_id: session,
        });
        pending.push(RenderCommand::Shutdown);

        assert!(pending.works.is_empty());
        assert_eq!(pending.controls.len(), 1);
        assert!(matches!(pending.pop().unwrap(), RenderCommand::Shutdown));
        assert!(pending.is_empty());
    }

    #[test]
    fn event_mailbox_coalesces_page_events_by_session_and_request() {
        let session = RenderSessionId(1);
        let current = request(1, RenderPurpose::CurrentSlide);
        let mut pending = PendingRenderEvents::new(8);

        pending.push(RenderEvent::PageFailed {
            session_id: session,
            job_id: RenderJobId(1),
            request: current,
            message: "first".to_owned(),
        });
        pending.push(RenderEvent::PageFailed {
            session_id: session,
            job_id: RenderJobId(2),
            request: current,
            message: "second".to_owned(),
        });

        let events = pending.drain();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RenderEvent::PageFailed {
                job_id, message, ..
            } => {
                assert_eq!(*job_id, RenderJobId(2));
                assert_eq!(message, "second");
            }
            other => panic!("expected page failed event, got {other:?}"),
        }
    }

    #[test]
    fn event_mailbox_preserves_open_result_over_visible_event_when_full() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderEvents::new(1);

        pending.push(RenderEvent::PageFailed {
            session_id: session,
            job_id: RenderJobId(1),
            request: request(1, RenderPurpose::CurrentSlide),
            message: "current".to_owned(),
        });
        pending.push(RenderEvent::OpenFailed {
            session_id: session,
            message: "open failed".to_owned(),
        });

        let events = pending.drain();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RenderEvent::OpenFailed {
                session_id: event_session,
                message,
            } => {
                assert_eq!(*event_session, session);
                assert_eq!(message, "open failed");
            }
            other => panic!("expected open failed event, got {other:?}"),
        }
    }

    #[test]
    fn event_mailbox_preserves_speaker_notes_over_visible_event_when_full() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderEvents::new(1);

        pending.push(RenderEvent::PageFailed {
            session_id: session,
            job_id: RenderJobId(1),
            request: request(1, RenderPurpose::CurrentSlide),
            message: "current".to_owned(),
        });
        pending.push(RenderEvent::SpeakerNotesLoaded {
            session_id: session,
            notes: SpeakerNotes::empty(),
            status_text: "Ready".to_owned(),
        });

        let events = pending.drain();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RenderEvent::SpeakerNotesLoaded {
                session_id: event_session,
                status_text,
                ..
            } => {
                assert_eq!(*event_session, session);
                assert_eq!(status_text, "Ready");
            }
            other => panic!("expected speaker notes loaded event, got {other:?}"),
        }
    }

    #[test]
    fn event_mailbox_preserves_visible_event_over_thumbnail_when_full() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderEvents::new(1);

        pending.push(RenderEvent::PageFailed {
            session_id: session,
            job_id: RenderJobId(1),
            request: request(1, RenderPurpose::Thumbnail),
            message: "thumbnail".to_owned(),
        });
        pending.push(RenderEvent::PageFailed {
            session_id: session,
            job_id: RenderJobId(2),
            request: request(2, RenderPurpose::CurrentSlide),
            message: "current".to_owned(),
        });

        let events = pending.drain();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RenderEvent::PageFailed {
                job_id, request, ..
            } => {
                assert_eq!(*job_id, RenderJobId(2));
                assert_eq!(request.purpose, RenderPurpose::CurrentSlide);
            }
            other => panic!("expected page failed event, got {other:?}"),
        }
    }

    #[test]
    fn event_mailbox_preserves_worker_failure_when_full() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderEvents::new(1);

        pending.push(RenderEvent::PageFailed {
            session_id: session,
            job_id: RenderJobId(1),
            request: request(1, RenderPurpose::CurrentSlide),
            message: "current".to_owned(),
        });
        pending.push(RenderEvent::WorkerFailed {
            session_id: Some(session),
            message: "worker failed".to_owned(),
        });

        let events = pending.drain();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RenderEvent::WorkerFailed {
                session_id,
                message,
            } => {
                assert_eq!(*session_id, Some(session));
                assert_eq!(message, "worker failed");
            }
            other => panic!("expected worker failed event, got {other:?}"),
        }
    }

    #[test]
    fn event_mailbox_drops_lower_value_incoming_event_when_full() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderEvents::new(1);

        pending.push(RenderEvent::OpenFailed {
            session_id: session,
            message: "open failed".to_owned(),
        });
        pending.push(RenderEvent::PageFailed {
            session_id: session,
            job_id: RenderJobId(1),
            request: request(1, RenderPurpose::Thumbnail),
            message: "thumbnail failed".to_owned(),
        });

        let events = pending.drain();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RenderEvent::OpenFailed { message, .. } => {
                assert_eq!(message, "open failed");
            }
            other => panic!("expected open failed event, got {other:?}"),
        }
    }

    #[test]
    fn event_mailbox_with_zero_capacity_drops_every_event() {
        let session = RenderSessionId(1);
        let mut pending = PendingRenderEvents::new(0);

        pending.push(RenderEvent::WorkerFailed {
            session_id: Some(session),
            message: "worker failed".to_owned(),
        });

        assert!(pending.drain().is_empty());
    }

    #[test]
    fn event_mailbox_drain_clears_events() {
        let session = RenderSessionId(1);
        let mailbox = RenderEventMailbox::default();

        mailbox.send(RenderEvent::OpenFailed {
            session_id: session,
            message: "open failed".to_owned(),
        });

        assert_eq!(mailbox.drain().len(), 1);
        assert!(mailbox.drain().is_empty());
    }

    #[test]
    fn public_scheduler_methods_send_commands_without_worker() {
        let session = RenderSessionId(1);
        let scheduler = RenderScheduler::without_worker_for_test();
        let current = request(1, RenderPurpose::CurrentSlide);

        scheduler.open(session, PathBuf::from("deck.pdf"));
        scheduler.render_page(session, current, RenderPriority::BlockingVisible);
        scheduler.extract_speaker_notes(session);

        let commands = scheduler.drain_commands_for_test();
        assert_eq!(commands.len(), 3);
        match &commands[0] {
            RenderCommand::Open { session_id, path } => {
                assert_eq!(*session_id, session);
                assert_eq!(path, &PathBuf::from("deck.pdf"));
            }
            other => panic!("expected open command, got {other:?}"),
        }
        match &commands[1] {
            RenderCommand::RenderPage {
                session_id,
                request,
                priority,
            } => {
                assert_eq!(*session_id, session);
                assert_eq!(*request, current);
                assert_eq!(*priority, RenderPriority::BlockingVisible);
            }
            other => panic!("expected render page command, got {other:?}"),
        }
        assert!(matches!(
            commands[2],
            RenderCommand::ExtractSpeakerNotes { session_id } if session_id == session
        ));
    }

    #[test]
    fn public_scheduler_drain_events_clears_queued_events() {
        let session = RenderSessionId(1);
        let scheduler = RenderScheduler::without_worker_for_test();

        scheduler.send_event_for_test(RenderEvent::OpenFailed {
            session_id: session,
            message: "open failed".to_owned(),
        });

        assert_eq!(scheduler.drain_events().len(), 1);
        assert!(scheduler.drain_events().is_empty());
    }

    #[test]
    fn dropping_scheduler_without_worker_sends_shutdown_and_cancels_session() {
        let session = RenderSessionId(1);
        let command_mailbox = Arc::new(RenderCommandMailbox::default());
        let event_mailbox = Arc::new(RenderEventMailbox::default());
        let cancellation = Arc::new(RenderCancellation::default());
        let lifecycle = Arc::new(Mutex::new(RenderWorkerLifecycle::Running));
        cancellation.activate(session);

        let scheduler = RenderScheduler {
            command_mailbox: Arc::clone(&command_mailbox),
            event_mailbox,
            cancellation: Arc::clone(&cancellation),
            lifecycle: Arc::clone(&lifecycle),
            worker: Mutex::new(None),
        };

        drop(scheduler);

        assert!(cancellation.is_cancelled(session));
        assert_eq!(
            *lifecycle.lock().expect("render lifecycle poisoned"),
            RenderWorkerLifecycle::ShutdownRequested
        );
        assert!(matches!(
            command_mailbox.try_recv(),
            Some(RenderCommand::Shutdown)
        ));
    }

    #[test]
    fn guarded_worker_panic_emits_worker_failed_event_for_active_session() {
        let session = RenderSessionId(1);
        let command_mailbox = Arc::new(RenderCommandMailbox::default());
        let event_mailbox = Arc::new(RenderEventMailbox::default());
        let cancellation = Arc::new(RenderCancellation::default());
        let lifecycle = Arc::new(Mutex::new(RenderWorkerLifecycle::Running));
        cancellation.activate(session);

        run_render_worker_guarded(
            command_mailbox,
            Arc::clone(&event_mailbox),
            cancellation,
            Arc::clone(&lifecycle),
            |_, _, _| panic!("fake worker panic"),
        );

        assert_eq!(
            *lifecycle.lock().expect("render lifecycle poisoned"),
            RenderWorkerLifecycle::Failed
        );
        match drain_single_event(&event_mailbox) {
            RenderEvent::WorkerFailed {
                session_id,
                message,
            } => {
                assert_eq!(session_id, Some(session));
                assert!(message.contains("fake worker panic"));
            }
            other => panic!("expected worker failed event, got {other:?}"),
        }
    }

    #[test]
    fn guarded_worker_shutdown_does_not_emit_worker_failed_event() {
        let command_mailbox = Arc::new(RenderCommandMailbox::default());
        let event_mailbox = Arc::new(RenderEventMailbox::default());
        let cancellation = Arc::new(RenderCancellation::default());
        let lifecycle = Arc::new(Mutex::new(RenderWorkerLifecycle::Running));

        run_render_worker_guarded(
            command_mailbox,
            Arc::clone(&event_mailbox),
            Arc::clone(&cancellation),
            Arc::clone(&lifecycle),
            |_, _, cancellation| cancellation.shutdown(),
        );

        assert_eq!(
            *lifecycle.lock().expect("render lifecycle poisoned"),
            RenderWorkerLifecycle::Stopped
        );
        assert!(event_mailbox.drain().is_empty());
    }

    #[test]
    fn scheduler_without_worker_is_immediately_replaceable() {
        let scheduler = RenderScheduler::without_worker_for_test();

        assert_eq!(scheduler.lifecycle(), RenderWorkerLifecycle::Stopped);
        assert!(scheduler.can_be_replaced());
        assert!(scheduler.try_join_finished_worker());
    }

    #[test]
    fn open_command_emits_opened_event_and_sets_active_document() {
        let session = RenderSessionId(1);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();

        handle_fake_command(
            RenderCommand::Open {
                session_id: session,
                path: PathBuf::from("deck.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert_eq!(state.active_session, Some(session));
        assert!(state.document.is_some());
        match drain_single_event(&event_mailbox) {
            RenderEvent::Opened {
                session_id,
                title,
                page_count,
                status_text,
            } => {
                assert_eq!(session_id, session);
                assert_eq!(title, "deck.pdf");
                assert_eq!(page_count, 3);
                assert_eq!(status_text, "Ready");
            }
            other => panic!("expected opened event, got {other:?}"),
        }
    }

    #[test]
    fn open_command_emits_open_failed_event() {
        let session = RenderSessionId(1);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();

        handle_fake_command(
            RenderCommand::Open {
                session_id: session,
                path: PathBuf::from("fail.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert_eq!(state.active_session, None);
        assert!(state.document.is_none());
        match drain_single_event(&event_mailbox) {
            RenderEvent::OpenFailed {
                session_id,
                message,
            } => {
                assert_eq!(session_id, session);
                assert!(message.contains("fake open failed"));
            }
            other => panic!("expected open failed event, got {other:?}"),
        }
    }

    #[test]
    fn failed_replacement_open_keeps_active_document_renderable() {
        let current_session = RenderSessionId(1);
        let pending_session = RenderSessionId(2);
        let current = request(1, RenderPurpose::CurrentSlide);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();

        handle_fake_command(
            RenderCommand::Open {
                session_id: current_session,
                path: PathBuf::from("deck.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );
        event_mailbox.drain();

        handle_fake_command(
            RenderCommand::Open {
                session_id: pending_session,
                path: PathBuf::from("fail.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert_eq!(state.active_session, Some(current_session));
        assert!(state.document.is_some());
        match drain_single_event(&event_mailbox) {
            RenderEvent::OpenFailed { session_id, .. } => {
                assert_eq!(session_id, pending_session);
            }
            other => panic!("expected open failed event, got {other:?}"),
        }

        handle_fake_command(
            RenderCommand::RenderPage {
                session_id: current_session,
                request: current,
                priority: RenderPriority::BlockingVisible,
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert!(process_next_work(&mut state, &event_mailbox, &cancellation));
        match drain_single_event(&event_mailbox) {
            RenderEvent::PageRendered {
                session_id,
                request,
                ..
            } => {
                assert_eq!(session_id, current_session);
                assert_eq!(request, current);
            }
            other => panic!("expected page rendered event, got {other:?}"),
        }
    }

    #[test]
    fn render_work_emits_page_rendered_event() {
        let session = RenderSessionId(1);
        let current = request(1, RenderPurpose::CurrentSlide);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();

        handle_fake_command(
            RenderCommand::Open {
                session_id: session,
                path: PathBuf::from("deck.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );
        event_mailbox.drain();
        handle_fake_command(
            RenderCommand::RenderPage {
                session_id: session,
                request: current,
                priority: RenderPriority::BlockingVisible,
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert!(process_next_work(&mut state, &event_mailbox, &cancellation));
        match drain_single_event(&event_mailbox) {
            RenderEvent::PageRendered {
                session_id,
                request,
                page,
                ..
            } => {
                assert_eq!(session_id, session);
                assert_eq!(request, current);
                assert_eq!(page.aspect_ratio, 100.0);
            }
            other => panic!("expected page rendered event, got {other:?}"),
        }
    }

    #[test]
    fn render_work_without_document_emits_page_failed_event() {
        let session = RenderSessionId(1);
        let current = request(1, RenderPurpose::CurrentSlide);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();
        state.active_session = Some(session);
        cancellation.activate(session);

        handle_fake_command(
            RenderCommand::RenderPage {
                session_id: session,
                request: current,
                priority: RenderPriority::BlockingVisible,
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert!(process_next_work(&mut state, &event_mailbox, &cancellation));
        match drain_single_event(&event_mailbox) {
            RenderEvent::PageFailed {
                session_id,
                request,
                message,
                ..
            } => {
                assert_eq!(session_id, session);
                assert_eq!(request, current);
                assert!(message.contains("missing open PDF"));
            }
            other => panic!("expected page failed event, got {other:?}"),
        }
    }

    #[test]
    fn stale_render_work_is_dropped_without_event() {
        let first_session = RenderSessionId(1);
        let second_session = RenderSessionId(2);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();
        state.active_session = Some(second_session);
        cancellation.activate(second_session);

        state.queue.push(
            first_session,
            request(1, RenderPurpose::CurrentSlide),
            RenderPriority::BlockingVisible,
        );

        assert!(process_next_work(&mut state, &event_mailbox, &cancellation));
        assert!(event_mailbox.drain().is_empty());
    }

    #[test]
    fn speaker_notes_work_emits_loaded_notes_event() {
        let session = RenderSessionId(1);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();

        handle_fake_command(
            RenderCommand::Open {
                session_id: session,
                path: PathBuf::from("notes.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );
        event_mailbox.drain();
        handle_fake_command(
            RenderCommand::ExtractSpeakerNotes {
                session_id: session,
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        match process_until_single_event(&mut state, &event_mailbox, &cancellation) {
            RenderEvent::SpeakerNotesLoaded {
                session_id,
                notes,
                status_text,
            } => {
                assert_eq!(session_id, session);
                assert_eq!(notes.note_for_page_number(2), Some("Presenter note"));
                assert_eq!(status_text, "Ready");
            }
            other => panic!("expected speaker notes event, got {other:?}"),
        }
    }

    #[test]
    fn speaker_notes_work_yields_to_visible_render_work_between_pages() {
        let session = RenderSessionId(1);
        let current = request(2, RenderPurpose::CurrentSlide);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();

        handle_fake_command(
            RenderCommand::Open {
                session_id: session,
                path: PathBuf::from("notes.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );
        event_mailbox.drain();
        handle_fake_command(
            RenderCommand::ExtractSpeakerNotes {
                session_id: session,
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert!(process_next_work(&mut state, &event_mailbox, &cancellation));
        assert!(event_mailbox.drain().is_empty());
        assert!(state.notes_extraction.is_some());

        handle_fake_command(
            RenderCommand::RenderPage {
                session_id: session,
                request: current,
                priority: RenderPriority::BlockingVisible,
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert!(process_next_work(&mut state, &event_mailbox, &cancellation));
        match drain_single_event(&event_mailbox) {
            RenderEvent::PageRendered {
                session_id,
                request,
                ..
            } => {
                assert_eq!(session_id, session);
                assert_eq!(request, current);
            }
            other => {
                panic!("expected visible page render before notes continuation, got {other:?}")
            }
        }
    }

    #[test]
    fn open_command_clears_in_progress_speaker_notes_extraction() {
        let first_session = RenderSessionId(1);
        let second_session = RenderSessionId(2);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();

        handle_fake_command(
            RenderCommand::Open {
                session_id: first_session,
                path: PathBuf::from("notes.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );
        event_mailbox.drain();
        handle_fake_command(
            RenderCommand::ExtractSpeakerNotes {
                session_id: first_session,
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );
        assert!(process_next_work(&mut state, &event_mailbox, &cancellation));
        assert!(state.notes_extraction.is_some());

        handle_fake_command(
            RenderCommand::Open {
                session_id: second_session,
                path: PathBuf::from("deck.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert!(state.notes_extraction.is_none());
        assert_eq!(state.active_session, Some(second_session));
        assert!(cancellation.is_cancelled(first_session));
    }

    #[test]
    fn speaker_notes_failure_emits_warning_event() {
        let session = RenderSessionId(1);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();

        handle_fake_command(
            RenderCommand::Open {
                session_id: session,
                path: PathBuf::from("notes-fail.pdf"),
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );
        event_mailbox.drain();
        handle_fake_command(
            RenderCommand::ExtractSpeakerNotes {
                session_id: session,
            },
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        match process_until_single_event(&mut state, &event_mailbox, &cancellation) {
            RenderEvent::SpeakerNotesLoaded {
                session_id,
                notes,
                status_text,
            } => {
                assert_eq!(session_id, session);
                assert!(notes.is_empty());
                assert_ne!(status_text, "Ready");
            }
            other => panic!("expected speaker notes warning event, got {other:?}"),
        }
    }

    #[test]
    fn shutdown_command_sets_shutdown_and_clears_queue() {
        let session = RenderSessionId(1);
        let (mut state, event_mailbox, cancellation) = fake_worker_parts();
        state.active_session = Some(session);
        cancellation.activate(session);
        state.queue.push(
            session,
            request(1, RenderPurpose::CurrentSlide),
            RenderPriority::Warm,
        );

        handle_fake_command(
            RenderCommand::Shutdown,
            &mut state,
            &event_mailbox,
            &cancellation,
        );

        assert!(state.shutdown);
        assert!(state.queue.is_empty());
        assert_eq!(state.active_session, Some(session));
        assert!(cancellation.is_cancelled(session));
    }
}
