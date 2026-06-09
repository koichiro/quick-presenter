use slint::{Rgba8Pixel, SharedPixelBuffer};

pub fn notes() {
    // Corrected snippets for pdfium-render 0.9.1 + Slint 1.16
    // let page = self.document.pages().get(self.page_index as i32)?;
    //
    // let pixels = bitmap.as_rgba8().expect("RGBA8 image").as_raw();
    // let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
    //     pixels,
    //     width,
    //     height,
    // );
}