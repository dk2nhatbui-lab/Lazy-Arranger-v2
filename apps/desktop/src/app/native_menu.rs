//! The native macOS menu bar, built from the shared menu table.
//!
//! macOS only: on Windows, muda needs an unsafe call with the window handle,
//! which this workspace forbids, so Windows and Linux draw the menu bar in
//! the window instead (as the C++ app did).

#[cfg(target_os = "macos")]
pub(super) use mac::NativeMenu;

/// True where the native menu bar is built.
pub(super) const SUPPORTED: bool = cfg!(target_os = "macos");

#[cfg(not(target_os = "macos"))]
pub(super) use other::NativeMenu;

#[cfg(not(target_os = "macos"))]
mod other {
    use super::super::menu::{Menu, MenuCommand};

    /// No native menu on this platform.
    #[derive(Debug)]
    pub(in super::super) struct NativeMenu;

    impl NativeMenu {
        pub(in super::super) fn build(_context: &egui::Context, _model: &[Menu]) -> Option<Self> {
            None
        }

        pub(in super::super) fn update(&mut self, _model: &[Menu]) {}

        pub(in super::super) fn commands(&mut self) -> Vec<MenuCommand> {
            Vec::new()
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::mpsc::{Receiver, channel};

    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{
        CheckMenuItem, IsMenuItem, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
    };

    use super::super::menu::{Menu, MenuCommand, MenuEntry, Panel};
    use crate::settings::Binding;

    /// One item we made, so its text, ticks and enabled state can change.
    enum Handle {
        Plain(MenuItem),
        Check(CheckMenuItem),
    }

    /// What decides whether the menu can be changed in place or must be
    /// built again: the items, their order, shortcuts and ticks.
    #[derive(PartialEq)]
    struct Shape(Vec<(MenuCommand, Option<Binding>, bool)>);

    /// The live native menu bar.
    pub(in super::super) struct NativeMenu {
        menu: muda::Menu,
        /// Index in this list = the item's menu id.
        commands: Vec<MenuCommand>,
        handles: Vec<Handle>,
        shape: Shape,
        events: Receiver<MenuId>,
    }

    impl std::fmt::Debug for NativeMenu {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("NativeMenu")
                .field("items", &self.commands.len())
                .finish()
        }
    }

