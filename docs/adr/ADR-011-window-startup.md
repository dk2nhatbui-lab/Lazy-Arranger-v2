# ADR-011: Window startup, rendering backend and the first-frame flicker

**Status:** Accepted. Applies to the Stage 6 desktop shell.

## Context

The product owner has seen egui applications blink or show a blank/white first
screen on launch. This is a known class of startup bug in the surrounding stack,
not in egui's immediate-mode drawing itself. Upstream evidence:

* **Window shown before the first frame is painted.** eframe fixed this by
  starting the window hidden and showing it after the first painted frame
  ([egui PR #2279](https://github.com/emilk/egui/pull/2279),
  [commit 4dc964a](https://github.com/emilk/egui/commit/4dc964ab4bf0d43d9748e9e8afda2e68236d150e),
  [solidcraft PR #3444](https://github.com/tangency-labs/solidcraft/pull/3444)).
* **Backend surface not ready or not cleared consistently.** With the glow
  backend the framebuffer used to be cleared after `App::update` except for
  single-viewport cases
  ([PR #3665](https://github.com/emilk/egui/pull/3665)); raw winit+glow examples
  flash white unless the window is made visible after the GL context exists
  ([issue #2333](https://github.com/emilk/egui/issues/2333)).
* **wgpu surface configuration failure.** On eframe 0.36, wgpu panics when it
  cannot configure a surface (no usable GPU or no desktop), exiting with code
  101 and nothing on screen
  ([craft-anything PR #786](https://github.com/storytold/craft-anything/pull/786)).
* **macOS auxiliary viewports.** Native child viewports flicker or abort when
  opened from a fullscreen or maximised root window
  ([issue #8259](https://github.com/emilk/egui/issues/8259)).

The practical cause of most "blinking first screen" reports is the window
becoming visible before a themed frame exists, plus a default clear colour
showing through. "Direct draw" makes it worse where an app paints the
framebuffer itself instead of going through the frame pipeline.

## Decision

1. **Pin eframe 0.31.1 for the workspace.** The first-frame visibility fix is
   present (it landed long before 0.31). 0.36 and 0.37 were rejected for now
   because they replace the `App::update(Context)` + panel API with a
   `Ui`-rooted `App::ui` API and remove `egui::TopBottomPanel`/`SidePanel`;
   adopting that redesign is a separate decision, not a startup fix. Revisit
   when the redesign is documented and the reference projects' patterns are
   reviewed. Pinning one version also keeps `Cargo.lock` stable.
2. **Never show an unpainted window.** Do application setup (theme, fonts,
   project load, style scan) before the first frame, and if setup is slow, paint
   a themed startup surface rather than leaving the window blank. Do not rely on
   an OS default background.
3. **One clear colour from the v1 theme** (`background` `#0b0d0f`) for the first
   frame and every later frame, so no white default can flash through.
4. **wgpu is the default backend, with a handled failure path.** On macOS that
   is Metal. A surface-configuration failure must produce a visible message and a
   non-zero exit, never a panic with an empty screen; a glow fallback is
   acceptable if it keeps the window usable.
5. **No direct framebuffer painting from `App::update`.** Custom GPU work goes
   through egui paint callbacks. This matters for ADR-010's engraved score tiles,
   which will be textures, not direct clears.
6. **Avoid aux native viewports for editing panels.** Use egui areas/panels
   inside the main viewport; the macOS child-viewport flicker above is not worth
   the risk for a dockable inspector.
7. **Manual startup check is part of the Stage 6 exit**, on macOS and Windows:
   cold start shows the themed window with no white or black flash, a second
   launch from a warm cache behaves the same, and a forced wgpu failure shows a
   message instead of a blank window.

## Consequences

* Startup cost is paid before the first frame, so the shell must not block on
  style scanning or project analysis; those move to workers or a startup page.
* A small `startup.rs` module owns visibility, backend selection and the failure
  dialog, so the app entry point stays readable.
* CI stays compile-only for the desktop crate; flicker is a manual check,
  recorded in the Stage 6 exit notes rather than claimed as verified by tests.

## Evidence

* Upstream links above, read 2026-11-22.
* [ADR-001](ADR-001-ui-toolkit.md) for the egui decision and theme.
* [`docs/design/v1-visual-language.md`](../design/v1-visual-language.md) for the
  clear colour and metrics the first frame uses.
