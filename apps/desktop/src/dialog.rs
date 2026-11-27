//! One look for every dialog and tool window.
//!
//! The look comes from the unsaved-changes question: a raised surface with
//! an accent outline and a deep shadow, larger high-contrast text, roomy
//! controls and a filled accent button for the main action. Questions that
//! block the app ([`modal`]) also dim everything behind them.

use egui::{Color32, Response, RichText, Ui};

use crate::theme;

/// Body text size in dialogs.
pub const TEXT_SIZE: f32 = 15.0;
/// Window title size.
pub const TITLE_SIZE: f32 = 17.0;
/// Size of the main action buttons in a question.
pub const BUTTON: egui::Vec2 = egui::vec2(104.0, 34.0);

/// The dialog surface: raised fill, accent outline, deep shadow.
pub fn frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::raised())
        .stroke(egui::Stroke::new(1.5_f32, theme::accent()))
        .corner_radius(theme::RADIUS)
        .inner_margin(egui::Margin::same(16))
        .shadow(egui::Shadow {
            offset: [0, 8],
            blur: 28,
            spread: 0,
            color: Color32::from_black_alpha(if theme::light() { 90 } else { 160 }),
        })
}

/// The dimmed backdrop behind a [`modal`].
#[must_use]
pub fn backdrop() -> Color32 {
    Color32::from_black_alpha(if theme::light() { 110 } else { 170 })
}

/// Larger text, roomier controls and controls that stand out from the
/// raised surface.
pub fn readable(style: &mut egui::Style) {
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Body, FontId::proportional(TEXT_SIZE)),
        (TextStyle::Button, FontId::proportional(TEXT_SIZE)),
        (TextStyle::Small, FontId::proportional(12.5)),
        (
            TextStyle::Heading,
            FontId::new(TITLE_SIZE, FontFamily::Name("inter-semibold".into())),
        ),
        (TextStyle::Monospace, FontId::monospace(13.5)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 10.0;
    let visuals = &mut style.visuals;
    visuals.override_text_color = Some(theme::text());
    // Fields sit below the raised surface, buttons on the line colour with
    // a strong outline, like the save question's plain buttons.
    visuals.extreme_bg_color = theme::panel();
    visuals.window_fill = theme::raised();
    visuals.widgets.inactive.weak_bg_fill = theme::line();
    visuals.widgets.inactive.bg_fill = theme::line();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, theme::line_strong());
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, theme::text());
    visuals.widgets.hovered.weak_bg_fill = theme::line_strong();
    visuals.widgets.hovered.bg_fill = theme::line_strong();
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, theme::accent());
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, theme::line_strong());
}

/// Runs `add` with [`readable`] applied to `ui`.
fn styled<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    readable(ui.style_mut());
    add(ui)
}

/// A tool window or dialog with the shared look. Mirrors the builder calls
/// of [`egui::Window`] that the app uses.
#[must_use = "call show() to draw the window"]
pub struct Window<'open>(egui::Window<'open>);

impl std::fmt::Debug for Window<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("dialog::Window")
    }
}

/// Starts a [`Window`] titled `title`.
pub fn window<'open>(title: impl Into<String>) -> Window<'open> {
    Window(
        egui::Window::new(
            RichText::new(title)
                .font(theme::semibold(TITLE_SIZE))
                .color(theme::text()),
        )
        .frame(frame()),
    )
}

impl<'open> Window<'open> {
    /// See [`egui::Window::open`].
    pub fn open(self, open: &'open mut bool) -> Self {
        Self(self.0.open(open))
    }
    /// See [`egui::Window::collapsible`].
    pub fn collapsible(self, collapsible: bool) -> Self {
        Self(self.0.collapsible(collapsible))
    }
    /// See [`egui::Window::resizable`].
    pub fn resizable(self, resizable: bool) -> Self {
        Self(self.0.resizable(resizable))
    }
    /// See [`egui::Window::default_width`].
    pub fn default_width(self, width: f32) -> Self {
        Self(self.0.default_width(width))
    }
    /// See [`egui::Window::default_height`].
    pub fn default_height(self, height: f32) -> Self {
        Self(self.0.default_height(height))
    }
    /// See [`egui::Window::default_size`].
    pub fn default_size(self, size: impl Into<egui::Vec2>) -> Self {
        Self(self.0.default_size(size))
    }
    /// See [`egui::Window::anchor`].
    pub fn anchor(self, align: egui::Align2, offset: impl Into<egui::Vec2>) -> Self {
        Self(self.0.anchor(align, offset))
    }
    /// See [`egui::Window::id`].
    pub fn id(self, id: egui::Id) -> Self {
        Self(self.0.id(id))
    }
    /// Draws the window; `add` gets a ui with [`readable`] applied.
    pub fn show<R>(
        self,
        context: &egui::Context,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        self.0.show(context, |ui| styled(ui, add))
    }
}

/// A question that blocks the app: the dialog look over a dimmed backdrop.
/// Escape or a click on the backdrop asks it to close
/// ([`egui::ModalResponse::should_close`]).
pub fn modal<R>(
    context: &egui::Context,
    id: &str,
    width: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::ModalResponse<R> {
    egui::Modal::new(egui::Id::new(id))
        .backdrop_color(backdrop())
        .frame(frame().inner_margin(egui::Margin::same(24)))
        .show(context, |ui| {
            ui.set_width(width);
            styled(ui, add)
        })
}

/// A question's title.
pub fn title(ui: &mut Ui, text: impl Into<String>) {
    ui.label(
        RichText::new(text)
            .font(theme::semibold(22.0))
            .color(theme::text()),
    );
    ui.add_space(6.0);
}

/// Main text, at full contrast.
pub fn body(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text).size(16.0).color(theme::text()))
}

/// A secondary line, muted but still readable.
pub fn note(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text).size(13.0).color(theme::text_muted()))
}

/// A section heading inside a dialog.
pub fn heading(ui: &mut Ui, text: impl Into<String>) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(text)
            .font(theme::semibold(15.0))
            .color(theme::accent()),
    );
}

/// The row of answers at the bottom of a question, right-aligned. Add the
/// primary button first: it lands on the right.
pub fn buttons<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add_space(14.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
        .inner
}

/// The main action: filled with the accent.
pub fn primary(ui: &mut Ui, text: &str) -> Response {
    ui.add_sized(
        BUTTON,
        egui::Button::new(
            RichText::new(text)
                .size(16.0)
                .strong()
                .color(theme::accent_text()),
        )
        .fill(theme::accent()),
    )
}

/// Any other answer.
pub fn secondary(ui: &mut Ui, text: &str) -> Response {
    ui.add_sized(
        BUTTON,
        egui::Button::new(RichText::new(text).size(16.0).color(theme::text()))
            .fill(theme::line())
            .stroke(egui::Stroke::new(1.0_f32, theme::line_strong())),
    )
}

/// True when Enter was pressed and no text field is taking it.
#[must_use]
pub fn enter_pressed(ui: &Ui) -> bool {
    ui.input(|input| input.key_pressed(egui::Key::Enter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_text_is_larger_than_the_app_text() {
        let mut style = egui::Style::default();
        readable(&mut style);
        let body = &style.text_styles[&egui::TextStyle::Body];
        assert!(body.size > theme::FONT_SIZE);
        assert_eq!(style.visuals.override_text_color, Some(theme::text()));
    }

    #[test]
    fn the_frame_is_outlined_in_the_accent() {
        theme::set_light(false);
        let frame = frame();
        assert_eq!(frame.stroke.color, theme::accent());
        assert_eq!(frame.fill, theme::raised());
    }
}
