# dmap: design specification

This document describes the visual language of the DM window. Use it to build the UI in egui and to make new screens that match.

## 1. Principles

- dmap is a tool, not a dashboard. The canvas is the product. The chrome stays out of the way.
- Less is more. Show only what a session needs. Put all other controls behind Settings or behind an edit mode that the user enters on purpose.
- Legibility comes first. No text is smaller than 13 px. Body text in dialogs is 15 px.
- One accent color only. The accent marks the active tool, the TV box, the snapped zoom, the selected segment and the checked box. Nothing else uses it.
- No gradients, no rounded corners, no drop shadows with blur, no emoji. Shadows are hard offsets.
- The user places maps on the canvas by hand. The program does not align maps or grids for the user.

## 2. Color tokens

The UI has a light theme and a dark theme. The light theme is the default. Both themes use the same token names and the same layout. Only the values change.

| Token | Light | Dark | Use |
|---|---|---|---|
| `surface` | `#ece4d2` | `#191a1c` | Rail, dialog surface |
| `canvas` | `#e3d9c3` | `#101112` | Canvas background default |
| `raised` | `#e3d9c3` | `#2c2e31` | Active tool background, selected segment, active navigation entry |
| `field` | `#f5efe2` | `#1f2124` | Input, select, button and checkbox background |
| `ink` | `#2b2419` | `#dcd8cf` | Text, icons, dialog border |
| `mute` | `#7d7462` | `#8a877f` | Secondary text, units, helper lines |
| `rule` | `#b9ad92` | `#2c2e31` | 1 px borders and dividers |
| `grid` | `#9fb1c4` at 33 % | `#2c2e31` | Canvas grid lines |
| `accent` | `#b4452c` | `#f0a830` | See section 1 |
| `dim` | `rgba(43, 36, 25, 0.12)` | `rgba(16, 17, 18, 0.45)` | Wash outside the TV box |
| `scrim` | `rgba(43, 36, 25, 0.35)` | `rgba(0, 0, 0, 0.5)` | Behind a dialog |
| `shadow` | `#b9ad92` | none | Hard offset shadow under map previews and dialogs |

The light theme is warm: cream surfaces, dark brown ink, vermilion accent. The dark theme is neutral: near-black surfaces, warm grey text, amber accent.

## 3. Typography

- Font: Atkinson Hyperlegible, weights 400 and 700. Fallback: Helvetica Neue, Arial, sans-serif.
- Numbers use tabular figures.
- Sizes:

| Use | Size | Weight |
|---|---|---|
| Rail labels | 13 px | 400 |
| Zoom label on the TV box | 14 px | 700 |
| Helper text under a control | 14 px | 400, `mute` |
| Control text, dialog navigation, row labels | 15 px | 400 |
| Dialog title | 18 px | 700 |

## 4. Icons

- Set: Lucide (ISC license). Keep the license text in the repository.
- Stroke 2 px on a 24 px grid. Round caps and joins. No fills.
- Sizes: 22 px in the rail, 20 px in dialog navigation and the close button, 18 px for the select chevron, 16 px for the checkbox check.
- Glyphs in use: `mouse-pointer` (Select), `pencil` (Draw), `monitor` (Table), `plus` (Add map), `sliders-horizontal` (Settings), `grid-3x3` (Grid tab), `sun` (Light tab), `keyboard` (Shortcuts tab), `x` (close), `chevron-down` (select), `check` (checkbox).

## 5. Layout of the DM window

The window has two parts: the rail and the canvas. Nothing else is visible during play.

### 5.1 Rail

- Width 72 px, full height, on the left. Background `surface`. Right border 1 px `rule`.
- Top group: Select, Draw, Table. Bottom group: Add map, Settings. Padding 8 px at both ends.
- Each tool is a 72 px wide column: icon 22 px, then the label 13 px, gap 4 px, vertical padding 10 px.
- The active tool has background `raised`, a 3 px `accent` bar on its left edge, and `accent` icon and label.

### 5.2 Canvas

- Fills the rest of the window. Background is the user's canvas color. Default `canvas`.
- The canvas grid is drawn on top of the background: 1 px lines in `grid`, 48 px apart in the light theme and 64 px apart in the dark theme.
- Map previews have a hard shadow `2px 3px 0 shadow` when the theme has a shadow value.
- The DM camera pans with space+drag or the middle button and zooms with the wheel. There is no on-screen control for the DM camera.

### 5.3 TV box

- A rectangle with a 2 px `accent` outline. The aspect ratio is the TV's.
- Four square handles, 8 px, `accent`, centered on the corners.
- The area outside the box gets the `dim` wash.
- The zoom label sits above the top-right corner, 14 px bold, for example `100 %`. The label is `accent` when snapped to true size. Otherwise it is `ink`.

## 6. Controls

All controls have square corners.

| Control | Spec |
|---|---|
| Text input | Height 36 px, 1 px `rule` border, `field` background, 10 px horizontal padding, 15 px text. A unit follows the input in `mute`. |
| Select | As text input, width 300 px, value left, `chevron-down` right. |
| Segmented control | Segments 36 px high, 14 px horizontal padding, 1 px `rule` border, borders overlap by 1 px. The selected segment has `raised` background, `accent` text and a 3 px `accent` bar on its bottom edge. |
| Checkbox | 20 px square, 1 px border (`ink` when checked, `rule` when not), `field` background, `check` glyph 16 px in `accent`. Label 15 px to the right, gap 10 px. |
| Slider | Track 2 px `rule`, filled part `ink`, knob 16 px square with 1 px `ink` border and `field` background. Value text in `mute` to the right. |
| Color swatch | 36 px square, 1 px `rule` border, the hex value 15 px to the right. Click opens the platform color picker. |
| Button | Height 36 px, 14 px horizontal padding, 1 px `ink` border, `field` background, 15 px text. |

## 7. Settings dialog

- Opens over the canvas with the `scrim`. The canvas stays in place behind it.
- Size 820 × 520 px, centered. Background `surface`, 1 px `ink` border, hard shadow `6px 8px 0` in `shadow` at 25 % when the theme has a shadow value.
- Header 52 px: title 18 px bold at 24 px from the left, close icon in a 36 px square at 16 px from the right, 1 px `rule` bottom border.
- Navigation column 200 px wide, 1 px `rule` right border, 12 px vertical padding. Each entry: icon 20 px, label 15 px, gap 10 px, padding 10 px 16 px. The active entry has `raised` background and a 3 px `accent` bar on its left edge.
- Body: padding 24 px 28 px, rows with a 22 px gap. Each row has a label column 170 px wide (15 px, 8 px top padding) and a control column with an 8 px gap between stacked controls.

### 7.1 Table tab

1. Display: select with the display name and its resolution.
2. Size: segmented control (Diagonal, Pixels per inch, Width), then one input with its unit, then a helper line with the computed pixels per inch and the size of the table area at true size.
3. Snap to true size: input in percent, helper text "either side of 100 %".
4. Check: checkbox "Show a 1 inch grid and a 6 inch ruler on the TV".

### 7.2 Grid tab

1. Canvas background: color swatch. Helper: "Shown on both screens where no map is".
2. Kind: segmented control (Square, Hex pointy top, Hex flat top, None).
3. Cell size: input in inches. Helper: "Hex size is measured flat to flat".
4. Show on: segmented control (Both screens, DM only, TV only).
5. Line: color swatch, width input in TV pixels, opacity slider, style segmented control (Solid, Dashed, Dots).

### 7.3 Other tabs

Light and Shortcuts are listed but not designed yet.
