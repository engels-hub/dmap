# dmap: design specification

This document describes the visual language of the DM window. Use it to build the UI in egui and to make new screens that match.

## 1. Principles

- dmap is a tool, not a dashboard. The canvas is the product. The chrome stays out of the way.
- The canvas fills the window. The toolbar and the panels float over it. No chrome takes a column or a row away from the canvas.
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
- Stroke 2 px on a 24 px grid. Round caps and joins. No fills. A star that is set is the one glyph with a fill.
- Sizes: 22 px in the toolbar, 20 px in dialog navigation and in a close button, 18 px in a panel, a menu and a list row, 18 px for the select chevron, 16 px for a list twist and for the checkbox check, 14 px in a path line.
- Glyphs in the toolbar: `mouse-pointer` (Select), `pencil` (Draw), `ruler` (Edit), `monitor` (Table), `layers` (Scenes), `plus` (Add map), `sliders-horizontal` (Settings).
- Glyphs on the second bar of the Edit view: `wall` (Walls), `door` (Doors), `sun` (Light), `fog` (Fog).
- Glyphs in a dialog: `grid-3x3` (Grid tab), `sun` (Light tab), `keyboard` (Shortcuts tab), `x` (close), `chevron-down` (select), `check` (checkbox), `import` (Import).
- Glyphs in a panel or a menu: `folder`, `image`, `film`, `eye`, `eye-off`, `monitor-off`, `star`, `search`, `chevron-right`, `arrow-up`, `arrow-down`, `rotate-cw`, `flip-horizontal`, `flip-vertical`, `ruler`, `trash-2`, `line`, `square`, `circle`, `eraser`, `play`, `pause`, `undo`, `alert`.

## 5. Layout of the DM window

The canvas fills the window. The toolbar floats at the bottom. A panel floats at the left or at the right. Each one appears only when the view or the selection asks for it.

### 5.1 Canvas

- Fills the window. Background is the user's canvas color. Default `canvas`.
- One grid covers the whole canvas. It lies over every map, so the DM lines a map up with it. A map carries no grid of its own.
- The grid is 1 px lines in `grid`, 48 px apart in the light theme and 64 px apart in the dark theme. One cell is one inch on the TV.
- Map previews have a hard shadow `2px 3px 0 shadow` when the theme has a shadow value.
- The DM camera pans with space+drag or the middle button and zooms with the wheel. There is no on-screen control for the DM camera.

### 5.2 Toolbar

- The toolbar floats over the canvas, centered, 16 px from the bottom edge. Background `surface`, 1 px `ink` border, and the hard shadow of the theme.
- Each entry is 64 px wide and 56 px high: icon 22 px, then the label 13 px, gap 4 px.
- The first group holds the views: Select, Draw, Edit, Table. A 1 px `rule` follows. The second group holds Scenes, Add map and Settings, because they are not views.
- The active entry has background `raised`, a 3 px `accent` bar on its top edge, and `accent` icon and label.
- The Edit view brings a second bar of the same shape. It sits 8 px over the toolbar and holds Walls, Doors, Light and Fog. The views stay on the screen under it.

### 5.3 Panels and their places

- The list of objects docks on the left, 16 px from the left, the top and the bottom edge. A child row indents to the right, into the panel, and never toward the canvas.
- The properties of the view or of the selection dock on the right, 16 px from the right and the top edge.
- One panel of each kind is open at a time. A panel opens with its view and closes with it.

### 5.4 TV box

- A rectangle with a 2 px `accent` outline. The aspect ratio is the TV's.
- The outline and the zoom label show in every view. The DM always sees what the TV shows.
- The dim wash and the four corner handles belong to the Table view alone. A click on the box reaches the map under it in the Select view.
- Four square handles, 8 px, `accent`, centered on the corners. They stand 4 px outside the box.
- The area outside the box gets the `dim` wash.
- The zoom label sits above the top-right corner, 14 px bold, for example `100 %`. The label is `accent` when the box snaps to true size. Otherwise it is `ink`.