    impl NativeMenu {
        /// Builds the menu and puts it in the macOS menu bar.
        pub(in super::super) fn build(context: &egui::Context, model: &[Menu]) -> Option<Self> {
            let (sender, events) = channel();
            let repaint = context.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let _ = sender.send(event.id);
                repaint.request_repaint();
            }));
            let mut native = Self {
                menu: muda::Menu::new(),
                commands: Vec::new(),
                handles: Vec::new(),
                shape: shape(model),
                events,
            };
            native.fill(model).ok()?;
            native.menu.init_for_nsapp();
            Some(native)
        }

        /// Brings the menu in line with the model.
        pub(in super::super) fn update(&mut self, model: &[Menu]) {
            let next = shape(model);
            if next != self.shape {
                self.shape = next;
                self.menu = muda::Menu::new();
                self.commands.clear();
                self.handles.clear();
                if self.fill(model).is_ok() {
                    self.menu.init_for_nsapp();
                }
                return;
            }
            let mut index = 0;
            for menu in model {
                refresh(&menu.entries, &self.handles, &mut index);
            }
        }

        /// The commands chosen since the last call.
        pub(in super::super) fn commands(&mut self) -> Vec<MenuCommand> {
            self.events
                .try_iter()
                .filter_map(|id| id.0.parse::<usize>().ok())
                .filter_map(|index| self.commands.get(index).copied())
                .collect()
        }

        fn fill(&mut self, model: &[Menu]) -> muda::Result<()> {
            // The first submenu is the app menu, titled with the app's name.
            let app = Submenu::new("Lazy Arranger Midi DAW", true);
            let about = self.item(&MenuEntry::app(
                MenuCommand::Show(Panel::About),
                "About Lazy Arranger Midi DAW",
                None,
            ));
            let settings = self.item(&MenuEntry::app(
                MenuCommand::Show(Panel::Settings),
                "Settings…",
                super::super::menu::SETTINGS_KEYS,
            ));
            let quit = self.item(&MenuEntry::app(
                MenuCommand::Quit,
                "Quit Lazy Arranger Midi DAW",
                super::super::menu::QUIT_KEYS,
            ));
            app.append_items(&[
                about.as_dyn(),
                &PredefinedMenuItem::separator(),
                settings.as_dyn(),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                quit.as_dyn(),
            ])?;
            self.menu.append(&app)?;
            self.handles.extend([about, settings, quit]);

            for menu in model {
                let submenu = Submenu::new(menu.title, true);
                if menu.title == "Window" {
                    submenu.append_items(&[
                        &PredefinedMenuItem::minimize(None),
                        &PredefinedMenuItem::fullscreen(None),
                        &PredefinedMenuItem::separator(),
                    ])?;
                }
                self.append_entries(&submenu, &menu.entries)?;
                if menu.title == "Window" {
                    submenu.set_as_windows_menu_for_nsapp();
                }
                if menu.title == "Help" {
                    submenu.set_as_help_menu_for_nsapp();
                }
                self.menu.append(&submenu)?;
            }
            Ok(())
        }

        fn append_entries(&mut self, parent: &Submenu, entries: &[MenuEntry]) -> muda::Result<()> {
            for entry in entries {
                match entry {
                    MenuEntry::Separator => parent.append(&PredefinedMenuItem::separator())?,
                    MenuEntry::Submenu { label, entries } => {
                        let submenu = Submenu::new(label, true);
                        self.append_entries(&submenu, entries)?;
                        parent.append(&submenu)?;
                    }
                    MenuEntry::Item { .. } => {
                        let handle = self.item(entry);
                        parent.append(handle.as_dyn())?;
                        self.handles.push(handle);
                    }
                }
            }
            Ok(())
        }

        /// Makes one item and gives it the next id.
        fn item(&mut self, entry: &MenuEntry) -> Handle {
            let MenuEntry::Item {
                command,
                label,
                shortcut,
                enabled,
                checked,
            } = entry
            else {
                return Handle::Plain(MenuItem::new("", false, None));
            };
            let id = self.commands.len().to_string();
            self.commands.push(*command);
            let accelerator = shortcut.and_then(accelerator);
            match checked {
                Some(checked) => Handle::Check(CheckMenuItem::with_id(
                    id,
                    label,
                    *enabled,
                    *checked,
                    accelerator,
                )),
                None => Handle::Plain(MenuItem::with_id(id, label, *enabled, accelerator)),
            }
        }
    }

    impl Handle {
        fn as_dyn(&self) -> &dyn IsMenuItem {
            match self {
                Self::Plain(item) => item,
                Self::Check(item) => item,
            }
        }
    }

    impl MenuEntry {
        fn app(command: MenuCommand, label: &str, shortcut: Option<Binding>) -> Self {
            Self::Item {
                command,
                label: label.to_string(),
                shortcut,
                enabled: true,
                checked: None,
            }
        }
    }

    /// The shape of the whole bar, app menu included.
    fn shape(model: &[Menu]) -> Shape {
        fn walk(entries: &[MenuEntry], out: &mut Vec<(MenuCommand, Option<Binding>, bool)>) {
            for entry in entries {
                match entry {
                    MenuEntry::Item {
                        command,
                        shortcut,
                        checked,
                        ..
                    } => out.push((*command, *shortcut, checked.is_some())),
                    MenuEntry::Submenu { entries, .. } => walk(entries, out),
                    MenuEntry::Separator => {}
                }
            }
        }
        let mut out = Vec::new();
        for menu in model {
            walk(&menu.entries, &mut out);
        }
        Shape(out)
    }

    /// Updates text, enabled state and ticks in place. The three app-menu
    /// items come first in `handles` and never change.
    fn refresh(entries: &[MenuEntry], handles: &[Handle], index: &mut usize) {
        for entry in entries {
            match entry {
                MenuEntry::Item {
                    label,
                    enabled,
                    checked,
                    ..
                } => {
                    if let Some(handle) = handles.get(*index + 3) {
                        match (handle, checked) {
                            (Handle::Plain(item), _) => {
                                item.set_text(label);
                                item.set_enabled(*enabled);
                            }
                            (Handle::Check(item), checked) => {
                                item.set_text(label);
                                item.set_enabled(*enabled);
                                item.set_checked(checked.unwrap_or(false));
                            }
                        }
                    }
                    *index += 1;
                }
                MenuEntry::Submenu { entries, .. } => refresh(entries, handles, index),
                MenuEntry::Separator => {}
            }
        }
    }

    /// The native accelerator for a binding. Keys without Cmd, Ctrl or
    /// Alt get none (except function keys), so the menu never takes plain
    /// typing away from text fields.
    fn accelerator(binding: Binding) -> Option<Accelerator> {
        let held = binding.modifiers;
        let function_key = matches!(
            binding.key,
            egui::Key::F1
                | egui::Key::F2
                | egui::Key::F3
                | egui::Key::F4
                | egui::Key::F5
                | egui::Key::F6
                | egui::Key::F7
                | egui::Key::F8
                | egui::Key::F9
                | egui::Key::F10
                | egui::Key::F11
                | egui::Key::F12
        );
        if !(held.command || held.mac_cmd || held.ctrl || held.alt || function_key) {
            return None;
        }
        let mut modifiers = Modifiers::empty();
        if held.command || held.mac_cmd {
            modifiers |= Modifiers::SUPER;
        }
        if held.ctrl {
            modifiers |= Modifiers::CONTROL;
        }
        if held.shift {
            modifiers |= Modifiers::SHIFT;
        }
        if held.alt {
            modifiers |= Modifiers::ALT;
        }
        Some(Accelerator::new(Some(modifiers), code(binding.key)?))
    }

    fn code(key: egui::Key) -> Option<Code> {
        use egui::Key as K;
        Some(match key {
            K::A => Code::KeyA,
            K::B => Code::KeyB,
            K::C => Code::KeyC,
            K::D => Code::KeyD,
            K::E => Code::KeyE,
            K::F => Code::KeyF,
            K::G => Code::KeyG,
            K::H => Code::KeyH,
            K::I => Code::KeyI,
            K::J => Code::KeyJ,
            K::K => Code::KeyK,
            K::L => Code::KeyL,
            K::M => Code::KeyM,
            K::N => Code::KeyN,
            K::O => Code::KeyO,
            K::P => Code::KeyP,
            K::Q => Code::KeyQ,
            K::R => Code::KeyR,
            K::S => Code::KeyS,
            K::T => Code::KeyT,
            K::U => Code::KeyU,
            K::V => Code::KeyV,
            K::W => Code::KeyW,
            K::X => Code::KeyX,
            K::Y => Code::KeyY,
            K::Z => Code::KeyZ,
            K::Num0 => Code::Digit0,
            K::Num1 => Code::Digit1,
            K::Num2 => Code::Digit2,
            K::Num3 => Code::Digit3,
            K::Num4 => Code::Digit4,
            K::Num5 => Code::Digit5,
            K::Num6 => Code::Digit6,
            K::Num7 => Code::Digit7,
            K::Num8 => Code::Digit8,
            K::Num9 => Code::Digit9,
            K::Comma => Code::Comma,
            K::Period => Code::Period,
            K::Minus => Code::Minus,
            K::Space => Code::Space,
            K::Enter => Code::Enter,
            K::Delete => Code::Delete,
            K::Backspace => Code::Backspace,
            K::F1 => Code::F1,
            K::F2 => Code::F2,
            K::F3 => Code::F3,
            K::F4 => Code::F4,
            K::F5 => Code::F5,
            K::F6 => Code::F6,
            K::F7 => Code::F7,
            K::F8 => Code::F8,
            K::F9 => Code::F9,
            K::F10 => Code::F10,
            K::F11 => Code::F11,
            K::F12 => Code::F12,
            _ => return None,
        })
    }
}
