//! Finder and Dock document requests. Native callbacks only enqueue; Rust owns opening.
use crate::{app_state::AppState, window_controller::AppWindowRefs};
use slint::{winit_030::WinitWindowAccessor, ComponentHandle, Timer, TimerMode};
use std::{
    cell::RefCell,
    ffi::{c_char, c_void, CStr, CString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

unsafe extern "C" {
    fn qp_documents_install() -> bool;
    fn qp_documents_set_modal_busy(value: bool);
    fn qp_documents_enqueue_path(path: *const c_char);
    fn qp_documents_take(path: *mut c_char, capacity: usize) -> *mut c_void;
    fn qp_documents_release(handle: *mut c_void);
    fn qp_documents_take_rejected() -> bool;
}

pub fn install() -> anyhow::Result<()> {
    anyhow::ensure!(
        unsafe { qp_documents_install() },
        "could not install macOS document open handler"
    );
    Ok(())
}

pub fn set_modal_busy(value: bool) {
    unsafe { qp_documents_set_modal_busy(value) };
}

pub fn enqueue_path(path: &Path) {
    if let Ok(path) = CString::new(path.as_os_str().as_bytes()) {
        unsafe { qp_documents_enqueue_path(path.as_ptr()) };
    }
}

struct Document {
    handle: *mut c_void,
    path: PathBuf,
}
impl Drop for Document {
    fn drop(&mut self) {
        unsafe { qp_documents_release(self.handle) };
    }
}
impl Document {
    fn take() -> Option<Self> {
        let mut path = [0 as c_char; 4096];
        let handle = unsafe { qp_documents_take(path.as_mut_ptr(), path.len()) };
        if handle.is_null() {
            return None;
        }
        let bytes = unsafe { CStr::from_ptr(path.as_ptr()) }.to_bytes().to_vec();
        Some(Self {
            handle,
            path: std::ffi::OsString::from_vec(bytes).into(),
        })
    }
}

fn busy(state: &AppState) -> bool {
    state.file_dialog.open
        || state
            .pending_open
            .as_ref()
            .is_some_and(|pending| state.automatic_reopen != Some(pending.session_id))
}

pub fn start(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) -> Timer {
    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.window().on_winit_window_event(|_, event| {
            if let slint::winit_030::winit::event::WindowEvent::DroppedFile(path) = event {
                enqueue_path(path);
            }
            slint::winit_030::EventResult::Propagate
        });
    }
    let timer = Timer::default();
    let mut documents: Vec<Document> = Vec::new();
    timer.start(TimerMode::Repeated, Duration::from_millis(25), move || {
        // Retain at most the active and candidate NSURL, including their sandbox grants.
        documents.retain(|document| {
            let state = state.borrow();
            state.active_document_path.as_ref() == Some(&document.path)
                || state
                    .pending_open
                    .as_ref()
                    .is_some_and(|pending| pending.path == document.path)
        });
        let rejected = unsafe { qp_documents_take_rejected() };
        if let Some(document) = Document::take() {
            if busy(&state.borrow()) {
                message(
                    &windows,
                    "A PDF open or file dialog is already in progress. Try again when it finishes.",
                );
                return;
            }
            if let Err(error) = crate::macos_file_access::remember_selected(&document.path) {
                tracing::warn!(error = %error, "could not remember Finder PDF access");
            }
            state.borrow_mut().automatic_reopen = None;
            crate::begin_open_pdf(&windows, &state, document.path.clone());
            documents.retain(|previous| previous.path != document.path);
            documents.push(document);
            crate::window_controller::show_presenter_window(&windows);
        }
        if rejected {
            message(
                &windows,
                "Open one local PDF at a time. Additional requests were not opened.",
            );
        }
    });
    timer
}

fn message(windows: &AppWindowRefs, text: &str) {
    crate::view_sync::set_presenter_message(
        &windows.presenter,
        crate::errors::PresenterMessage::new(text, crate::errors::MessageSeverity::Warning),
    );
    crate::window_controller::show_presenter_window(windows);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_request_supersedes_only_automatic_reopen() {
        let mut state = AppState::default();
        assert!(!busy(&state));
        let session =
            crate::session_controller::begin_open_pdf_state(&mut state, "/old.pdf".into());
        assert!(busy(&state));
        state.automatic_reopen = Some(session);
        assert!(!busy(&state));
        state.file_dialog.open = true;
        assert!(busy(&state));
    }
}
