//! A reader for SF2 SoundFont banks.
//!
//! This parses the RIFF structure and the `pdta` tables (presets, instruments,
//! zones and sample headers) and can list what a bank contains. It does not
//! render samples yet, and modulators and filter generators are ignored.
#![allow(clippy::chunks_exact_to_as_chunks)]

/// One sample header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    /// Name from the header.
    pub name: String,
    /// First sample frame.
    pub start: u32,
    /// One past the last sample frame.
    pub end: u32,
    /// Loop start frame.
    pub loop_start: u32,
    /// Loop end frame.
    pub loop_end: u32,
    /// Sample rate in Hz.
    pub rate: u32,
    /// Root key.
    pub root: u8,
    /// Pitch correction in cents.
    pub correction: i8,
    /// 1 mono, 2 right, 4 left, 8 linked.
    pub kind: u16,
    /// Linked sample index.
    pub link: u16,
}

/// One instrument zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstrumentZone {
    /// Sample index, when this is not a global zone.
    pub sample: Option<usize>,
    /// Lowest key.
    pub key_low: u8,
    /// Highest key.
    pub key_high: u8,
    /// Lowest velocity.
    pub vel_low: u8,
    /// Highest velocity.
    pub vel_high: u8,
    /// Overriding root key.
    pub root_override: Option<u8>,
    /// Coarse tuning in semitones.
    pub coarse_tune: i16,
    /// Fine tuning in cents.
    pub fine_tune: i16,
    /// 1 means loop continuously.
    pub sample_modes: u16,
    /// Volume-envelope delay in timecents.
    pub delay: i16,
    /// Volume-envelope attack in timecents.
    pub attack: i16,
    /// Volume-envelope hold in timecents.
    pub hold: i16,
    /// Volume-envelope decay in timecents.
    pub decay: i16,
    /// Volume-envelope sustain attenuation in centibels.
    pub sustain: u16,
    /// Volume-envelope release in timecents.
    pub release: i16,
    /// Lowpass cutoff in absolute cents above 8.176 Hz.
    pub filter_cutoff: i16,
    /// Lowpass Q in centibels.
    pub filter_q: i16,
}

/// One instrument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instrument {
    /// Name from the header.
    pub name: String,
    /// Zones in order.
    pub zones: Vec<InstrumentZone>,
}

/// One preset zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetZone {
    /// Instrument index, when this is not a global zone.
    pub instrument: Option<usize>,
    /// Lowest key.
    pub key_low: u8,
    /// Highest key.
    pub key_high: u8,
    /// Lowest velocity.
    pub vel_low: u8,
    /// Highest velocity.
    pub vel_high: u8,
}

/// One preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    /// Name from the header.
    pub name: String,
    /// Bank number.
    pub bank: u16,
    /// Program number.
    pub program: u16,
    /// Zones in order.
    pub zones: Vec<PresetZone>,
}

/// A parsed bank.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundFont {
    /// Bank name from the RIFF `INAM` chunk, when present.
    pub name: String,
    /// Presets in file order.
    pub presets: Vec<Preset>,
    /// Instruments in file order.
    pub instruments: Vec<Instrument>,
    /// Samples in file order.
    pub samples: Vec<Sample>,
    /// Sample frames from the `smpl` chunk, 16-bit signed.
    pub pcm: Vec<i16>,
}

impl SoundFont {
    /// Finds a preset by bank and program.
    #[must_use]
    pub fn preset(&self, bank: u16, program: u16) -> Option<&Preset> {
        self.presets
            .iter()
            .find(|preset| preset.bank == bank && preset.program == program)
    }
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, String> {
    data.get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .ok_or_else(|| String::from("truncated table"))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .ok_or_else(|| String::from("truncated table"))
}

fn name_at(data: &[u8], offset: usize) -> String {
    let Some(rest) = data.get(offset..) else {
        return String::new();
    };
    let end = rest
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(rest.len())
        .min(20);
    String::from_utf8_lossy(&rest[..end]).into_owned()
}

type Chunk<'a> = ([u8; 4], &'a [u8]);

/// Iterates RIFF chunks: `(id, body)`.
fn chunks(data: &[u8]) -> Result<Vec<Chunk<'_>>, String> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset + 8 <= data.len() {
        let id: [u8; 4] = data[offset..offset + 4].try_into().unwrap_or(*b"    ");
        let size = u32_at(data, offset + 4)? as usize;
        let start = offset + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| format!("chunk {} runs past the file", String::from_utf8_lossy(&id)))?;
        out.push((id, &data[start..end]));
        offset = end + (size & 1);
    }
    Ok(out)
}

