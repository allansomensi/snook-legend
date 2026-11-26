//! Development aid: records the game into a video, frame by frame
//! (`SNOOK_LEGEND_RECORD=<file.mp4>`, see `tools/video.py`).
//!
//! Every frame moves the game's clock on by exactly 1/60 s, however long it
//! took to draw (Mesa's software renderer under Xvfb draws a few a second),
//! and its pixels go to `ffmpeg` through a pipe: a smooth 60 fps video of
//! what a player would see, without a picture per frame on the disk.
//!
//! - `SNOOK_LEGEND_RECORD_FRAMES=<n>`: how many frames (600 by default);
//!   then the game quits.
//! - `SNOOK_LEGEND_RECORD_FROM=<n>`: frames played (on the same clock)
//!   before the recording starts.
//!
//! The file is H.264 of a high quality (4:4:4, nearly lossless), meant to be
//! cut and encoded again; the game's sounds, mixed at the frames they were
//! played, go beside it in a WAV file of the same name. Release builds
//! ignore all of it.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use macroquad::miniquad::date;

const FPS: f64 = 60.0;

/// What the recording hears: the clock when its first frame was drawn and
/// the sounds mixed since (mono, at the rate of the game's sounds).
static HEARD: Mutex<Option<(f64, Vec<f32>)>> = Mutex::new(None);

/// Whether the sounds are recorded (a recording was asked for).
pub fn hears() -> bool {
    crate::dev::var("SNOOK_LEGEND_RECORD").is_some_and(|p| !p.is_empty())
}

/// A sound the game plays now, at `volume`: mixed into the recording's
/// sound when one is going on.
pub fn hear(samples: &[f32], volume: f32) {
    // (No samples are kept unless a recording was asked for.)
    if samples.is_empty() {
        return;
    }
    let mut heard = HEARD.lock().unwrap_or_else(|e| e.into_inner());
    let Some((start, mix)) = heard.as_mut() else {
        return;
    };
    let at = ((date::now() - *start) * crate::audio::RATE as f64).round();
    if at < 0.0 {
        return;
    }
    let at = at as usize;
    if mix.len() < at + samples.len() {
        mix.resize(at + samples.len(), 0.0);
    }
    for (out, s) in mix[at..].iter_mut().zip(samples) {
        *out += s * volume;
    }
}

pub struct Recorder {
    path: String,
    ffmpeg: Option<Child>,
    size: (u16, u16),
    from: u32,
    frames: u32,
    /// Frames drawn since the clock was taken over.
    drawn: u32,
    /// The clock when it was taken over.
    start: f64,
}

impl Recorder {
    /// A recorder if `SNOOK_LEGEND_RECORD` asks for one; from now on the
    /// clock only moves with [`Recorder::frame`].
    pub fn from_env() -> Option<Recorder> {
        let path = crate::dev::var("SNOOK_LEGEND_RECORD").filter(|p| !p.is_empty())?;
        let number =
            |name, default| crate::dev::var(name).and_then(|v| v.parse().ok()).unwrap_or(default);
        let start = date::now();
        date::set_fixed(Some(start));
        Some(Recorder {
            path,
            ffmpeg: None,
            size: (0, 0),
            from: number("SNOOK_LEGEND_RECORD_FROM", 0),
            frames: number("SNOOK_LEGEND_RECORD_FRAMES", 600).max(1),
            drawn: 0,
            start,
        })
    }

    /// Once a frame is drawn (just before `next_frame`): records it and
    /// moves the clock on. False when the recording is over (and the file
    /// written): time to quit.
    pub fn frame(&mut self) -> bool {
        self.drawn += 1;
        date::set_fixed(Some(self.start + self.drawn as f64 / FPS));
        if self.drawn <= self.from {
            return true;
        }
        let image = macroquad::texture::get_screen_data();
        let size = (image.width, image.height);
        if self.ffmpeg.is_none() {
            match ffmpeg(&self.path, size) {
                Ok(child) => {
                    self.ffmpeg = Some(child);
                    self.size = size;
                    // The sounds of this frame (played while it was drawn)
                    // start the recording's.
                    let first = self.start + (self.drawn - 1) as f64 / FPS;
                    *HEARD.lock().unwrap_or_else(|e| e.into_inner()) = Some((first, Vec::new()));
                }
                Err(e) => {
                    eprintln!("recording: ffmpeg did not start ({e})");
                    return false;
                }
            }
        }
        if size != self.size {
            eprintln!("recording: the window changed size, stopped");
            return self.finish();
        }
        let written = self
            .ffmpeg
            .as_mut()
            .and_then(|c| c.stdin.as_mut())
            .is_some_and(|pipe| pipe.write_all(&image.bytes).is_ok());
        if !written {
            eprintln!("recording: ffmpeg stopped");
            return self.finish();
        }
        if self.drawn - self.from >= self.frames {
            return self.finish();
        }
        true
    }

    /// Closes the pipe and waits for ffmpeg to write the file; writes the
    /// sound.
    fn finish(&mut self) -> bool {
        if let Some(mut child) = self.ffmpeg.take() {
            drop(child.stdin.take());
            let _ = child.wait();
        }
        if let Some((_, mut mix)) = HEARD.lock().unwrap_or_else(|e| e.into_inner()).take() {
            // As long as the video, and never clipped.
            let frames = self.drawn.saturating_sub(self.from) as f64;
            mix.resize((frames / FPS * crate::audio::RATE as f64) as usize, 0.0);
            let peak = mix.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            if peak > 1.0 {
                mix.iter_mut().for_each(|s| *s /= peak);
            }
            let path = std::path::Path::new(&self.path).with_extension("wav");
            if let Err(e) = std::fs::write(&path, crate::audio::wav(&mix)) {
                eprintln!("recording: the sound was not written ({e})");
            }
        }
        false
    }
}

/// ffmpeg reading raw frames (RGBA, bottom row first, as OpenGL reads them).
fn ffmpeg(path: &str, (w, h): (u16, u16)) -> std::io::Result<Child> {
    let size = format!("{w}x{h}");
    let rate = FPS.to_string();
    Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba"])
        .args(["-s", &size, "-r", &rate, "-i", "-", "-vf", "vflip"])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "10", "-pix_fmt", "yuv444p"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
}
