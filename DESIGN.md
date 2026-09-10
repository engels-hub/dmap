# dmap: design specification

This document describes the visual language of the DM window. Use it to build the UI in egui and to make new screens that match.

## 1. Principles

- dmap is a tool, not a dashboard. The canvas is the product. The chrome stays out of the way.
- The canvas fills the window. The toolbar and the panels float over it. No chrome takes a column or a row away from the canvas.
- Less is more. Show only what a session needs. Put all other controls behind Settings or behind an edit mode that the user enters on purpose.
- Legibility comes first. No text is smaller than 13 px. Control text is 14 px.
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
| Toolbar labels, helper text, small print | 13 px | 400 |
| Control text, dialog navigation, row labels | 14 px | 400 |
| Zoom label on the TV box | 14 px | 700 |
| Panel title | 14 px | 700 |
| Dialog title | 16 px | 700 |

Helper text takes the `mute` token.

### 3.1 The interface scale

Every size in this document is a size at scale 1. The DM picks a scale between 75 % and 175 % in Settings, and the program multiplies each size by it.

- The scale reaches the toolbar, the panels, the dialogs and the text. It does not reach the canvas. A map keeps its size on the screen, and one grid cell stays one inch.
- The scale is one number for the whole window. No screen takes a scale of its own.
- The program keeps the choice between runs.
- A step is 5 %, so the value stays a round number.

Caution: a scale under 100 % takes a button under the 24 px floor of section 6. That is the DM's choice, and the floor holds for the design itself.

## 4. Icons

- Set: Lucide (ISC license). Keep the license text in the repository.
- Stroke 2 px on a 24 px grid. Round caps and joins. No fills. A star that is set is the one glyph with a fill.
- Sizes: 18 px in the toolbar, in dialog navigation, in a close button and for the select chevron, 16 px in a panel, a menu and a list row, 14 px for a list twist and for the checkbox check, 13 px in a path line.
- Glyphs in the toolbar: `mouse-pointer` (Select), `pencil` (Draw), `drafting-compass` (Edit), `monitor` (Table), `layers` (Scenes), `plus` (Add map), `sliders-horizontal` (Settings).
- Glyphs on the second bar of the Edit view: `brick-wall` (Walls), `door-open` (Doors), `sun` (Light), `cloud-fog` (Fog).
- Glyphs in a dialog: `monitor` (Table tab), `grid-3x3` (Grid tab), `sun` (Light tab), `keyboard` (Shortcuts tab), `info` (About tab), `x` (close), `chevron-down` (select), `check` (checkbox), `import` (Import).
- Glyphs in a panel or a menu: `folder`, `image`, `film`, `eye`, `eye-off`, `monitor-off`, `star`, `search`, `chevron-right`, `arrow-up`, `arrow-down`, `rotate-cw`, `flip-horizontal-2`, `flip-vertical-2`, `ruler`, `trash`, `minus` (line), `square`, `circle`, `eraser`, `play`, `pause`, `undo`, `triangle-alert`.

Every name above is a name the Lucide set holds. Check a new one against the set before this document takes it: the set renames a glyph from time to time, and a name that reads well is not always a name that exists.

## 5. Layout of the DM window

The canvas fills the window. The toolbar floats at the bottom. A panel floats at the left or at the right. Each one appears only when the view or the selection asks for it.

### 5.1 Canvas

- Fills the window. Background is the user's canvas color. Default `canvas`.
- One grid covers the whole canvas. It lies over every map, so the DM lines a map up with it. A map carries no grid of its own.
- Both screens draw the grid. The players see the same cells the DM does.
- The grid is 1 px lines in `grid`. One cell is one inch, on the DM screen and on the TV. A camera far enough out would draw those lines closer together than the eye can read, so the step doubles until a cell is at least 24 px wide. Every line that remains was a line before.
- Map previews have a hard shadow `2px 3px 0 shadow` when the theme has a shadow value.
- The DM camera pans with space+drag or the middle button and zooms with the wheel. There is no on-screen control for the DM camera.