fn find<'a>(list: &[Chunk<'a>], id: &[u8; 4]) -> Option<&'a [u8]> {
    list.iter()
        .find(|(chunk, _)| chunk == id)
        .map(|(_, body)| *body)
}

fn bag_indices(table: &[u8]) -> Vec<usize> {
    table
        .chunks_exact(4)
        .map(|record| usize::from(u16_at(record, 0).unwrap_or(0)))
        .collect()
}

fn generators(table: &[u8]) -> Vec<(u16, u16)> {
    table
        .chunks_exact(4)
        .map(|record| {
            (
                u16_at(record, 0).unwrap_or(0),
                u16_at(record, 2).unwrap_or(0),
            )
        })
        .collect()
}

fn key_range(gens: &[(u16, u16)], op: u16) -> (u8, u8) {
    gens.iter()
        .find(|(kind, _)| *kind == op)
        .map(|(_, amount)| ((*amount & 0xff) as u8, (*amount >> 8) as u8))
        .unwrap_or((0, 127))
}

fn preset_zone(gens: &[(u16, u16)]) -> PresetZone {
    let instrument = gens
        .iter()
        .find(|(op, _)| *op == 41)
        .map(|(_, amount)| usize::from(*amount));
    let (key_low, key_high) = key_range(gens, 43);
    let (vel_low, vel_high) = key_range(gens, 44);
    PresetZone {
        instrument,
        key_low,
        key_high,
        vel_low,
        vel_high,
    }
}

fn timecents(gens: &[(u16, u16)], op: u16) -> i16 {
    gens.iter()
        .find(|(kind, _)| *kind == op)
        .map_or(-12_000, |(_, amount)| *amount as i16)
}

fn instrument_zone(gens: &[(u16, u16)]) -> InstrumentZone {
    let (key_low, key_high) = key_range(gens, 43);
    let (vel_low, vel_high) = key_range(gens, 44);
    InstrumentZone {
        sample: gens
            .iter()
            .find(|(op, _)| *op == 53)
            .map(|(_, amount)| usize::from(*amount)),
        key_low,
        key_high,
        vel_low,
        vel_high,
        root_override: gens
            .iter()
            .find(|(op, _)| *op == 58)
            .map(|(_, amount)| *amount as u8),
        coarse_tune: gens
            .iter()
            .find(|(op, _)| *op == 51)
            .map_or(0, |(_, amount)| *amount as i16),
        fine_tune: gens
            .iter()
            .find(|(op, _)| *op == 52)
            .map_or(0, |(_, amount)| *amount as i16),
        sample_modes: gens
            .iter()
            .find(|(op, _)| *op == 54)
            .map_or(0, |(_, amount)| *amount),
        delay: timecents(gens, 33),
        attack: timecents(gens, 34),
        hold: timecents(gens, 35),
        decay: timecents(gens, 36),
        sustain: gens
            .iter()
            .find(|(op, _)| *op == 37)
            .map_or(0, |(_, amount)| *amount),
        release: timecents(gens, 38),
        filter_cutoff: gens
            .iter()
            .find(|(op, _)| *op == 8)
            .map_or(13_500, |(_, amount)| *amount as i16),
        filter_q: gens
            .iter()
            .find(|(op, _)| *op == 9)
            .map_or(0, |(_, amount)| *amount as i16),
    }
}

fn zones<'a>(
    gens: &'a [(u16, u16)],
    bags: &[usize],
    first: usize,
    next: usize,
    decode: impl Fn(&'a [(u16, u16)]) -> InstrumentZone,
) -> Vec<InstrumentZone> {
    let mut out = Vec::new();
    for bag in first..next.min(bags.len().saturating_sub(1)) {
        let start = bags[bag].min(gens.len());
        let end = bags
            .get(bag + 1)
            .copied()
            .unwrap_or(gens.len())
            .min(gens.len());
        if end < start {
            continue;
        }
        out.push(decode(&gens[start..end]));
    }
    out
}

