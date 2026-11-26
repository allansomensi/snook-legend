//! Synthesised sound effects (no audio files needed).

use std::f32::consts::TAU;

use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};

use crate::game::{ImpactKind, ImpactSound};

pub const RATE: u32 = 44_100;

/// Variants of each sound, so repeated hits never sound identical.
const VARIANTS: u32 = 4;

pub struct Sounds {
    cue: Vec<Voice>,
    ball: Vec<Voice>,
    cushion: Vec<Voice>,
    pocket: Vec<Voice>,
    tick: Vec<Voice>,
    chime: Vec<Voice>,
    click: Vec<Voice>,
    next: std::cell::Cell<u32>,
    /// The voices started lately: (when, how loud), for the mix's headroom.
    recent: std::cell::RefCell<std::collections::VecDeque<(f64, f32)>>,
}

/// A sound ready to play, with its samples while a video is recorded (the
/// recording mixes them itself, see record.rs; empty otherwise).
struct Voice {
    sound: Sound,
    pcm: Vec<f32>,
}

/// How loud the voices started within [`RECENT`] may add up to: a break
/// (the cue, a burst of clacks and cushions within a few frames) stays
/// clean instead of clipping, and a single hit is never turned down.
const HEADROOM: f32 = 1.5;
const RECENT: f64 = 0.09;
/// At most this many clacks and cushions start in one frame (the loudest).
const BALLS_PER_FRAME: usize = 4;
const CUSHIONS_PER_FRAME: usize = 3;

/// The volume setting (0 to 1) as a gain: the ear hears loudness on a
/// curve, so the low half of the slider is not all nearly as loud as the
/// top (the default stays about as loud as it was).
pub fn gain(setting: f32) -> f32 {
    setting.clamp(0.0, 1.0).powf(1.6)
}