### 5.5 Marker for a box off the screen

- The marker shows only when no part of the TV box is on the canvas.
- It is a solid `accent` triangle, 28 px tall and 20 px deep. It points at the box center.
- It sits on the canvas edge, where the line from the canvas center to the box center crosses that edge. It stays whole inside the canvas.
- The marker takes no click and no drag. It is paint only.

### 5.6 Marks on the canvas

- A picked map has a 2 px `accent` outline and four 8 px `accent` corner handles.
- A turn handle sits 24 px above the top edge, on a 1 px `accent` line.
- A picked group has a 2 px dashed `accent` box. A 1 px dash is too thin to read at a zoom that shows the whole canvas. The root group has no box.

## 6. Controls

All controls have square corners.

Two rules hold for every control:

- A control keeps the width it is given. Its border and its padding sit inside that width.
- A line of text takes 1.3 times its size. A row of a fixed height is at least that tall, and it holds its text on one line.

| Control | Spec |
|---|---|
| Text input | Height 36 px, 1 px `rule` border, `field` background, 10 px horizontal padding, 15 px text. A unit follows the input in `mute`. The border is `accent` while the input takes the key. |
| Select | As text input, width 300 px, value left, `chevron-down` right. |
| Open select | The list comes under the field and overlaps its border by 1 px. Each row is 36 px with 10 px horizontal padding. The chosen row has `raised` background, `accent` text, a 3 px `accent` bar on its left edge and a `check` glyph 16 px on its right. |
| Segmented control | Segments 36 px high, 14 px horizontal padding, 1 px `rule` border, borders overlap by 1 px. The selected segment has `raised` background, `accent` text and a 3 px `accent` bar on its bottom edge. |
| Checkbox | 20 px square, 1 px border (`ink` when checked, `rule` when not), `field` background, `check` glyph 16 px in `accent`. Label 15 px to the right, gap 10 px. |
| Slider | Track 2 px `rule`, filled part `ink`, knob 16 px square with 1 px `ink` border and `field` background. Value text in `mute` to the right. |
| Color swatch | 36 px square, 1 px `rule` border, the hex value 15 px to the right. A click opens the color popover. |
| Button | Height 36 px, 14 px horizontal padding, 1 px `ink` border, `field` background, 15 px text. A button in a panel is 32 px high. A button in a row is 28 px high. |
| Key chip | Height 26 px, 8 px horizontal padding, 1 px `rule` border, `field` background, 15 px text. |
| Search field | As text input, with a `search` glyph 18 px on the left and a 10 px gap. |
| Star | 18 px in a 26 px square. A star that is set is `accent` and filled. A star that is not set is a `mute` outline, and it shows under the pointer. |

## 7. Chrome over the canvas

A panel, a menu and a message strip share one recipe: a `surface` box, a 1 px `ink` border, and the hard shadow of the theme. All corners are square.

### 7.1 Panel

- Width 268 px to 316 px. The list of objects goes to 560 px. See 8.4.
- Header 40 px: the title 15 px bold at 14 px from the left, a close icon in a 28 px square on the right, 1 px `rule` bottom border.
- Body: padding 14 px, rows with a 14 px gap.
- A panel is narrower than a dialog. So the label of a row sits over its control, not beside it, and the control keeps the full width of the body. A label column of its own leaves too little room for a segmented control or a slider.
- Footer: 1 px `rule` top border, padding 12 px 14 px, buttons 32 px high with a 10 px gap. A row of buttons that does not fit takes a second line.

### 7.2 Menu

- The right button opens a menu at the pointer. Width 288 px to 304 px, padding 4 px at both ends.
- Each row is 32 px high with 14 px horizontal padding and 15 px text, on one line. An icon 18 px sits on the left, with a 10 px gap.
- The key of the row sits on the right in `mute`.
- The row under the pointer takes the `raised` background.
- A 1 px `rule` with a 4 px margin divides two groups of rows.
- A click on a row closes the menu. A click away closes it. `Escape` closes it. That click does not reach the canvas.

