//! Text clipboard boundary, retaining ownership for X11/Wayland paste requests.
use anyhow::{Context, Result};

#[derive(Default)]
pub struct TextClipboard {
    context: Option<arboard::Clipboard>,
}

impl TextClipboard {
    pub fn copy(&mut self, text: &str) -> Result<()> {
        if self.context.is_none() {
            self.context = Some(arboard::Clipboard::new().context("cannot access clipboard")?);
        }
        self.context
            .as_mut()
            .expect("clipboard initialized above")
            .set_text(text)
            .context("cannot write clipboard")
    }

    pub fn release(&mut self) {
        self.context.take();
    }
}