### 5.2 Toolbar

- The toolbar floats over the canvas, centered, 12 px from the bottom edge. Background `surface`, 1 px `ink` border, and the hard shadow of the theme.
- Each entry is 52 px wide and 44 px high: icon 18 px, then the label 13 px, gap 3 px. An entry grows past 52 px when its label needs the room.
- The toolbar comes in two boxes, 8 px apart. Each box takes the `surface` background, the 1 px `ink` border and the hard shadow.
- The first box holds the views: Select, Draw, Edit, Table. It is a segmented control, so one of them is always on.
- A dashed 1 px line stands between each pair of views, dash and gap 3 px. It takes the `ink` of the box, not the `rule` of section 6: a lighter line beside an `ink` border reads as a seam, not as a join.
- The dash is what tells the two boxes apart. A solid line would give the views the same divided look as the buttons beside them, and the eye would read one long control in two halves.
- The second box holds Scenes, Add map and Settings. They are buttons, not views: none of them stays on, and no rule gathers them into one control. Each opens something at once.
- Two boxes say this louder than one rule between two groups. A DM reads the shape before the label.
- The active entry has background `raised`, a 3 px `accent` bar on its top edge, and `accent` icon and label.
- The Edit view brings a second bar of the same shape. It sits 8 px over the toolbar and holds Walls, Doors, Light and Fog. The views stay on the screen under it.

### 5.3 Panels and their places

- The list of objects docks on the left, 12 px from the left, the top and the bottom edge. A child row indents to the right, into the panel, and never toward the canvas.
- The properties of the view or of the selection dock on the right, 12 px from the right and the top edge.
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
- A map the TV does not show draws at half strength on the DM screen. A group that is off for the TV takes every map under it to half strength. The TV draws every map it shows at full strength.

### 5.7 History button

- The button floats 12 px from the right edge and 12 px from the bottom edge. It stands level with the toolbar.
- It takes the shape of a toolbar entry: 44 px high, the `undo` glyph 18 px over the label 13 px, on a `surface` box with a 1 px `ink` border and the hard shadow.
- It sits apart from the toolbar, because it belongs to no view. Every view writes to the history.
- A press opens the History dialog. A second press closes it.

## 6. Controls

All controls have square corners.

Two rules hold for every control:

- A control keeps the width it is given. Its border and its padding sit inside that width.
- A line of text takes 1.3 times its size. A row of a fixed height is at least that tall, and it holds its text on one line.

| Control | Spec |
|---|---|
| Text input | Height 28 px, 1 px `rule` border, `field` background, 8 px horizontal padding, 14 px text. A unit follows the input in `mute`. The border is `accent` while the input takes the key. |
| Select | As text input, width 260 px, value left, `chevron-down` right. |
| Open select | The list comes under the field and overlaps its border by 1 px. Each row is 28 px with 8 px horizontal padding. The chosen row has `raised` background, `accent` text, a 3 px `accent` bar on its left edge and a `check` glyph 14 px on its right. |
| Segmented control | Segments 28 px high, 10 px horizontal padding, 1 px `rule` border, borders overlap by 1 px. The selected segment has `raised` background, `accent` text and a 3 px `accent` bar on its bottom edge. |
| Checkbox | 18 px square, 1 px border (`ink` when checked, `rule` when not), `field` background, `check` glyph 14 px in `accent`. Label 14 px to the right, gap 8 px. A label too long for its row wraps under itself. |
| Slider | Track 2 px `rule`, filled part `ink`, knob 14 px square with 1 px `ink` border and `field` background. Value text in `mute` to the right. |
| Color swatch | 28 px square, 1 px `rule` border, the hex value 14 px to the right. A click opens the color popover. |
| Button | Height 28 px, 10 px horizontal padding, 1 px `ink` border, `field` background, 14 px text. A button in a panel is 26 px high. A button in a row is 24 px high. No button goes under 24 px. |
| Key chip | Height 22 px, 8 px horizontal padding, 1 px `rule` border, `field` background, 14 px text. |
| Search field | As text input, with a `search` glyph 16 px on the left and an 8 px gap. |
| Star | 16 px in a 22 px square. A star that is set is `accent` and filled. A star that is not set is a `mute` outline, and it shows under the pointer. |