### 7.3 Message line

- One strip says what the program did, or what went wrong. It floats 16 px from the bottom-left corner of the canvas.
- Height 36 px, horizontal padding 14 px, icon 18 px, text 15 px, gap 10 px.
- A message that reports work uses the `check` glyph in `mute` and `ink` text.
- Caution: a message that reports a failure uses the `alert` glyph and the text in `accent`. Write the cause in the message.
- Work that takes time gets a strip of its own: the file name, the percent on the right, and a 2 px bar under them. The track is `rule` and the filled part is `ink`.

## 8. Panels for each view

### 8.1 Map panel, in the Select view

Rows: Map with the file name in `mute`; Pixels per cell with an input and the Measure a cell button; Size in percent; Turn in degrees. A helper line under Measure reads "Click two corners of one cell. Escape gives it up." The footer holds Turn, Flip and Delete.

### 8.2 TV box panel, in the Table view

Rows: Zoom in percent, with the helper "100 % is true size on the TV"; Move, with the helper "The arrow keys move the box one cell"; Frame, with a button that shows the whole box.

### 8.3 Draw panel, in the Draw view

A row of five 36 px squares holds the pen, the line, the rectangle, the ellipse and the eraser. The squares share a 1 px `rule` border and overlap by 1 px. The chosen square takes the segmented control treatment. Rows below: Color as a swatch, Width as a slider.

### 8.4 Objects list

The list shows the scene as a tree of groups and assets. It docks on the left.

**A row**

- Each row is 32 px high. A child row indents 18 px from its parent.
- A twist glyph 16 px opens and closes a group. A group starts closed.
- The icon says what the row is: `folder` for a group, `image` for a picture, `film` for a video.
- The name takes the width that is left. A name too long for its row ends in an ellipsis, and the whole name comes up under the pointer.
- The right of the row holds the star, then two switches, each 18 px in a 26 px square: `eye` for the DM screen and `monitor` for the TV. An off switch takes the `eye-off` or `monitor-off` glyph in `mute`.
- The root group is always visible and carries no switches.
- A picked row takes the `raised` background, a 3 px `accent` bar on its left edge, and `accent` text.

**The width of a deep tree**

A scene of a hundred thousand assets branches deep. Three things keep the list inside the panel, and none of them is a sideways scroll.

1. A click into a group puts that group at the top of the list. The list shows that group and its children, and never the whole tree from the root. So the indent has no reason to run away.
2. A path line 30 px high sits over the list and names the group the list shows. Each part takes a click and goes back up. A path of more than four parts drops its middle to an ellipsis.
3. A drag on the right edge of the panel takes the width from 280 px to 560 px. The program keeps the width.

**Search and favourites**

- A search field at the top of the list flattens it to the rows that match the name.
- Each result row is 44 px: the name 15 px over its path in `mute` 13 px. Both take an ellipsis.
- A star button 36 px square sits beside the field. It keeps the rows the DM marked, and it takes the `raised` background and an `accent` star while it is on.
- The result comes in two runs, Favourites first, then Everything else. A run has a label 26 px high in `mute` 13 px.
- A helper line under the list says how many rows match, of how many.

**The footer**

The footer holds Group and New group, and a helper line that names the picked nodes.

## 9. Dialogs

- A dialog opens over the canvas with the `scrim`. The canvas stays in place behind it.
- Size 760 × 520 px to 880 × 700 px, centered. Background `surface`, 1 px `ink` border, hard shadow `6px 8px 0` in `shadow` at 25 % when the theme has a shadow value.
- Header 52 px: title 18 px bold at 24 px from the left, close icon in a 36 px square at 16 px from the right, 1 px `rule` bottom border.
- Navigation column 200 px wide, 1 px `rule` right border, 12 px vertical padding. Each entry: icon 20 px, label 15 px, gap 10 px, padding 10 px 16 px. The active entry has `raised` background and a 3 px `accent` bar on its left edge.
- Body: padding 24 px 28 px, rows with a 22 px gap. Each row has a label column 170 px wide (15 px, 8 px top padding) and a control column with an 8 px gap between stacked controls. A dialog is wide enough for a label column, and a panel is not.
- Footer, where a dialog has one: 60 px high, 1 px `rule` top border, padding 0 24 px, buttons with a 12 px gap. The last button sits on the right.

