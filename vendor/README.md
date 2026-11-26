# Vendored crates

`miniquad/` is miniquad 0.4.11 with five small fixes and a hook, marked "Snook Legend patch" in the
source (used through `[patch.crates-io]` in the workspace `Cargo.toml`):

- `src/graphics/gl.rs`: stream buffers are re-specified ("orphaned") on every
  update. Without it, Intel graphics drivers on Windows make the 2D interface
  flicker or disappear.
- `src/graphics/gl.rs`: a pipeline can test depth without writing it
  (upstream switched the test off whenever writes were off). The haze under
  the lamps and the halos round the bulbs use it: hidden by what is solid,
  they no longer cut squares out of each other. Pipelines that test nothing
  and write nothing behave as before.
- `src/native/windows.rs`: full screen uses the monitor the window is on and
  restores the window's previous place afterwards.
- `src/native/windows.rs`: a character beyond U+FFFF typed in two halves
  (a UTF-16 surrogate pair, as the emoji panel sends most emoji) reaches the
  game as one character; each half alone was dropped.
- `src/native/windows.rs`: the window opens on the monitor the pointer is on
  (where the game was started from), in the middle of it, not always on the
  primary monitor.
- `src/lib.rs`: the clock (`date::now`, behind macroquad's `get_time` and
  `get_frame_time`) can be held at a time and stepped by hand
  (`date::set_fixed`). Only the game's development recorder uses it
  (`crates/client/src/record.rs`): each frame of a video is exactly 1/60 s
  of the game, however long it took to draw.