## 7. Chrome over the canvas

A panel, a menu and a message strip share one recipe: a `surface` box, a 1 px `ink` border, and the hard shadow of the theme. All corners are square.

A fill inside one of these boxes stays inside its border. A fill that ran to the edge would rub the border out where it sits, and the box would lose its outline under the row or the entry that carries the fill.

### 7.1 Panel

- Width 232 px to 268 px. The list of objects goes to 480 px. See 8.4.
- Header 32 px: the title 14 px bold at 10 px from the left, a close icon in a 24 px square on the right, 1 px `rule` bottom border.
- The title names what the panel is about: the file of a map, or the name of the view. A title too long for the header ends in an ellipsis, and the whole title comes up under the pointer.
- Body: padding 10 px, rows with a 10 px gap.
- A panel is narrower than a dialog. So the label of a row sits over its control, not beside it, and the control keeps the full width of the body. A label column of its own leaves too little room for a segmented control or a slider.
- Footer: 1 px `rule` top border, padding 10 px, buttons 26 px high with an 8 px gap. A row of buttons that does not fit takes a second line.

### 7.2 Menu

- The right button opens a menu at the pointer. Width 248 px to 264 px, padding 4 px at both ends.
- Each row is 26 px high with 10 px horizontal padding and 14 px text, on one line. An icon 16 px sits on the left, with an 8 px gap.
- The key of the row sits on the right in `mute`.
- The row under the pointer takes the `raised` background.
- A 1 px `rule` with a 4 px margin divides two groups of rows.
- A click on a row closes the menu. A click away closes it. `Escape` closes it. That click does not reach the canvas.

### 7.3 Message line

- One strip says what the program did, or what went wrong. It floats 12 px from the bottom-left corner of the canvas.
- Height 28 px, horizontal padding 10 px, icon 16 px, text 14 px, gap 8 px.
- A message that reports work uses the `check` glyph in `mute` and `ink` text.
- Caution: a message that reports a failure uses the `alert` glyph and the text in `accent`. Write the cause in the message.
- Work that takes time gets a strip of its own: the file name, the percent on the right, and a 2 px bar under them. The track is `rule` and the filled part is `ink`.

## 8. Panels for each view

### 8.1 Map panel, in the Select view

The title of the panel is the file name. Rows: Group with a select that moves the map to another group; Pixels per cell with an input and the Measure a cell button; Size in percent; Turn in degrees. A helper line under Measure reads "Click two corners of one cell. Escape gives it up." The footer holds Turn, Flip and Delete.

### 8.2 TV box panel, in the Table view

Rows: Zoom in percent, with the helper "100 % is true size on the TV"; Move, with the helper "The arrow keys move the box one cell"; Frame, with a button that shows the whole box.

### 8.3 Draw panel, in the Draw view

A row of six 28 px squares holds the pen, the line, the rectangle, the ellipse, the eraser and the ruler. The squares share a 1 px `rule` border and overlap by 1 px. The chosen square takes the segmented control treatment. Rows below: Color as a swatch, Width as a slider. The ruler takes the same color and the same width as a stroke.

The ruler measures a distance. A drag draws a line from the press to the pointer, and a label at the pointer gives the distance. A click in the drag sets a waypoint, so the line bends. `Shift` as the drag ends keeps the measure: the line becomes an asset in the scene, with a row in the Objects list and a place in the project file. A plain end clears the line.

### 8.4 Objects list

The list shows the scene as a tree of groups and assets. It docks on the left.

**A row**