/// A small, deterministic noise source.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// A two-pole filter (the "cookbook" biquad).
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn new(b: [f32; 3], a: [f32; 3]) -> Biquad {
        Biquad {
            b: [b[0] / a[0], b[1] / a[0], b[2] / a[0]],
            a: [a[1] / a[0], a[2] / a[0]],
            z: [0.0; 2],
        }
    }

    fn parts(f: f32, q: f32) -> (f32, f32) {
        let w = TAU * (f / RATE as f32).clamp(0.0005, 0.45);
        (w.cos(), w.sin() / (2.0 * q.max(0.1)))
    }

    /// A resonance: what rings at `f` when struck, the higher `q` the longer.
    fn bandpass(f: f32, q: f32) -> Biquad {
        let (c, alpha) = Self::parts(f, q);
        Biquad::new([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * c, 1.0 - alpha])
    }

    fn lowpass(f: f32) -> Biquad {
        let (c, alpha) = Self::parts(f, 0.707);
        Biquad::new(
            [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
            [1.0 + alpha, -2.0 * c, 1.0 - alpha],
        )
    }

    fn highpass(f: f32) -> Biquad {
        let (c, alpha) = Self::parts(f, 0.707);
        Biquad::new(
            [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0],
            [1.0 + alpha, -2.0 * c, 1.0 - alpha],
        )
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// What rings when a thing is struck: frequency (Hz), sharpness, level.
type Body = (f32, f32, f32);

/// How a kind of impact sounds.
struct Recipe {
    duration: f32,
    /// How long the two things touch on a firm hit (s): the shorter, the
    /// brighter the click. A soft hit touches longer.
    contact: f32,
    /// How loud the bare knock is beside what it sets ringing.
    knock: f32,
    bodies: &'static [Body],
    /// The hiss of the contact itself (cloth, leather, the air pushed
    /// out): level, how fast it dies (s) and its band (Hz).
    hiss: f32,
    hiss_decay: f32,
    hiss_band: (f32, f32),
    /// Nothing above this gets through on a firm hit (Hz); a soft hit is
    /// duller still.
    ceiling: f32,
    /// Nothing below this (Hz).
    floor: f32,
}

/// An impact, made the way the real one is: two things touch for a moment
/// (a smooth push, shorter and sharper the harder the hit), a little hiss
/// comes with it, and both go through what the things are made of
/// (resonances that are struck, so they never ring as pure tones), then
/// through a room. `hardness` (0 soft to 1 hard) is the hit's own: a hard
/// hit is not a soft one played louder, it is brighter.
fn impact(seed: u32, r: &Recipe, hardness: f32, variant: u32) -> Vec<f32> {
    let hardness = hardness.clamp(0.0, 1.0);
    let n = (r.duration * RATE as f32) as usize;
    let mut noise = Noise(seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453) | 1);
    let mut vary = Noise(0x9e37_79b9 ^ (variant * 7919 + 1));
    // No two hits alike: every resonance a little off.
    let mut bodies: Vec<(Biquad, f32)> = r
        .bodies
        .iter()
        .map(|&(f, q, level)| {
            let f = f * (1.0 + 0.06 * vary.next()) * (0.9 + 0.2 * hardness);
            (Biquad::bandpass(f, q * (1.0 + 0.2 * vary.next())), level)
        })
        .collect();
    let contact = r.contact * (1.9 - 1.1 * hardness) * (1.0 + 0.1 * vary.next());
    let contact_n = ((contact * RATE as f32) as usize).max(2);
    let mut hiss_low = Biquad::lowpass(r.hiss_band.1 * (0.55 + 0.45 * hardness));
    let mut hiss_high = Biquad::highpass(r.hiss_band.0);
    let mut ceiling = Biquad::lowpass(r.ceiling * (0.5 + 0.5 * hardness));
    let mut floor = Biquad::highpass(r.floor);
    let mut last_push = 0.0f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            // The push of the contact (a raised cosine); what is heard of
            // it is how fast it changes.
            let push = if i < contact_n {
                0.5 - 0.5 * (TAU * i as f32 / contact_n as f32).cos()
            } else {
                0.0
            };
            let knock = (push - last_push) * contact_n as f32 * 0.5;
            last_push = push;
            // The hiss swells with the contact and dies away after it.
            let swell = (i as f32 / contact_n as f32).min(1.0);
            let hiss = hiss_high.run(hiss_low.run(noise.next()))
                * swell
                * (-(t - contact).max(0.0) / r.hiss_decay).exp()
                * r.hiss
                * (0.6 + 0.4 * hardness);
            let struck = knock + hiss;
            let mut v = r.knock * knock + hiss;
            for (body, level) in &mut bodies {
                v += *level * body.run(struck);
            }
            floor.run(ceiling.run(v))
        })
        .collect();
    room(&mut out, seed);
    normalise(&mut out);
    out
}

/// The same peak for every sound (loudness comes from the volume it is
/// played at), with the end faded out.
fn normalise(out: &mut [f32]) {
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let fade_n = (0.004 * RATE as f32) as usize;
    let len = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        let fade = ((len - i) as f32 / fade_n as f32).min(1.0);
        *v = (*v / peak * 0.95 * fade).clamp(-1.0, 1.0);
    }
}

