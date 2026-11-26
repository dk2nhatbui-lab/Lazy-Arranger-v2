//! Right-click menus opened from app state instead of from a widget.
//!
//! The timeline and the notes views report a right-click; the app then
//! shows the menu here. It runs on egui's own menu root, so submenus open
//! on hover like any other egui menu, and it closes only on a press outside
//! the menu (and its submenus) or on Escape. The release of the right-click
//! that opened it never closes it, which made the menu blink before.

use egui::menu::MenuRoot;
use egui::{Context, Id, Pos2, Rect, Sense, Ui, Vec2};

/// The menu's root, kept between frames with the place it opened at.
#[derive(Clone)]
struct Open {
    pos: Pos2,
    root: MenuRoot,
}

/// Shows the menu `id` at `pos`. Returns what `add_contents` returned and
/// whether the menu stays open; the caller forgets the menu when it is
/// false.
pub(super) fn show<R>(
    context: &Context,
    id: Id,
    pos: Pos2,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (Option<R>, bool) {
    let state_id = id.with("popup-menu-root");
    let kept = context
        .data(|data| data.get_temp::<Open>(state_id))
        .filter(|open| open.pos == pos);
    let fresh = kept.is_none();
    let open = kept.unwrap_or_else(|| Open {
        pos,
        root: MenuRoot::new(pos, id),
    });

    // `MenuRoot::show` draws for a response with the root's id.
    let anchor = egui::Area::new(id.with("popup-menu-anchor"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .interactable(false)
        .show(context, |ui| {
            ui.interact(Rect::from_min_size(pos, Vec2::ZERO), id, Sense::hover())
        })
        .inner;
    // `MenuRoot::show` hands back the contents' result only on the frame it
    // closes, so take it from the closure.
    let mut result = None;
    let (response, _) = crate::theme::with_menu_style(context, || {
        open.root.show(&anchor, |ui| {
            ui.set_min_width(200.0);
            result = Some(add_contents(ui));
        })
    });

    // A press outside the menu closes it, but not on the frame it opened:
    // that frame still carries the right-click that opened it.
    let pressed_outside = !fresh
        && context.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|at| !open.root.menu_state.read().area_contains(at))
        });
    let stays = !response.is_close() && !pressed_outside;
    if stays {
        context.data_mut(|data| data.insert_temp(state_id, open));
    } else {
        context.data_mut(|data| data.remove::<Open>(state_id));
    }
    (result, stays)
}

#[cfg(test)]
mod tests {
    use egui::{Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

    /// Runs one frame of the menu at a fractional point, as on a scaled
    /// display; returns whether it stays open.
    fn frame(context: &egui::Context, events: Vec<Event>) -> bool {
        let mut stays = false;
        let _ = context.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                events,
                ..RawInput::default()
            },
            |context| {
                stays = super::show(context, egui::Id::new("menu"), pos2(100.3, 100.7), |ui| {
                    let _ = ui.button("Item");
                })
                .1;
            },
        );
        stays
    }

    fn button(at: Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn the_right_click_that_opens_the_menu_does_not_close_it() {
        let context = egui::Context::default();
        let at = pos2(100.3, 100.7);
        // The release of the right-click arrives on the frame it opens.
        assert!(frame(
            &context,
            vec![
                Event::PointerMoved(at),
                button(at, PointerButton::Secondary, true),
                button(at, PointerButton::Secondary, false),
            ],
        ));
        assert!(frame(&context, Vec::new()));
        assert!(frame(&context, Vec::new()));
        // A press outside closes it.
        let away = pos2(600.0, 500.0);
        assert!(!frame(
            &context,
            vec![
                Event::PointerMoved(away),
                button(away, PointerButton::Primary, true)
            ],
        ));
    }

    #[test]
    fn escape_closes_the_menu() {
        let context = egui::Context::default();
        assert!(frame(&context, Vec::new()));
        assert!(!frame(
            &context,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ));
    }
}
