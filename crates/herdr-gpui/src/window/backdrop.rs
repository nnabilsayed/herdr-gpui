//! The picture behind the whole window, drawn under every panel with a dark
//! veil between so terminal text stays readable.

use super::HerdrWindow;
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// The picture, covering the window, then the veil. Both are absolute so
    /// the panels laid out after them paint translucent over the picture.
    pub(super) fn render_backdrop(&self) -> Option<AnyElement> {
        let background = &self.config.background;
        // Only a picture that loaded, so a bad file leaves a plain window.
        let image = background.image.clone().filter(|_| self.theme.backdrop)?;
        Some(
            div()
                .absolute()
                .size_full()
                .overflow_hidden()
                .child(
                    img(image)
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        // The file can still vanish after the check.
                        .with_fallback(|| div().into_any_element()),
                )
                .child(
                    div()
                        .absolute()
                        .size_full()
                        .bg(hsla(0., 0., 0., background.dim)),
                )
                .into_any_element(),
        )
    }
}