/// Parses an SF2 bank.
///
/// # Errors
///
/// Returns a message when the RIFF structure or a `pdta` table is malformed.
pub fn parse(bytes: &[u8]) -> Result<SoundFont, String> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"sfbk") {
        return Err(String::from("not an SF2 RIFF file"));
    }
    let size = u32_at(bytes, 4)? as usize;
    let end = (8 + size).min(bytes.len());
    if end < 12 {
        return Err(String::from("RIFF size is too short"));
    }
    let top = chunks(&bytes[12..end])?;
    let pdta = top
        .iter()
        .find(|(id, body)| id == b"LIST" && body.get(0..4) == Some(b"pdta"))
        .map(|(_, body)| chunks(&body[4..]))
        .transpose()?
        .ok_or_else(|| String::from("missing pdta list"))?;
    let name = top
        .iter()
        .find(|(id, body)| id == b"LIST" && body.get(0..4) == Some(b"INFO"))
        .and_then(|(_, body)| chunks(&body[4..]).ok())
        .and_then(|list| find(&list, b"INAM").map(|body| name_at(body, 0)))
        .unwrap_or_default();

    let phdr = find(&pdta, b"phdr").ok_or_else(|| String::from("missing phdr"))?;
    let pbag = find(&pdta, b"pbag").ok_or_else(|| String::from("missing pbag"))?;
    let pgen = find(&pdta, b"pgen").ok_or_else(|| String::from("missing pgen"))?;
    let inst = find(&pdta, b"inst").ok_or_else(|| String::from("missing inst"))?;
    let ibag = find(&pdta, b"ibag").ok_or_else(|| String::from("missing ibag"))?;
    let igen = find(&pdta, b"igen").ok_or_else(|| String::from("missing igen"))?;
    let shdr = find(&pdta, b"shdr").ok_or_else(|| String::from("missing shdr"))?;
    if phdr.len() % 38 != 0 || pbag.len() % 4 != 0 || pgen.len() % 4 != 0 {
        return Err(String::from("preset tables have a partial record"));
    }
    if inst.len() % 22 != 0 || ibag.len() % 4 != 0 || igen.len() % 4 != 0 || shdr.len() % 46 != 0 {
        return Err(String::from("instrument tables have a partial record"));
    }

    let pgen = generators(pgen);
    let igen = generators(igen);
    let pbags = bag_indices(pbag);
    let ibags = bag_indices(ibag);

    let samples: Vec<Sample> = shdr
        .chunks_exact(46)
        .map(|record| Sample {
            name: name_at(record, 0),
            start: u32_at(record, 20).unwrap_or(0),
            end: u32_at(record, 24).unwrap_or(0),
            loop_start: u32_at(record, 28).unwrap_or(0),
            loop_end: u32_at(record, 32).unwrap_or(0),
            rate: u32_at(record, 36).unwrap_or(0),
            root: *record.get(40).unwrap_or(&0),
            correction: *record.get(41).unwrap_or(&0) as i8,
            kind: u16_at(record, 44).unwrap_or(0),
            link: u16_at(record, 42).unwrap_or(0),
        })
        .filter(|sample| !sample.name.is_empty())
        .collect();

    let raw_instruments: Vec<(String, usize)> = inst
        .chunks_exact(22)
        .map(|record| {
            (
                name_at(record, 0),
                usize::from(u16_at(record, 20).unwrap_or(0)),
            )
        })
        .collect();
    let mut instruments = Vec::new();
    for (index, (name, bag)) in raw_instruments.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        let next = raw_instruments
            .get(index + 1)
            .map_or(ibags.last().copied().unwrap_or(0), |(_, bag)| *bag);
        let decoded = zones(&igen, &ibags, *bag, next, instrument_zone);
        instruments.push(Instrument {
            name: name.clone(),
            zones: decoded,
        });
    }

    let raw_presets: Vec<(String, u16, u16, usize)> = phdr
        .chunks_exact(38)
        .map(|record| {
            (
                name_at(record, 0),
                u16_at(record, 22).unwrap_or(0),
                u16_at(record, 20).unwrap_or(0),
                usize::from(u16_at(record, 24).unwrap_or(0)),
            )
        })
        .collect();
    let mut presets = Vec::new();
    for (index, (name, bank, program, bag)) in raw_presets.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        let next = raw_presets
            .get(index + 1)
            .map_or(pbags.last().copied().unwrap_or(0), |(_, _, _, bag)| *bag);
        let decoded = zones(&pgen, &pbags, *bag, next, |gens| {
            let zone = preset_zone(gens);
            InstrumentZone {
                sample: zone.instrument,
                key_low: zone.key_low,
                key_high: zone.key_high,
                vel_low: zone.vel_low,
                vel_high: zone.vel_high,
                root_override: None,
                coarse_tune: 0,
                fine_tune: 0,
                sample_modes: 0,
                delay: -12_000,
                attack: -12_000,
                hold: -12_000,
                decay: -12_000,
                sustain: 0,
                release: -12_000,
                filter_cutoff: 13_500,
                filter_q: 0,
            }
        });
        let zones: Vec<PresetZone> = decoded
            .into_iter()
            .map(|zone| PresetZone {
                instrument: zone.sample,
                key_low: zone.key_low,
                key_high: zone.key_high,
                vel_low: zone.vel_low,
                vel_high: zone.vel_high,
            })
            .collect();
        presets.push(Preset {
            name: name.clone(),
            bank: *bank,
            program: *program,
            zones,
        });
    }

    let pcm: Vec<i16> = top
        .iter()
        .find(|(id, body)| id == b"LIST" && body.get(0..4) == Some(b"sdta"))
        .and_then(|(_, body)| chunks(&body[4..]).ok())
        .and_then(|list| {
            find(&list, b"smpl").map(|body| {
                body.chunks_exact(2)
                    .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                    .collect()
            })
        })
        .unwrap_or_default();
    Ok(SoundFont {
        name,
        presets,
        instruments,
        samples,
        pcm,
    })
}

