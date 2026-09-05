# dmap

dmap shows maps for a tabletop role-playing game on a TV.

- The program has one canvas with no limits. The canvas holds many maps.
- The game master controls the canvas in one window. A second window shows the players' view on the TV.
- The program uses the real size of the TV. One grid cell is one inch on the TV.
- The program has square grids, hex grids, tools to draw, and video maps.
- The program reads PNG, JPEG, Foundry VTT scenes and Universal VTT files.
- The GPU calculates the light. Walls and doors make shadows.
- The program runs on Linux. A new build runs on Windows. The code is Rust with wgpu, winit and egui.

Read [PLAN.md](PLAN.md) for the architecture and the milestones. Read [DESIGN.md](DESIGN.md) for the visual language of the DM window.
