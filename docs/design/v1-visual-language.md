# Lazy Arranger v1 visual language

Extracted from the C++/JUCE application so the egui shell can match the look the
product already has. Values are copied from source, not eyeballed.

**Source:** `arranger-midi-daw` at `8ca41644`:
`resources/themes/dark.json`, `src/app/ui/AppLookAndFeel.cpp`,
`resources/fonts/`.

The theme names itself **"Studio Console"**: near-black surfaces, warm off-white
text, coral accent, LCD-style displays for transport and tempo, and a 7-colour
track palette.

## Palette (dark theme, the only complete one)

| Token | Hex | Use |
| --- | --- | --- |
| `background` | `#0b0d0f` | window |
| `panel` | `#121518` | panels, list boxes, alerts |
| `panelRaised` | `#1a1e22` | buttons, editors, combo boxes, menus |
| `line` | `#23282d` | separators, scrollbar thumb |
| `lineStrong` | `#353c44` | emphasised separators and outlines |
| `text` | `#eee9e1` | primary text |
| `textMuted` | `#a69e93` | secondary text, combo arrow |
| `textDim` | `#6e6961` | tertiary text |
| `accent` | `#fc7256` | selection, focus, slider track, caret |
| `accentText` | `#1b1208` | text on the accent |
| `playing` | `#7ecb72` | transport playing |
| `recording` | `#dc7a6b` | record arm |
| `ok` | `#91ad7c` | success |
| `warning` | `#dca45f` | warning |
| `playhead` | `#e7bd86` | playhead |
| `timeline` | `#0a0c0e` | timeline background |
| `lane` / `laneAlt` / `laneSelected` | `#101316` / `#13171a` / `#1b2025` | track lanes |
| `gridBar` / `gridBeat` | `#2b3138` / `#1d2227` | bar and beat grid |
| `lcd` | `#07090a` | LCD face |
| `lcdText` | `#f1ece4` | LCD text |
| `lcdBack` | `#b3bbb0` | LCD backlight |
| `lcdInk` / `lcdDim` / `lcdBezel` | `#1d241d` / `#4d574d` / `#2a2e2b` | LCD detail |
| `faderFaceTop` / `faderFaceBottom` / `faderEdge` / `faderLine` | `#f4f4f1` / `#a8adb1` / `#111315` / `#f7f8f5` | fader chrome |

Track colours, in palette order: coral `#fc7256`, amber `#d9c195`, teal
`#6aa79a`, blue `#7693b5`, violet `#9a8bbb`, green `#8ba97b`, pink `#fba6a3`.

## Metrics

| Token | Value | Notes |
| --- | --- | --- |
| `grid` | 4 | base spacing unit in points |
| `radius` | 9 | panels |
| `radiusSmall` | 7 | buttons, editors, LCD |
| `headerHeight` | 60 | top bar |
| `statusHeight` | 28 | status bar |
| `sidebarWidth` | 248 | browser |
| `inspectorWidth` | 296 | inspector |
| `trackHeight` | 64 | one track lane |
| `fontSize` | 13 | base UI text |

## Typography

The UI font is **Inter** (`Inter-Regular.ttf`, `Inter-SemiBold.ttf`, SIL OFL
1.1). JUCE substitutes Inter for the default sans face at 12 pt in the current
theme; text buttons, combo boxes and labels use the theme font at 13 pt
(`AppLookAndFeel.cpp:62-66,87,181,183`). The score font is **Bravura**
(SMuFL, SIL OFL 1.1), used only by notation.

Licence note: OFL requires the licence text to travel with the font files. The
C++ repository already carries `Inter-LICENSE.txt` and `Bravura-LICENSE.txt`;
the same files must ship with any copy into this project.

## Mapping to egui

| JUCE concept | egui equivalent |
| --- | --- |
| `setColourScheme` + `setColour` ids | `egui::Visuals` fields plus a small `Theme` struct built from the tokens above |
| `metric(name)` | a `Metrics` struct; feed `Visuals::window_rounding`, `widgets.*.rounding`, `spacing.item_spacing` from `grid` |
| `Theme::font(size, bold)` | `FontDefinitions` with Inter regular and semibold, `FontId::proportional(13.0)` |
| Custom LCD and fader drawing | `egui::Painter` in a small widget module; no layout decisions there |
| Theme JSON with `dark`/`light` | Ship both; only `dark.json` exists today, so a light theme is new work |

Two rules carried over from ADR-001 and ADR-010:

1. The theme is data, not code: one `Theme` value with the tokens above, loaded
   once and cloned into contexts. No colour literals in widgets.
2. The score canvas will render engraved glyphs from Bravura through the
   engraver's layout (ADR-010), not through egui text shaping, so the theme only
   supplies colour and size there.

## Implemented in the shell

The header now reproduces `HeaderBar.cpp` and `LcdDisplay::paint` rather than
approximating them:

* panel fill with a 1 px `line` bottom edge, a 220 px title area with the round
  `accent`/`accentText` badge, 16 pt title and 12.5 pt `textMuted` subtitle;
* transport circles at the v1 sizes (stop 44, play 54, 8 px gaps);
* a 350 x 52 LCD in three cells of 142/116/92: `lcdBezel` bezel with a
  `lineStrong` outline, `lcdBack` glass with a top highlight, 7% scanlines every
  3 px, `lcdDim` separators and 9.5 pt labels, values in 20 pt `lcdInk`;
* the bar/beat text uses the oracle's `bar . beat . sixteenth` form
  (`Time.cpp::formatBarBeat`), tempo prints with two decimals and meter as
  `4 / 4`;
* view tabs on a `lcd` strip with `accent` 14% fill, 40% outline, a 26 px accent
  underline and 10.5 pt labels.

Remaining differences from v1 that need screenshots to settle: icon badge
(the app icon is not copied), status pills, zoom buttons, mixer and AI buttons,
and the record, loop and click transport buttons (not implemented yet).

## Deliberate deviation: square corners

The owner asked for **no rounded rectangles**. v1 used `radius` 9 for panels and
`radiusSmall` 7 for controls; the Rust shell now uses 0 for every rounded
rectangle: header badge, LCD bezel and glass, tab strip, selected-tab fill and
underline, track colour swatches, and piano-roll notes. Circles are kept only
where v1 uses actual circles (transport buttons). The app icon artwork keeps its
own rounded shape because it is the supplied v1 art, not a UI rectangle.

## App icon

The v1 icon (keyboard, wave and note blocks) is reused in the Rust shell,
ringed by a generic steel-style cog in Rust orange. The gear is drawn from
scratch, not the official Rust logo mark, so no Rust Foundation trademark is
used. The source art belongs to this product.

Files in `apps/desktop/assets/`: `app-icon.png` (1024), sized variants down to
32 px, and `app-icon-256.rgba` (raw RGBA) for the window icon and the header
badge, which no longer shows the placeholder "A".

## What is not in v1 and must be designed

* A light theme (the JSON exists as a file name only).
* Focus and keyboard-navigation visuals: JUCE relied on its own focus outline;
  egui needs explicit `widgets.noninteractive`/`active` stroke rules.
* Disabled states and busy states beyond the colour set above.