### 9.1 Settings, Table tab

1. Display: select with the display name and its resolution.
2. Size: segmented control (Diagonal, Pixels per inch, Width), then one input with its unit, then a helper line with the computed pixels per inch and the size of the table area at true size.
3. Snap to true size: input in percent, helper text "either side of 100 %".
4. Windows: checkbox "Swap the two windows instead of moving the DM window".
5. Check: checkbox "Show a 1 inch grid and a 6 inch ruler on the TV".

### 9.2 Settings, Grid tab

1. Canvas background: color swatch. Helper: "Shown on both screens where no map is".
2. Kind: segmented control (Square, Hex pointy top, Hex flat top, None).
3. Cell size: input in inches. Helper: "Hex size is measured flat to flat".
4. Show on: segmented control (Both screens, DM only, TV only).
5. Line: color swatch, width input in TV pixels, opacity slider, style segmented control (Solid, Dashed, Dots).

### 9.3 Settings, Light tab

1. Darkness: slider. Helper: "0 shows the map as it is. 1 shows the lit areas only".
2. Ambient color: color swatch.
3. Walls: checkbox "Walls block the light", checkbox "An open door lets the light through".
4. Shadow edge: segmented control (Hard, Soft). Helper: "A soft edge costs more on the GPU".
5. Show on: segmented control (Both screens, DM only, TV only).

### 9.4 Settings, Shortcuts tab

- The view the DM works in comes first. A 15 px bold line names each group.
- Each row is 42 px high with a 1 px `rule` bottom border: the action 15 px in a 320 px column, then the key chip, then the Change button 28 px high on the right.
- The chip takes an `accent` border and the text "Press a key" while the program waits for the new key.
- Caution: the program refuses a key that another control holds. A 14 px `accent` line under the row names that control.
- The footer holds "Bring back the default keys" on the left and Close on the right.

### 9.5 Scenes

- Row at the top: Scenes folder, the path in an input 420 px wide, and the Change button.
- Each scene is a row 52 px high with a 1 px `rule` bottom border: the `folder` icon and the name on the left, the buttons on the right.
- The open scene is `accent` and bold, and it carries the text "Open now" in place of the Open button.
- The buttons are Open, Rename, Folder and Delete, each 28 px high.
- Caution: Delete takes the scene folder and every map in it. The row asks once, on the row itself. The question comes before the Delete and Keep buttons.
- The footer holds New scene on the left and Close on the right.

### 9.6 Import

- Row: File, with the file name in an input and the Choose button.
- Helper: "dmap reads PNG, JPEG, MP4, WebM, a Foundry VTT scene and a Universal VTT file".
- Row: The file holds, with one checkbox for each thing the importer found. Each checkbox starts on.
- Row: Foundry folder, with the path and the Change button. Helper: "A Foundry scene needs this. A Universal VTT file does not".
- The footer holds Import on the left and Cancel on the right.

## 10. Other overlays

- The color popover: a grid of 36 px swatches four across, then a hex input, then the helper "Click a swatch, or type a hex value".
- The tooltip: height 28 px, 10 px horizontal padding, the label 14 px and its key in `mute`, gap 10 px.
- A file over the window: a 2 px `accent` outline 8 px inside the canvas edge, an `accent` wash at 6 %, and a box at the center that names the scene the file goes into.
- The empty state: a box at the center with the title 18 px bold, a helper line, and the New scene and Open a scene buttons.
- `F3` shows a box on the right with the frame rate, the size of each window, the drawn and culled counts, the resident tiles, the draw calls and the state of the scene. Each row is 22 px, on one line: the name 14 px `mute` in a 140 px column, then the value 14 px `ink`.
