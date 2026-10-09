//! Settings > Background: the picture behind the whole window, how dark the
//! veil over it is, and how solid the panels are. The two levels are
//! segmented bars, since GPUI ships no slider.
use super::*;
use crate::config::background::{Edit, MAX_DIM, MIN_PANEL_OPACITY};

/// Segments in a level bar; each one is a tenth.
const STEPS: usize = 10;

impl SettingsWindow {
    pub(super) fn render_background_controls(&self, cx: &mut Context<Self>) -> Div {
        let background = &self.config.background;
        let name = background
            .image
            .as_ref()
            .map(|path| {
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                )
            })
            .unwrap_or_else(|| "None".into());
        let has_image = background.image.is_some();
        let (dim, opacity) = (background.dim, background.panel_opacity);
        let mut actions = div().flex().flex_wrap().gap(px(8.)).child(
            self.control_choice("settings-background-choose", "Choose image...", false, true)
                .on_click(cx.listener(|this, _, window, cx| this.choose_background(window, cx))),
        );
        if has_image {
            actions = actions.child(
                self.control_choice("settings-background-clear", "Remove", false, true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_native(|| Config::save_background(Edit::Image(None)), cx);
                    })),
            );
        }
        self.control_card("Window background")
            .child(self.control_row("Image", name))
            .child(actions)
            .child(self.control_note(
                "The picture fills the whole window, behind the sidebar and every pane. PNG, JPEG, WebP, GIF, and BMP work.",
            ))
            .child(self.control_row("Image darkness", format!("{}%", percent(dim))))
            .child(self.level_bar(
                "settings-background-dim",
                dim,
                (0., MAX_DIM),
                has_image,
                Edit::Dim,
                cx,
            ))
            .child(self.control_row("Panel opacity", format!("{}%", percent(opacity))))
            .child(self.level_bar(
                "settings-background-opacity",
                opacity,
                (MIN_PANEL_OPACITY, 1.),
                has_image,
                Edit::PanelOpacity,
                cx,
            ))
            .child(self.control_note(
                "Lower panel opacity shows more of the picture through the sidebar and terminal; raise darkness if text is hard to read.",
            ))
    }

    /// A row of segments, one per tenth within `limits`; clicking one sets it.
    fn level_bar(
        &self,
        id: &'static str,
        current: f32,
        limits: (f32, f32),
        enabled: bool,
        edit: fn(f32) -> Edit,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut bar = div()
            .flex()
            .gap(px(4.))
            .when(!enabled, |bar| bar.opacity(0.5));
        let tenths = |value: f32| (value * STEPS as f32).round() as usize;
        for step in tenths(limits.0)..=tenths(limits.1) {
            let value = (step as f32 / STEPS as f32).clamp(limits.0, limits.1);
            let filled = current + 0.001 >= value;
            bar = bar.child(
                div()
                    .id((id, step))
                    .debug_selector(move || format!("{id}-{step}"))
                    .flex_1()
                    .h(px(22.))
                    .rounded(px(corners::SMALL))
                    .border_1()
                    .border_color(rgb(self.theme.active))
                    .bg(if filled {
                        crate::menu::accent(&self.theme)
                    } else {
                        rgb(self.theme.background)
                    })
                    .when(enabled, |segment| {
                        segment
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.save_native(move || Config::save_background(edit(value)), cx);
                            }))
                    }),
            );
        }
        bar
    }

    fn choose_background(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Use as background".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let outcome = picker.await;
            let _ = this.update_in(cx, |this, _, cx| match outcome {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.save_native(
                            move || Config::save_background(Edit::Image(Some(path))),
                            cx,
                        );
                    }
                }
                Ok(Ok(None)) | Err(_) => {}
                Ok(Err(error)) => {
                    this.error = Some(format!("Unable to open file dialog: {error}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

fn percent(value: f32) -> u32 {
    (value * 100.).round() as u32
}