struct Lowpass {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Lowpass {
    fn new(cutoff_cents: i16, q_centibels: i16, rate: u32) -> Self {
        let nyquist = rate.max(1) as f32 / 2.0;
        let ceiling = (nyquist * 0.95).max(20.0);
        let cutoff = (8.176 * 2.0_f32.powf(f32::from(cutoff_cents) / 1200.0)).clamp(20.0, ceiling);
        let q = 10.0_f32
            .powf(f32::from(q_centibels) / 200.0)
            .clamp(0.5, 20.0);
        let w0 = std::f32::consts::TAU * cutoff / rate as f32;
        let cos = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }
}

fn timecents_to_seconds(value: i16) -> f32 {
    (2.0_f32.powf(f32::from(value) / 1200.0)).clamp(0.0, 30.0)
}

fn envelope(zone: &InstrumentZone, seconds: f32, held: f32) -> f32 {
    let delay = timecents_to_seconds(zone.delay);
    let attack = timecents_to_seconds(zone.attack);
    let hold = timecents_to_seconds(zone.hold);
    let decay = timecents_to_seconds(zone.decay);
    let release = timecents_to_seconds(zone.release);
    let sustain = 10.0_f32.powf(-f32::from(zone.sustain) / 200.0);
    let level = if seconds < delay {
        0.0
    } else if attack > 0.0 && seconds < delay + attack {
        (seconds - delay) / attack
    } else {
        let decay_start = delay + attack + hold;
        if seconds < decay_start {
            1.0
        } else if decay > 0.0 && seconds < decay_start + decay {
            1.0 - (1.0 - sustain) * ((seconds - decay_start) / decay)
        } else {
            sustain
        }
    };
    if release > 0.0 && seconds > held - release {
        level * ((held - seconds) / release).clamp(0.0, 1.0)
    } else {
        level
    }
}

fn resolve_zone(
    font: &SoundFont,
    bank: u16,
    program: u16,
    key: u8,
    velocity: u8,
) -> Option<(&InstrumentZone, usize)> {
    let preset = font.preset(bank, program)?;
    let preset_zone = preset.zones.iter().find(|zone| {
        zone.instrument.is_some()
            && key >= zone.key_low
            && key <= zone.key_high
            && velocity >= zone.vel_low
            && velocity <= zone.vel_high
    })?;
    let instrument = font.instruments.get(preset_zone.instrument?)?;
    let zone = instrument.zones.iter().find(|zone| {
        zone.sample.is_some()
            && key >= zone.key_low
            && key <= zone.key_high
            && velocity >= zone.vel_low
            && velocity <= zone.vel_high
    })?;
    Some((zone, zone.sample?))
}

fn render_sample(
    font: &SoundFont,
    zone: &InstrumentZone,
    sample: &Sample,
    key: u8,
    velocity: u8,
    seconds: f32,
    rate: u32,
) -> Option<Vec<f32>> {
    let pcm_len = font.pcm.len();
    let start = usize::try_from(sample.start).unwrap_or(0).min(pcm_len);
    let end = usize::try_from(sample.end).unwrap_or(0).min(pcm_len);
    if end <= start {
        return None;
    }
    let loop_start = usize::try_from(sample.loop_start).unwrap_or(0).min(pcm_len);
    let loop_end = usize::try_from(sample.loop_end).unwrap_or(0).min(pcm_len);
    let looped = zone.sample_modes & 1 == 1 && loop_end > loop_start + 1;
    let root = f32::from(zone.root_override.unwrap_or(sample.root));
    let cents = f32::from(zone.coarse_tune) * 100.0 + f32::from(zone.fine_tune);
    let semitones = f32::from(key) - root + cents / 100.0;
    let ratio = rate as f32;
    let sample_rate = sample.rate.max(1) as f32;
    let step = (sample_rate / ratio) * 2.0_f32.powf(semitones / 12.0);
    let frames = (seconds * ratio) as usize;
    let gain = f32::from(velocity.min(127)) / 127.0 * 0.6;
    let mut out = Vec::new();
    out.try_reserve(frames).ok()?;
    let mut filter = Lowpass::new(zone.filter_cutoff, zone.filter_q, rate);
    let mut position = start as f64;
    let loop_length = (loop_end.saturating_sub(loop_start)) as f64;
    // A short equal-power-ish crossfade around the loop seam hides a click when
    // the sample end and loop start do not match. It never spans more than half
    // the loop.
    let crossfade = if looped {
        (0.005 * f64::from(rate) * f64::from(step)).min(loop_length * 0.5)
    } else {
        0.0
    };
    let read = |position: f64| -> f32 {
        let base = position.floor() as usize;
        let fraction = (position - base as f64) as f32;
        let first = font.pcm.get(base).copied().unwrap_or(0) as f32 / 32_768.0;
        let next_index = if looped && base + 1 >= loop_end {
            loop_start
        } else {
            base + 1
        };
        let next = font.pcm.get(next_index).copied().unwrap_or(0) as f32 / 32_768.0;
        first + (next - first) * fraction
    };
    for _ in 0..frames {
        if looped {
            if position >= loop_end as f64 {
                position = loop_start as f64 + (position - loop_end as f64) % loop_length;
            }
        } else if position >= end as f64 {
            break;
        }
        let mut sample = read(position);
        if looped && crossfade > 0.0 && position >= loop_end as f64 - crossfade {
            let into = position - (loop_end as f64 - crossfade);
            let head = loop_start as f64 + into;
            let blend = (into / crossfade).clamp(0.0, 1.0) as f32;
            sample = sample * (1.0 - blend) + read(head) * blend;
        }
        let level = envelope(zone, out.len() as f32 / ratio, seconds);
        let filtered = filter.process(sample);
        out.push((filtered * gain * level).clamp(-1.0, 1.0));
        position += step as f64;
    }
    Some(out)
}

/// Renders one note, averaging a stereo pair to mono.
///
/// Samples are played with linear interpolation; loop mode 1 repeats the loop
/// region. Envelope and filter are applied.
#[must_use]
pub fn render_note(
    font: &SoundFont,
    bank: u16,
    program: u16,
    key: u8,
    velocity: u8,
    seconds: f32,
    rate: u32,
) -> Option<Vec<f32>> {
    if seconds <= 0.0 || rate == 0 {
        return None;
    }
    if let Some((left, right)) = render_stereo(font, bank, program, key, velocity, seconds, rate) {
        return Some(
            left.iter()
                .zip(right.iter())
                .map(|(left, right)| (left + right) * 0.5)
                .collect(),
        );
    }
    let (zone, sample_index) = resolve_zone(font, bank, program, key, velocity)?;
    let sample = font.samples.get(sample_index)?;
    render_sample(font, zone, sample, key, velocity, seconds, rate)
}

/// Renders one note as two channels.
///
/// Returns `None` for a mono sample; use [`render_note`] for those. A stereo
/// header links to its partner through `link` (2 right, 4 left).
#[must_use]
pub fn render_stereo(
    font: &SoundFont,
    bank: u16,
    program: u16,
    key: u8,
    velocity: u8,
    seconds: f32,
    rate: u32,
) -> Option<(Vec<f32>, Vec<f32>)> {
    if seconds <= 0.0 || rate == 0 {
        return None;
    }
    let (zone, index) = resolve_zone(font, bank, program, key, velocity)?;
    let sample = font.samples.get(index)?;
    let is_right = sample.kind & 2 != 0;
    let is_left = sample.kind & 4 != 0;
    if !is_right && !is_left {
        return None;
    }
    let link = usize::from(sample.link);
    let left_index = if is_left { index } else { link };
    let right_index = if is_right { index } else { link };
    if left_index == right_index {
        return None;
    }
    let left_sample = font.samples.get(left_index)?;
    let right_sample = font.samples.get(right_index)?;
    let left = render_sample(font, zone, left_sample, key, velocity, seconds, rate)?;
    let right = render_sample(font, zone, right_sample, key, velocity, seconds, rate)?;
    Some((left, right))
}

/// One-line summary for a CLI.
#[must_use]
pub fn describe(font: &SoundFont) -> String {
    format!(
        "{}: {} preset(s), {} instrument(s), {} sample(s)",
        if font.name.is_empty() {
            "SoundFont"
        } else {
            font.name.as_str()
        },
        font.presets.len(),
        font.instruments.len(),
        font.samples.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn list(kind: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut body = kind.to_vec();
        for chunk in chunks {
            body.extend_from_slice(chunk);
        }
        chunk(b"LIST", &body)
    }

    fn record_name(name: &str) -> [u8; 20] {
        let mut out = [0_u8; 20];
        for (slot, byte) in out.iter_mut().zip(name.bytes()) {
            *slot = byte;
        }
        out
    }

    #[test]
    fn malformed_names_and_short_riffs_do_not_panic() {
        assert_eq!(name_at(b"AB", 0), "AB");
        assert_eq!(name_at(b"A", 4), "");
        assert!(parse(b"RIFF").is_err());
        assert!(parse(b"RIFF\x02\x00\x00\x00WAVE").is_err());
        let _ = Lowpass::new(0, 0, 1);
    }

    fn build() -> Vec<u8> {
        let mut phdr = Vec::new();
        let mut preset = record_name("Test").to_vec();
        preset.extend_from_slice(&0_u16.to_le_bytes()); // preset
        preset.extend_from_slice(&0_u16.to_le_bytes()); // bank
        preset.extend_from_slice(&0_u16.to_le_bytes()); // bag
        preset.extend_from_slice(&0_u32.to_le_bytes());
        preset.extend_from_slice(&0_u32.to_le_bytes());
        preset.extend_from_slice(&0_u32.to_le_bytes());
        phdr.extend_from_slice(&preset);
        let mut term = [0_u8; 38];
        term[20..22].copy_from_slice(&0_u16.to_le_bytes());
        term[22..24].copy_from_slice(&0_u16.to_le_bytes());
        term[24..26].copy_from_slice(&1_u16.to_le_bytes());
        phdr.extend_from_slice(&term);

        let mut pbag = Vec::new();
        for index in [0_u16, 1] {
            pbag.extend_from_slice(&index.to_le_bytes());
            pbag.extend_from_slice(&0_u16.to_le_bytes());
        }
        let mut pgen = Vec::new();
        for (op, amount) in [(41_u16, 0_u16), (0, 0)] {
            pgen.extend_from_slice(&op.to_le_bytes());
            pgen.extend_from_slice(&amount.to_le_bytes());
        }

        let mut inst = Vec::new();
        let mut instrument = record_name("Piano").to_vec();
        instrument.extend_from_slice(&0_u16.to_le_bytes());
        inst.extend_from_slice(&instrument);
        let mut term = record_name("").to_vec();
        term.extend_from_slice(&1_u16.to_le_bytes());
        inst.extend_from_slice(&term);

        let mut ibag = Vec::new();
        for index in [0_u16, 7] {
            ibag.extend_from_slice(&index.to_le_bytes());
            ibag.extend_from_slice(&0_u16.to_le_bytes());
        }
        let mut igen = Vec::new();
        let immediate = (-24_000_i16) as u16;
        for (op, amount) in [
            (53_u16, 0_u16),
            (33, immediate),
            (34, immediate),
            (35, immediate),
            (36, immediate),
            (37, 0),
            (38, immediate),
            (0, 0),
        ] {
            igen.extend_from_slice(&op.to_le_bytes());
            igen.extend_from_slice(&amount.to_le_bytes());
        }

        let mut shdr = Vec::new();
        let mut sample = record_name("Sine").to_vec();
        for value in [0_u32, 64, 0, 64, 44_100] {
            sample.extend_from_slice(&value.to_le_bytes());
        }
        sample.push(60);
        sample.push(0);
        sample.extend_from_slice(&0_u16.to_le_bytes());
        sample.extend_from_slice(&1_u16.to_le_bytes());
        shdr.extend_from_slice(&sample);
        shdr.extend_from_slice(&[0_u8; 46]);

        let pdta = list(
            b"pdta",
            &[
                chunk(b"phdr", &phdr),
                chunk(b"pbag", &pbag),
                chunk(b"pgen", &pgen),
                chunk(b"inst", &inst),
                chunk(b"ibag", &ibag),
                chunk(b"igen", &igen),
                chunk(b"shdr", &shdr),
            ],
        );
        let mut pcm = Vec::new();
        for index in 0..64_i16 {
            pcm.extend_from_slice(&(index * 500).to_le_bytes());
        }
        let sdta = list(b"sdta", &[chunk(b"smpl", &pcm)]);
        let info = list(b"INFO", &[chunk(b"INAM", b"Test Bank\0")]);
        let mut body = b"sfbk".to_vec();
        body.extend_from_slice(&info);
        body.extend_from_slice(&pdta);
        body.extend_from_slice(&sdta);
        let mut riff = b"RIFF".to_vec();
        riff.extend_from_slice(&(body.len() as u32).to_le_bytes());
        riff.extend_from_slice(&body);
        riff
    }

    #[test]
    fn a_synthetic_bank_parses_to_one_preset_zone() {
        let font = parse(&build()).expect("parse");
        assert_eq!(font.name, "Test Bank");
        assert_eq!(font.presets.len(), 1);
        assert_eq!(font.presets[0].name, "Test");
        assert_eq!(font.presets[0].zones.len(), 1);
        assert_eq!(font.presets[0].zones[0].instrument, Some(0));
        assert_eq!(font.instruments.len(), 1);
        assert_eq!(font.instruments[0].zones[0].sample, Some(0));
        assert_eq!(font.samples.len(), 1);
        assert_eq!(font.samples[0].name, "Sine");
        assert_eq!(font.samples[0].rate, 44_100);
        assert_eq!(font.samples[0].root, 60);
        assert_eq!(font.pcm.len(), 64);
        let rendered = render_note(&font, 0, 0, 60, 127, 64.0 / 44_100.0, 44_100).expect("render");
        assert_eq!(rendered.len(), 64);
        assert!(rendered[0].abs() < 1.0e-6);
        assert!(
            rendered[32] > 0.2,
            "ramp reaches the middle: {}",
            rendered[32]
        );
        assert_eq!(font.samples[0].link, 0);
        assert!(render_note(&font, 0, 1, 60, 127, 0.01, 44_100).is_none());
        assert!(describe(&font).contains("1 preset"));
        assert_eq!(
            font.preset(0, 0).map(|preset| preset.name.as_str()),
            Some("Test")
        );
    }

    #[test]
    fn a_discontinuous_loop_is_crossfaded() {
        let mut font = parse(&build()).expect("parse");
        font.instruments[0].zones[0].sample_modes = 1;
        font.samples[0].loop_start = 0;
        font.samples[0].loop_end = 64;
        for (index, sample) in font.pcm.iter_mut().enumerate() {
            *sample = if index < 32 { 16_384 } else { -16_384 };
        }
        let output = render_note(&font, 0, 0, 60, 100, 200.0 / 44_100.0, 44_100).expect("loop");
        let largest = output
            .windows(2)
            .take(80)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            largest < 0.8,
            "loop seam should be softened, largest jump {largest}"
        );
    }

    #[test]
    fn loop_mode_and_root_override_change_the_length() {
        let mut font = parse(&build()).expect("parse");
        let short = render_note(&font, 0, 0, 60, 100, 200.0 / 44_100.0, 44_100).expect("short");
        assert_eq!(short.len(), 64, "a non-looped sample ends after its frames");
        font.instruments[0].zones[0].sample_modes = 1;
        font.samples[0].loop_start = 0;
        font.samples[0].loop_end = 64;
        let looped = render_note(&font, 0, 0, 60, 100, 200.0 / 44_100.0, 44_100).expect("loop");
        assert_eq!(looped.len(), 200, "loop mode fills the requested length");
        font.instruments[0].zones[0].root_override = Some(48);
        let higher = render_note(&font, 0, 0, 60, 100, 200.0 / 44_100.0, 44_100).expect("higher");
        assert_eq!(higher.len(), 200, "looping still fills it");
        font.instruments[0].zones[0].sample_modes = 0;
        let short_high =
            render_note(&font, 0, 0, 60, 100, 200.0 / 44_100.0, 44_100).expect("short high");
        assert!(
            short_high.len() < 40,
            "twelve semitones consume the sample sooner"
        );
    }

    #[test]
    fn the_volume_envelope_attacks_and_sustains() {
        let mut font = parse(&build()).expect("parse");
        font.pcm.fill(12_000);
        font.instruments[0].zones[0].sample_modes = 1;
        font.instruments[0].zones[0].attack = -1200; // 0.5 s
        let slow = render_note(&font, 0, 0, 60, 127, 1.0, 44_100).expect("attack");
        let early = slow[(44_100.0 * 0.1) as usize].abs();
        let later = slow[(44_100.0 * 0.7) as usize].abs();
        assert!(early < later * 0.6, "attack rises: {early} vs {later}");
        font.instruments[0].zones[0].attack = -12_000;
        font.instruments[0].zones[0].decay = -12_000; // 1 ms
        font.instruments[0].zones[0].sustain = 60; // about -6 dB
        let sustained = render_note(&font, 0, 0, 60, 127, 1.0, 44_100).expect("sustain");
        let full = render_note(
            &{
                let mut flat = font.clone();
                flat.instruments[0].zones[0].sustain = 0;
                flat
            },
            0,
            0,
            60,
            127,
            1.0,
            44_100,
        )
        .expect("flat");
        assert!(
            sustained[(44_100.0 * 0.5) as usize].abs()
                < full[(44_100.0 * 0.5) as usize].abs() * 0.7,
            "sustain attenuates"
        );
    }

    #[test]
    fn a_low_cutoff_removes_high_frequency_energy() {
        let mut font = parse(&build()).expect("parse");
        for (index, value) in font.pcm.iter_mut().enumerate() {
            *value = if index % 2 == 0 { 12_000 } else { -12_000 };
        }
        font.instruments[0].zones[0].sample_modes = 1;
        font.instruments[0].zones[0].attack = -12_000;
        font.instruments[0].zones[0].filter_cutoff = 13_500;
        let high = render_note(&font, 0, 0, 60, 127, 0.25, 44_100).expect("high");
        font.instruments[0].zones[0].filter_cutoff = 0;
        let low = render_note(&font, 0, 0, 60, 127, 0.25, 44_100).expect("low");
        let rms = |values: &[f32]| {
            (values.iter().map(|value| value * value).sum::<f32>() / values.len() as f32).sqrt()
        };
        assert!(
            rms(&low) < rms(&high) * 0.2,
            "low cutoff filters: {} vs {}",
            rms(&low),
            rms(&high)
        );
        assert!(!low.iter().any(|value| !value.is_finite()));
    }

    #[test]
    fn a_linked_stereo_pair_renders_two_channels() {
        let mut font = parse(&build()).expect("parse");
        font.pcm = (0..128)
            .map(|index| if index < 64 { 8_000 } else { -8_000 })
            .collect();
        font.samples[0].kind = 4;
        font.samples[0].link = 1;
        font.samples[0].start = 0;
        font.samples[0].end = 64;
        font.samples[0].loop_start = 0;
        font.samples[0].loop_end = 64;
        font.samples.push(Sample {
            name: String::from("Right"),
            start: 64,
            end: 128,
            loop_start: 64,
            loop_end: 128,
            rate: 44_100,
            root: 60,
            correction: 0,
            kind: 2,
            link: 0,
        });
        font.instruments[0].zones[0].sample_modes = 1;
        let (left, right) = render_stereo(&font, 0, 0, 60, 127, 0.1, 44_100).expect("stereo");
        assert_eq!(left.len(), right.len());
        assert!(left[100] > 0.0 && right[100] < 0.0, "channels differ");
        let mono = render_note(&font, 0, 0, 60, 127, 0.1, 44_100).expect("mono");
        assert!((mono[100] - (left[100] + right[100]) * 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn a_non_sf2_file_is_rejected() {
        assert!(parse(b"not a soundfont").is_err());
        let mut broken = build();
        broken.truncate(40);
        assert!(parse(&broken).is_err());
    }

    #[test]
    #[ignore = "needs ARRANGER_SF2 pointing at a real bank"]
    fn a_real_bank_parses() {
        let Ok(path) = std::env::var("ARRANGER_SF2") else {
            return;
        };
        let bytes = std::fs::read(path).expect("read");
        let font = parse(&bytes).expect("parse");
        assert!(!font.presets.is_empty());
        assert!(!font.samples.is_empty());
    }
}
