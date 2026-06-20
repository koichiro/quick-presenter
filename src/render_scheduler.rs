use std::{collections::HashSet, path::PathBuf};

use crate::rendering::RenderRequest;

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
        job_id: RenderJobId,
        request: RenderRequest,
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
    OpenFailed {
        session_id: RenderSessionId,
        message: String,
    },
    PageRendered {
        session_id: RenderSessionId,
        job_id: RenderJobId,
        request: RenderRequest,
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
struct QueueKey {
    session_id: RenderSessionId,
    request: RenderRequest,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct QueuedRender {
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
    items: Vec<QueuedRender>,
}

impl RenderQueue {
    pub fn push(
        &mut self,
        session_id: RenderSessionId,
        request: RenderRequest,
        priority: RenderPriority,
    ) -> Option<RenderJobId> {
        let key = QueueKey {
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
        self.items.push(QueuedRender {
            key,
            job_id,
            priority,
            sequence: self.next_sequence,
        });

        Some(job_id)
    }

    pub fn pop(&mut self) -> Option<(RenderJobId, RenderSessionId, RenderRequest)> {
        let index = self.next_item_index()?;
        let item = self.items.swap_remove(index);
        self.queued_keys.remove(&item.key);
        Some((item.job_id, item.key.session_id, item.key.request))
    }

    pub fn clear_session(&mut self, session_id: RenderSessionId) {
        self.items.retain(|item| item.key.session_id != session_id);
        self.queued_keys.retain(|key| key.session_id != session_id);
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

        assert_eq!(queue.pop().unwrap().2, current);
        assert_eq!(queue.pop().unwrap().2, preview);
        assert_eq!(queue.pop().unwrap().2, thumbnail);
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

        assert_eq!(queue.pop().unwrap().2, first);
        assert_eq!(queue.pop().unwrap().2, second);
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

        assert_eq!(queue.pop().unwrap().2, current);
        assert_eq!(queue.pop().unwrap().2, other);
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

        let (_, session_id, request) = queue.pop().unwrap();
        assert_eq!(session_id, second_session);
        assert_eq!(request, second);
        assert!(queue.is_empty());
    }
}