/// A pool hall round the table: a handful of early echoes off the walls
/// and the ceiling (each a little duller than the last) and a faint wash
/// after them. No feedback loops: those ring like a pipe.
fn room(x: &mut [f32], seed: u32) {
    let dry = x.to_vec();
    let energy = (dry.iter().map(|v| v * v).sum::<f32>() / dry.len().max(1) as f32).sqrt();
    let peak = dry.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let mut wet = vec![0.0f32; x.len()];
    let taps: [(f32, f32); 7] = [
        (5.3, 0.20),
        (8.9, -0.15),
        (13.7, 0.12),
        (19.1, -0.10),
        (25.9, 0.08),
        (34.3, 0.06),
        (45.1, -0.04),
    ];
    for (k, (ms, gain)) in taps.into_iter().enumerate() {
        let d = (ms / 1000.0 * RATE as f32) as usize;
        // Each bounce loses more of its top.
        let mut dull = Biquad::lowpass(7000.0 / (1.0 + 0.45 * k as f32));
        for i in d..x.len() {
            wet[i] += gain * dull.run(dry[i - d]);
        }
    }
    // The wash: dull noise dying away over a few tenths of a second, as
    // loud as the hit was.
    let mut noise = Noise(seed.wrapping_mul(2_654_435_761) | 1);
    let mut dull = Biquad::lowpass(2600.0);
    let start = (0.018 * RATE as f32) as usize;
    for (i, w) in wet.iter_mut().enumerate().skip(start) {
        let t = (i - start) as f32 / RATE as f32;
        *w += dull.run(noise.next()) * (-t / 0.07).exp() * (energy * 2.0 + peak * 0.05) * 0.5;
    }
    for (v, w) in x.iter_mut().zip(wet) {
        *v += w * 0.5;
    }
}

/// Two phenolic balls: a short, dry clack. Hard and nearly without a note:
/// the balls are solid and what they ring at dies in a millisecond.
const BALL: Recipe = Recipe {
    duration: 0.22,
    contact: 0.00022,
    knock: 0.9,
    bodies: &[(1750.0, 2.2, 0.55), (3300.0, 3.0, 0.8), (5200.0, 3.5, 0.6), (7900.0, 4.0, 0.3)],
    hiss: 0.25,
    hiss_decay: 0.0016,
    hiss_band: (1500.0, 9000.0),
    ceiling: 11_000.0,
    floor: 350.0,
};

/// The leather tip on the cue ball: a softer "tock", with the wood of the
/// cue under it.
const CUE: Recipe = Recipe {
    duration: 0.22,
    contact: 0.0009,
    knock: 0.7,
    bodies: &[(310.0, 2.0, 0.35), (820.0, 2.5, 0.8), (1500.0, 3.0, 0.6), (2600.0, 3.0, 0.3)],
    hiss: 0.18,
    hiss_decay: 0.004,
    hiss_band: (500.0, 4500.0),
    ceiling: 6000.0,
    floor: 120.0,
};

/// A ball into a rubber cushion backed by wood: a dull thump.
const CUSHION: Recipe = Recipe {
    duration: 0.3,
    contact: 0.0035,
    knock: 0.8,
    bodies: &[(95.0, 1.6, 1.0), (170.0, 2.0, 0.7), (310.0, 2.0, 0.35), (620.0, 2.0, 0.12)],
    hiss: 0.12,
    hiss_decay: 0.012,
    hiss_band: (120.0, 1400.0),
    ceiling: 1800.0,
    floor: 45.0,
};

/// A ball against the liner of a pocket: leather over wood.
const LINER: Recipe = Recipe {
    duration: 0.2,
    contact: 0.0022,
    knock: 0.7,
    bodies: &[(130.0, 1.8, 1.0), (260.0, 2.0, 0.6), (540.0, 2.2, 0.3), (1300.0, 2.5, 0.1)],
    hiss: 0.2,
    hiss_decay: 0.01,
    hiss_band: (150.0, 2200.0),
    ceiling: 2600.0,
    floor: 60.0,
};

/// A ball dropping into a pocket: it hits the liner, knocks about as it
/// settles and rolls down out of hearing.
fn pocket(seed: u32) -> Vec<f32> {
    let mut vary = Noise(seed.wrapping_mul(40_503) | 1);
    let len = (0.6 * RATE as f32) as usize;
    let mut out = vec![0.0f32; len];
    let mut at = 0.0f32;
    for (k, gain) in [1.0f32, 0.5, 0.3, 0.16].into_iter().enumerate() {
        let s = impact(seed * 13 + k as u32, &LINER, 0.75 - 0.18 * k as f32, seed + k as u32);
        let start = (at * RATE as f32) as usize;
        for (i, v) in s.iter().enumerate() {
            if start + i < len {
                out[start + i] += v * gain;
            }
        }
        // Each knock a little sooner after the last, never evenly.
        at += (0.085 - 0.014 * k as f32) * (1.0 + 0.25 * vary.next());
    }
    // The ball rolling away down the pocket: a low rumble that fades.
    let mut noise = Noise(seed.wrapping_mul(97) | 1);
    let mut low = Biquad::lowpass(260.0);
    let mut high = Biquad::highpass(70.0);
    let start = (0.06 * RATE as f32) as usize;
    for (i, v) in out.iter_mut().enumerate().skip(start) {
        let t = (i - start) as f32 / RATE as f32;
        let rise = (t / 0.05).min(1.0);
        *v += high.run(low.run(noise.next())) * rise * (-t / 0.16).exp() * 0.9;
    }
    normalise(&mut out);
    out
}

