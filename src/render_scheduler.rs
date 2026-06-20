use std::{collections::HashSet, path::PathBuf};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
};

use crate::{
    errors::speaker_notes_warning,
    notes::SpeakerNotes,
    pdf::PdfDocumentState,
    rendering::{estimated_render_bytes, RenderRequest, RenderedPagePixels},
};

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
    Close {
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
}

#[derive(Debug, Default)]
pub struct RenderSessionTracker {
    next_session: u64,
    current_session: Option<RenderSessionId>,
}

impl RenderSessionTracker {
    pub fn begin_session(&mut self) -> RenderSessionId {
        self.next_session = self.next_session.wrapping_add(1);
        let session_id = RenderSessionId(self.next_session);
        self.current_session = Some(session_id);
        session_id
    }

    pub fn current_session(&self) -> Option<RenderSessionId> {
        self.current_session
    }

    pub fn accepts(&self, session_id: RenderSessionId) -> bool {
        self.current_session == Some(session_id)
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
            priority,
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
            priority: RenderPriority::Background,
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

    pub fn clear_session(&mut self, session_id: RenderSessionId) {
        self.items
            .retain(|item| item.key.session_id() != session_id);
        self.queued_keys
            .retain(|key| key.session_id() != session_id);
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
            .max_by_key(|(_, item)| (item.priority, u64::MAX.saturating_sub(item.sequence)))
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

#[derive(Debug, Default)]
struct RenderCancellation {
    active_session: AtomicU64,
    shutdown: AtomicBool,
}

impl RenderCancellation {
    fn activate(&self, session_id: RenderSessionId) {
        self.active_session.store(session_id.0, Ordering::Release);
    }

    fn close(&self, session_id: RenderSessionId) {
        let _ = self.active_session.compare_exchange(
            session_id.0,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.active_session.store(0, Ordering::Release);
    }

    fn is_cancelled(&self, session_id: RenderSessionId) -> bool {
        self.shutdown.load(Ordering::Acquire)
            || self.active_session.load(Ordering::Acquire) != session_id.0
    }
}

pub struct RenderScheduler {
    command_sender: Sender<RenderCommand>,
    event_receiver: Receiver<RenderEvent>,
    cancellation: Arc<RenderCancellation>,
}

impl RenderScheduler {
    pub fn start() -> Self {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let cancellation = Arc::new(RenderCancellation::default());
        let worker_cancellation = Arc::clone(&cancellation);
        thread::spawn(move || render_worker(command_receiver, event_sender, worker_cancellation));

        Self {
            command_sender,
            event_receiver,
            cancellation,
        }
    }

    pub fn open(&self, session_id: RenderSessionId, path: PathBuf) {
        self.cancellation.activate(session_id);
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
        let mut events = Vec::new();
        while let Ok(event) = self.event_receiver.try_recv() {
            events.push(event);
        }
        events
    }

    fn send(&self, command: RenderCommand) {
        let _ = self.command_sender.send(command);
    }
}

impl Drop for RenderScheduler {
    fn drop(&mut self) {
        self.cancellation.shutdown();
        let _ = self.command_sender.send(RenderCommand::Shutdown);
    }
}

fn render_worker(
    command_receiver: Receiver<RenderCommand>,
    event_sender: Sender<RenderEvent>,
    cancellation: Arc<RenderCancellation>,
) {
    let mut document: Option<PdfDocumentState> = None;
    let mut active_session: Option<RenderSessionId> = None;
    let mut queue = RenderQueue::default();
    let mut shutdown = false;

    while !shutdown {
        match command_receiver.recv() {
            Ok(command) => {
                handle_command(
                    command,
                    &mut document,
                    &mut active_session,
                    &mut queue,
                    &event_sender,
                    &cancellation,
                    &mut shutdown,
                );
            }
            Err(_) => break,
        }

        drain_pending_commands(
            &command_receiver,
            &mut document,
            &mut active_session,
            &mut queue,
            &event_sender,
            &cancellation,
            &mut shutdown,
        );

        while !shutdown {
            let Some((job_id, session_id, work)) = queue.pop() else {
                break;
            };

            if active_session != Some(session_id) || cancellation.is_cancelled(session_id) {
                continue;
            }

            match work {
                RenderWork::Page(request) => {
                    render_page_on_worker(
                        document.as_ref(),
                        session_id,
                        job_id,
                        request,
                        &event_sender,
                        &cancellation,
                    );
                }
                RenderWork::SpeakerNotes => {
                    extract_speaker_notes_on_worker(
                        document.as_ref(),
                        session_id,
                        &event_sender,
                        &cancellation,
                    );
                }
            }

            drain_pending_commands(
                &command_receiver,
                &mut document,
                &mut active_session,
                &mut queue,
                &event_sender,
                &cancellation,
                &mut shutdown,
            );
        }
    }
}

fn drain_pending_commands(
    command_receiver: &Receiver<RenderCommand>,
    document: &mut Option<PdfDocumentState>,
    active_session: &mut Option<RenderSessionId>,
    queue: &mut RenderQueue,
    event_sender: &Sender<RenderEvent>,
    cancellation: &Arc<RenderCancellation>,
    shutdown: &mut bool,
) {
    while let Ok(command) = command_receiver.try_recv() {
        handle_command(
            command,
            document,
            active_session,
            queue,
            event_sender,
            cancellation,
            shutdown,
        );
        if *shutdown {
            break;
        }
    }
}

fn handle_command(
    command: RenderCommand,
    document: &mut Option<PdfDocumentState>,
    active_session: &mut Option<RenderSessionId>,
    queue: &mut RenderQueue,
    event_sender: &Sender<RenderEvent>,
    cancellation: &Arc<RenderCancellation>,
    shutdown: &mut bool,
) {
    match command {
        RenderCommand::Open { session_id, path } => {
            cancellation.activate(session_id);
            queue.clear();
            *active_session = Some(session_id);
            *document = None;
            open_document_on_worker(session_id, path, document, event_sender);
        }
        RenderCommand::RenderPage {
            session_id,
            request,
            priority,
        } => {
            queue.push(session_id, request, priority);
        }
        RenderCommand::ExtractSpeakerNotes { session_id } => {
            queue.push_speaker_notes(session_id);
        }
        RenderCommand::Close { session_id } => {
            cancellation.close(session_id);
            queue.clear_session(session_id);
            if *active_session == Some(session_id) {
                *document = None;
                *active_session = None;
            }
        }
        RenderCommand::Shutdown => {
            cancellation.shutdown();
            *shutdown = true;
            queue.clear();
        }
    }
}

fn open_document_on_worker(
    session_id: RenderSessionId,
    path: PathBuf,
    document: &mut Option<PdfDocumentState>,
    event_sender: &Sender<RenderEvent>,
) {
    match PdfDocumentState::open(path) {
        Ok(doc) => {
            let title = doc.title();
            let page_count = doc.page_count();
            *document = Some(doc);
            let _ = event_sender.send(RenderEvent::Opened {
                session_id,
                title,
                page_count,
                status_text: "Ready".to_owned(),
            });
        }
        Err(err) => {
            let _ = event_sender.send(RenderEvent::OpenFailed {
                session_id,
                message: err.to_string(),
            });
        }
    }
}

fn render_page_on_worker(
    document: Option<&PdfDocumentState>,
    session_id: RenderSessionId,
    job_id: RenderJobId,
    request: RenderRequest,
    event_sender: &Sender<RenderEvent>,
    cancellation: &RenderCancellation,
) {
    let event = match document {
        Some(document) => match render_page_pixels(document, request) {
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
        let _ = event_sender.send(event);
    }
}

fn extract_speaker_notes_on_worker(
    document: Option<&PdfDocumentState>,
    session_id: RenderSessionId,
    event_sender: &Sender<RenderEvent>,
    cancellation: &RenderCancellation,
) {
    let Some(document) = document else {
        return;
    };

    let notes = match document.speaker_notes_cancellable(|| cancellation.is_cancelled(session_id)) {
        Ok(Some(notes)) => notes,
        Ok(None) => return,
        Err(err) => {
            if !cancellation.is_cancelled(session_id) {
                let _ = event_sender.send(RenderEvent::SpeakerNotesLoaded {
                    session_id,
                    notes: SpeakerNotes::empty(),
                    status_text: speaker_notes_warning(&err).text().to_owned(),
                });
            }
            return;
        }
    };

    if !cancellation.is_cancelled(session_id) {
        let _ = event_sender.send(RenderEvent::SpeakerNotesLoaded {
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
        pixels,
        aspect_ratio,
        estimated_bytes: estimated_render_bytes(request.width, aspect_ratio),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::RenderPurpose;

    fn request(page_index: u32, purpose: RenderPurpose) -> RenderRequest {
        RenderRequest {
            page_index,
            width: 100,
            purpose,
        }
    }

    #[test]
    fn session_tracker_accepts_only_current_session() {
        let mut tracker = RenderSessionTracker::default();
        let first = tracker.begin_session();
        let second = tracker.begin_session();

        assert!(!tracker.accepts(first));
        assert!(tracker.accepts(second));
        assert_eq!(tracker.current_session(), Some(second));
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
    fn clear_session_drops_only_matching_jobs() {
        let mut queue = RenderQueue::default();
        let first_session = RenderSessionId(1);
        let second_session = RenderSessionId(2);
        let first = request(1, RenderPurpose::Thumbnail);
        let second = request(2, RenderPurpose::Thumbnail);

        queue.push(first_session, first, RenderPriority::Background);
        queue.push(second_session, second, RenderPriority::Background);
        queue.clear_session(first_session);

        let (_, session_id, work) = queue.pop().unwrap();
        assert_eq!(session_id, second_session);
        assert_eq!(work, RenderWork::Page(second));
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
    fn close_command_clears_active_session_and_matching_jobs() {
        let session = RenderSessionId(1);
        let mut document = None;
        let mut active_session = Some(session);
        let mut queue = RenderQueue::default();
        let (event_sender, _event_receiver) = mpsc::channel();
        let cancellation = Arc::new(RenderCancellation::default());
        let mut shutdown = false;
        cancellation.activate(session);
        queue.push(
            session,
            request(1, RenderPurpose::CurrentSlide),
            RenderPriority::Warm,
        );

        handle_command(
            RenderCommand::Close {
                session_id: session,
            },
            &mut document,
            &mut active_session,
            &mut queue,
            &event_sender,
            &cancellation,
            &mut shutdown,
        );

        assert_eq!(active_session, None);
        assert!(document.is_none());
        assert!(queue.is_empty());
        assert!(cancellation.is_cancelled(session));
        assert!(!shutdown);
    }

    #[test]
    fn shutdown_command_sets_shutdown_and_clears_queue() {
        let session = RenderSessionId(1);
        let mut document = None;
        let mut active_session = Some(session);
        let mut queue = RenderQueue::default();
        let (event_sender, _event_receiver) = mpsc::channel();
        let cancellation = Arc::new(RenderCancellation::default());
        let mut shutdown = false;
        cancellation.activate(session);
        queue.push(
            session,
            request(1, RenderPurpose::CurrentSlide),
            RenderPriority::Warm,
        );

        handle_command(
            RenderCommand::Shutdown,
            &mut document,
            &mut active_session,
            &mut queue,
            &event_sender,
            &cancellation,
            &mut shutdown,
        );

        assert!(shutdown);
        assert!(queue.is_empty());
        assert_eq!(active_session, Some(session));
        assert!(cancellation.is_cancelled(session));
    }
}