- Each row is 26 px high. A child row indents 14 px from its parent.
- A twist glyph 14 px opens and closes a group. A group starts closed.
- The icon says what the row is: `folder` for a group, `image` for a picture, `film` for a video, `ruler` for a kept measure.
- The name takes the width that is left. A name too long for its row ends in an ellipsis, and the whole name comes up under the pointer.
- The right of the row holds the star, then two switches, each 16 px in a 22 px square: `eye` for the DM screen and `monitor` for the TV. An off switch takes the `eye-off` or `monitor-off` glyph in `mute`.
- The root group is always visible and carries no switches.
- A picked row takes the `raised` background, a 3 px `accent` bar on its left edge, and `accent` text.
- A group has no panel of its own. It carries no size and no turn, and its two switches sit on its row, so a panel over it would hold nothing the row does not say. A double click on the name of the group at the top of the list opens it for a rename. Only that row takes the double click, because a click on any other row moves every row. The root keeps its own name.

**The width of a deep tree**

A scene of a hundred thousand assets branches deep. Three things keep the list inside the panel, and none of them is a sideways scroll.

1. A click into a group puts that group at the top of the list. The list shows that group and its children, and never the whole tree from the root. So the indent has no reason to run away.
2. A path line 24 px high sits over the list and names the group the list shows. Each part takes a click and goes back up. A path of more than four parts drops its middle to an ellipsis.
3. A drag on the right edge of the panel takes the width from 240 px to 480 px. The program keeps the width.

**Search and favourites**

- A search field at the top of the list flattens it to the rows that match the name.
- Each result row is 36 px: the name 14 px over its path in `mute` 13 px. Both take an ellipsis.
- A star button 28 px square sits beside the field. It keeps the rows the DM marked, and it takes the `raised` background and an `accent` star while it is on.
- The result comes in two runs, Favourites first, then Everything else. A run has a label 22 px high in `mute` 13 px.
- A helper line under the list says how many rows match, of how many.

**The footer**

The footer holds Group and New group, and a helper line that names the picked nodes.

## 9. Dialogs

- A dialog opens over the canvas with the `scrim`. The canvas stays in place behind it.
- Size 640 × 440 px to 740 × 580 px, centered. Background `surface`, 1 px `ink` border, hard shadow `6px 8px 0` in `shadow` at 25 % when the theme has a shadow value.
- Header 42 px: title 16 px bold at 20 px from the left, close icon in a 28 px square at 12 px from the right, 1 px `rule` bottom border.
- Navigation column 168 px wide, 1 px `rule` right border, 10 px vertical padding. Each entry is 32 px high: icon 18 px, label 14 px, gap 8 px, padding 12 px on the left. The active entry has `raised` background and a 3 px `accent` bar on its left edge.
- Body: padding 18 px 20 px, rows with a 16 px gap. Each row has a label column 140 px wide (14 px, 6 px top padding) and a control column with a 6 px gap between stacked controls. A dialog is wide enough for a label column, and a panel is not.
- Footer, where a dialog has one: 48 px high, 1 px `rule` top border, padding 0 20 px, buttons with a 10 px gap. The last button sits on the right.

### 9.1 Settings, Table tab

1. Display: select with the display name and its resolution.
2. Size: segmented control (Diagonal, Pixels per inch, Width), then one input with its unit, then a helper line with the computed pixels per inch and the size of the table area at true size.
3. Snap to true size: input in percent, helper text "either side of 100 %".
4. Windows: checkbox "Swap the two windows instead of moving the DM window".
5. Check: checkbox "Show a 1 inch grid and a 6 inch ruler on the TV".
6. Theme: segmented control (Light, Dark). See section 2.
7. Interface scale: slider with the percent to its right. Helper: "How big the toolbar, the panels and the dialogs draw. The maps keep their size". See section 3.1.

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

- The view the DM works in comes first. A 14 px bold line names each group.
- Each row is 34 px high with a 1 px `rule` bottom border: the action 14 px in a 270 px column, then the key chip, then the Change button 24 px high on the right.
- The chip takes an `accent` border and the text "Press a key" while the program waits for the new key.
- Caution: the program refuses a key that another control holds. A 14 px `accent` line under the row names that control.
- The footer holds "Bring back the default keys" on the left and Close on the right.