/// A clock's tick: a small, dry wooden knock, high and short.
fn tick() -> Vec<f32> {
    let len = (0.07 * RATE as f32) as usize;
    let mut noise = Noise(7_771);
    let mut air = Biquad::highpass(2500.0);
    let mut out: Vec<f32> = (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let tau = std::f32::consts::TAU;
            let body = (tau * 1650.0 * t).sin() * 0.7 + (tau * 2930.0 * t).sin() * 0.3;
            body * (-t / 0.011).exp() + air.run(noise.next()) * (-t / 0.003).exp() * 0.5
        })
        .collect();
    normalise(&mut out);
    out
}

/// The turn coming back: two soft notes rising, like a small bell (each
/// with the bell's own overtones, dying away).
fn chime() -> Vec<f32> {
    let len = (0.9 * RATE as f32) as usize;
    let mut out = vec![0.0f32; len];
    for (start, f, gain) in [(0.0f32, 784.0f32, 0.8f32), (0.12, 1175.0, 1.0)] {
        let from = (start * RATE as f32) as usize;
        for (i, v) in out.iter_mut().enumerate().skip(from) {
            let t = (i - from) as f32 / RATE as f32;
            let tau = std::f32::consts::TAU;
            let attack = (t / 0.004).min(1.0);
            let ring = (tau * f * t).sin()
                + 0.35 * (tau * f * 2.76 * t).sin() * (-t / 0.12).exp()
                + 0.18 * (tau * f * 5.4 * t).sin() * (-t / 0.05).exp();
            *v += gain * attack * ring * (-t / 0.32).exp();
        }
    }
    normalise(&mut out);
    out
}

/// A button pressed: a soft, short, low tap (felt more than heard, like
/// a key on a good keyboard), so the interface answers without chatter.
fn click() -> Vec<f32> {
    let len = (0.045 * RATE as f32) as usize;
    let mut noise = Noise(4_217);
    let mut air = Biquad::highpass(1800.0);
    let mut out: Vec<f32> = (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let tau = std::f32::consts::TAU;
            let attack = (t / 0.0008).min(1.0);
            let body = (tau * 620.0 * t).sin() * (-t / 0.009).exp()
                + 0.45 * (tau * 1340.0 * t).sin() * (-t / 0.004).exp();
            attack * (body + air.run(noise.next()) * (-t / 0.0015).exp() * 0.35)
        })
        .collect();
    normalise(&mut out);
    out
}

/// How hard each recorded version of a sound is hit (soft, firm, hard).
const HARDNESS: [f32; 3] = [0.15, 0.55, 1.0];

impl Sounds {
    /// Builds every sound. Missing audio hardware just means silence.
    pub async fn load() -> Self {
        async fn make(samples: Vec<f32>) -> Option<Voice> {
            let sound = load_sound_from_bytes(&wav(&samples)).await.ok()?;
            let pcm = if crate::record::hears() { samples } else { Vec::new() };
            Some(Voice { sound, pcm })
        }
        let mut sounds = Sounds {
            cue: Vec::new(),
            ball: Vec::new(),
            cushion: Vec::new(),
            pocket: Vec::new(),
            tick: Vec::new(),
            chime: Vec::new(),
            click: Vec::new(),
            next: std::cell::Cell::new(0),
            recent: Default::default(),
        };
        // Every kind at three strengths, each in a few versions (in that
        // order: `pick` relies on it).
        for (level, hardness) in HARDNESS.into_iter().enumerate() {
            for k in 0..VARIANTS {
                let seed = level as u32 * 101 + k;
                if let Some(s) = make(impact(11 + seed, &CUE, hardness, seed)).await {
                    sounds.cue.push(s);
                }
                if let Some(s) = make(impact(23 + seed, &BALL, hardness, seed)).await {
                    sounds.ball.push(s);
                }
                if let Some(s) = make(impact(37 + seed, &CUSHION, hardness, seed)).await {
                    sounds.cushion.push(s);
                }
            }
        }
        for k in 0..VARIANTS {
            if let Some(s) = make(pocket(41 + k)).await {
                sounds.pocket.push(s);
            }
        }
        if let Some(s) = make(tick()).await {
            sounds.tick.push(s);
        }
        if let Some(s) = make(chime()).await {
            sounds.chime.push(s);
        }
        if let Some(s) = make(click()).await {
            sounds.click.push(s);
        }
        sounds
    }

    /// One of the versions of a sound for a hit this strong (0 to 1).
    fn pick<'a>(&self, set: &'a [Voice], strength: f32) -> Option<&'a Voice> {
        let k = self.next.get();
        self.next.set(k.wrapping_add(1));
        let levels = HARDNESS.len();
        let per_level = set.len() / levels;
        if per_level == 0 || set.len() != per_level * levels {
            // Some could not be made, or a set with one strength only.
            return set.get(k as usize % set.len().max(1));
        }
        let level = if strength < 0.3 {
            0
        } else if strength < 0.68 {
            1
        } else {
            2
        };
        set.get(level * per_level + k as usize % per_level)
    }

    /// Plays (and clears) the queued impacts at `master` volume (the
    /// setting, 0..=1).
    pub fn play(&self, impacts: &mut Vec<ImpactSound>, master: f32) {
        if impacts.is_empty() {
            return;
        }
        let master = gain(master);
        // The loudest of each kind first; a crowd of clacks or cushions in
        // one frame is a few voices, not a wall of them (the cue and the
        // pockets all play).
        let mut queued: Vec<ImpactSound> = std::mem::take(impacts);
        queued.sort_by(|a, b| b.speed.total_cmp(&a.speed));
        let (mut balls, mut cushions) = (0, 0);
        queued.retain(|i| match i.kind {
            ImpactKind::Ball => {
                balls += 1;
                balls <= BALLS_PER_FRAME
            }
            ImpactKind::Cushion => {
                cushions += 1;
                cushions <= CUSHIONS_PER_FRAME
            }
            _ => true,
        });
        let now = macroquad::time::get_time();
        let mut recent = self.recent.borrow_mut();
        while recent.front().is_some_and(|(at, _)| now - at > RECENT) {
            recent.pop_front();
        }
        for impact in queued {
            // Loudness follows the impact speed, compressed like the ear.
            let (set, volume) = match impact.kind {
                ImpactKind::Cue => (&self.cue, (impact.speed / 6.0).sqrt()),
                ImpactKind::Ball => (&self.ball, (impact.speed / 3.5).sqrt()),
                ImpactKind::Cushion => (&self.cushion, (impact.speed / 3.0).sqrt() * 0.8),
                ImpactKind::Pocket => (&self.pocket, 0.75),
                ImpactKind::Tick => (&self.tick, 0.32),
                ImpactKind::Chime => (&self.chime, 0.42),
                ImpactKind::Click => (&self.click, 0.2),
            };
            let strength = volume.clamp(0.0, 1.0);
            let volume = strength * master.clamp(0.0, 1.0);
            if set.is_empty() || volume < 0.03 {
                continue;
            }
            let sound = match impact.kind {
                ImpactKind::Tick | ImpactKind::Chime | ImpactKind::Click => set.first(),
                ImpactKind::Pocket => {
                    let k = self.next.get();
                    self.next.set(k.wrapping_add(1));
                    set.get(k as usize % set.len())
                }
                _ => self.pick(set, strength),
            };
            if let Some(voice) = sound {
                // Within the headroom left by what is still ringing.
                let playing: f32 = recent.iter().map(|(_, v)| v).sum();
                let volume = volume.min((HEADROOM - playing).max(volume * 0.35));
                recent.push_back((now, volume));
                play_sound(&voice.sound, PlaySoundParams { looped: false, volume });
                crate::record::hear(&voice.pcm, volume);
            }
        }
    }
}