### 9.5 Settings, About tab

1. The name of the program and the version it runs, then one helper line that says what it is.
2. One row for each thing dmap carries: the name, the license it comes under, and a link that opens the source in the browser.
3. Built by: the people who built dmap, in one line.
4. One line that asks for help, with a link to the issues.
5. Licenses: a segmented control (dmap, Font, Glyphs), then the whole text of the one it marks, in a pane that scrolls.

This tab is a page to read, not a row of controls to fill in, so its lines sit closer together than the 16 px of section 9. The text pane takes the room that is left, so the tab needs no second scroll around the one the text already has. Two scrolls in a column leave a DM guessing which one a wheel turns.

The names come from the `CONTRIBUTORS` file at the top of the repository, one to a line. A person adds their own in the pull request that brings their first change.

The whole text is built into the program. A link on its own does not do: the GPL asks that a copy of it reach every person who gets the program, and the font and the glyph set ask that their notice travel with them. The font is compiled in, so its license has nowhere else to go.

The release archive keeps the same three files beside the program. This tab is the second way to them, for a DM who holds the binary alone.

### 9.6 Scenes

- Row at the top: Scenes folder, the path in an input 360 px wide, and the Change button.
- Each scene is a row 40 px high with a 1 px `rule` bottom border: the `folder` icon and the name on the left, the buttons on the right.
- The open scene is `accent` and bold, and it carries the text "Open now" in place of the Open button.
- The buttons are Open, Rename, Folder and Delete, each 24 px high.
- Caution: Delete takes the scene folder and every map in it. The row asks once, on the row itself. The question comes before the Delete and Keep buttons.
- The footer holds New scene on the left and Close on the right.

### 9.7 Import

- Row: File, with the file name in an input and the Choose button.
- Helper: "dmap reads PNG, JPEG, MP4, WebM, a Foundry VTT scene and a Universal VTT file".
- Row: The file holds, with one checkbox for each thing the importer found. Each checkbox starts on.
- Row: Foundry folder, with the path and the Change button. Helper: "A Foundry scene needs this. A Universal VTT file does not".
- The footer holds Import on the left and Cancel on the right.

### 9.8 History

- Size 520 x 460 px.
- Each step is a row 46 px high with a 1 px `rule` bottom border, and 12 px of padding on each side.
- A row holds two lines. The first line 14 px names what the DM did and the file or the group it happened to, such as "Move ivan.jpg". The time stands at the right end of that line, 13 px in `mute`, and reads "just now", "4 min ago", "2 h ago" or "3 days ago".
- The second line 13 px in `mute` holds the numbers the step wrote: two spots in inches for a move, two angles for a turn, two sizes in percent, the two pixel counts of a grid size, or the two pairs of screens of a switch.
- The newest step stands at the top. The last row reads "Before the first change" and carries no second line.
- The step the scene stands on takes the `raised` background, a 3 px `accent` bar on its left edge, and `accent` bold text.
- A step the DM took back draws in `mute`. It stays on the list, so a walk forward is one click.
- A click on a row takes the scene to the state after that step.
- The footer holds the helper "A click on a step takes the scene there".

## 10. Other overlays

- The color popover: a grid of 28 px swatches four across, then a hex input, then the helper "Click a swatch, or type a hex value".
- The tooltip: height 24 px, 8 px horizontal padding, the label 13 px and its key in `mute`, gap 8 px.
- A file over the window: a 2 px `accent` outline 8 px inside the canvas edge, an `accent` wash at 6 %, and a box at the center that names the scene the file goes into.
- The empty state: a box at the center with the title 16 px bold, a helper line, and the New scene and Open a scene buttons.
- `F3` shows a box on the right with the frame rate, the size of each window, the drawn and culled counts, the resident tiles, the draw calls and the state of the scene. Each row is 18 px, on one line: the name 13 px `mute` in a 120 px column, then the value 13 px `ink`.