/// How loud a sample of 1 is in the WAV files (some room for peaks).
const WAV_GAIN: f32 = 0.9;

/// Encodes mono samples as a 16-bit PCM WAV file.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s * WAV_GAIN * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_follows_the_ear() {
        assert_eq!(gain(0.0), 0.0);
        assert_eq!(gain(1.0), 1.0);
        assert!(gain(0.5) < 0.4, "half the slider is well under half as loud");
        assert!((gain(0.8) - 0.8).abs() < 0.12, "the default about as loud as before");
        assert_eq!(gain(7.0), 1.0);
    }

    /// How much of a sound's energy is fast (a measure of brightness): the
    /// mean frequency of its zero crossings, in Hz.
    fn brightness(s: &[f32]) -> f32 {
        let head = &s[..(0.03 * RATE as f32) as usize];
        let crossings = head.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count();
        crossings as f32 / 2.0 / 0.03
    }

    /// When the sound has all but died away (s): the last moment it is
    /// above a hundredth of its peak.
    fn length(s: &[f32]) -> f32 {
        let last = s.iter().rposition(|v| v.abs() > 0.01).unwrap_or(0);
        last as f32 / RATE as f32
    }

    #[test]
    fn sounds_are_finite_and_normalised() {
        for s in [
            impact(1, &BALL, 1.0, 0),
            impact(1, &BALL, 0.1, 1),
            impact(2, &CUE, 0.5, 0),
            impact(3, &CUSHION, 0.5, 0),
            pocket(3),
        ] {
            assert!(s.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
            let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.5, "too quiet: {peak}");
        }
    }

    /// What tells the sounds apart: balls click high and short, the cue
    /// knocks lower, a cushion thumps; a hard hit is brighter than a soft
    /// one; versions differ.
    #[test]
    fn each_sound_is_what_it_should_be() {
        let ball = impact(1, &BALL, 1.0, 0);
        let soft_ball = impact(1, &BALL, 0.15, 0);
        let cue = impact(2, &CUE, 0.6, 0);
        let cushion = impact(3, &CUSHION, 0.6, 0);
        let (b, sb, c, cu) =
            (brightness(&ball), brightness(&soft_ball), brightness(&cue), brightness(&cushion));
        assert!(b > 2500.0, "a clack of balls is bright: {b} Hz");
        assert!(sb < b * 0.85, "a soft touch is duller: {sb} Hz against {b} Hz");
        assert!(c < b * 0.6 && c > 300.0, "the cue knocks lower: {c} Hz");
        assert!(cu < 500.0, "a cushion thumps: {cu} Hz");
        // Short: the knock itself is over in a few hundredths of a second
        // (the room carries on, quietly).
        assert!(length(&ball) < 0.2 && length(&cushion) < 0.3);
        // Most of a clack's energy is in its first hundredth of a second.
        let energy = |s: &[f32]| s.iter().map(|v| v * v).sum::<f32>();
        let head = (0.01 * RATE as f32) as usize;
        assert!(energy(&ball[..head]) > 0.8 * energy(&ball), "a dry clack");
        assert_ne!(impact(1, &BALL, 1.0, 0), impact(1, &BALL, 1.0, 1));
    }

    #[test]
    fn wav_header_is_valid() {
        let bytes = wav(&[0.0, 0.5, -0.5]);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + 6);
    }
}
