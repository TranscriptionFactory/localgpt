//! FunDSP graph builders for each sound type + auto-inference rules.
//!
//! Each builder returns a mono FunDSP graph. The caller handles stereo panning
//! and volume control via `Shared` parameters.
//!
//! Note: fundsp `hacker` module uses f32 for node parameters. LFO closures
//! take and return f64. An<X> * f32 scales output; An<X> * lfo(...) modulates.

#![allow(clippy::precedence)]

use fundsp::hacker::*;

use super::commands::{AmbientSound, EmitterSound, FilterType, WaveformType};

// ---------------------------------------------------------------------------
// Ambient sound graph builders (mono output)
// ---------------------------------------------------------------------------

/// Build a mono FunDSP graph for an ambient sound type.
pub fn build_ambient_graph(sound: &AmbientSound) -> Box<dyn AudioUnit> {
    match sound {
        AmbientSound::Wind { speed, gustiness } => build_wind(*speed, *gustiness),
        AmbientSound::Rain { intensity } => build_rain(*intensity),
        AmbientSound::Forest { bird_density, wind } => build_forest(*bird_density, *wind),
        AmbientSound::Ocean { wave_size } => build_ocean(*wave_size),
        AmbientSound::Cave {
            drip_rate,
            resonance,
        } => build_cave(*drip_rate, *resonance),
        AmbientSound::Stream { flow_rate } => build_stream(*flow_rate),
        AmbientSound::Silence => Box::new(dc(0.0)),
    }
}

/// Wind: pink noise → lowpass with LFO-modulated cutoff.
fn build_wind(speed: f32, gustiness: f32) -> Box<dyn AudioUnit> {
    let base_cutoff = 200.0 + speed as f64 * 1000.0;
    let lfo_depth = gustiness as f64 * 400.0;
    Box::new(
        (pink() | lfo(move |t| (base_cutoff + lfo_depth * sin_hz(0.12, t)).max(100.0)))
            >> lowpole(),
    )
}

/// Rain: white noise → bandpass + amplitude modulation.
fn build_rain(intensity: f32) -> Box<dyn AudioUnit> {
    let cutoff = 2000.0 + intensity * 3000.0;
    let q = 0.5 + intensity * 0.5;
    let int_f64 = intensity as f64;
    Box::new(
        lfo(move |t| {
            let base = 0.3 + int_f64 * 0.7;
            base * (0.8 + 0.2 * sin_hz(0.15, t))
        }) * (white() >> bandpass_hz(cutoff, q)),
    )
}

/// Forest: quiet wind (pink noise) + random sine chirps (birds).
fn build_forest(bird_density: f32, wind_level: f32) -> Box<dyn AudioUnit> {
    let wind_vol = wind_level * 0.3;
    let bird_f64 = bird_density as f64 * 0.15;
    let mut net = Net::new(0, 1);

    let wind_layer = pink() * wind_vol;
    let id_wind = net.push(Box::new(wind_layer));

    let birds = lfo(move |t| {
        let chirp1 = (sin_hz(0.3, t) * 0.5 + 0.5).powf(8.0);
        let chirp2 = (sin_hz(0.17, t + 1.5) * 0.5 + 0.5).powf(10.0);
        bird_f64 * (chirp1 + chirp2)
    }) * (white() >> bandpass_hz(3000.0, 8.0));
    let id_birds = net.push(Box::new(birds));

    let sum = net.push(Box::new(pass() + pass()));
    net.set_source(sum, 0, Source::Local(id_wind, 0));
    net.set_source(sum, 1, Source::Local(id_birds, 0));
    net.set_output_source(0, Source::Local(sum, 0));

    Box::new(net)
}

/// Ocean: brown noise with slow amplitude LFO (waves) + foam hiss.
fn build_ocean(wave_size: f32) -> Box<dyn AudioUnit> {
    let wave_period = 4.0 + (1.0 - wave_size) as f64 * 6.0;
    let wave_depth = 0.3 + wave_size as f64 * 0.5;
    let mut net = Net::new(0, 1);

    let waves = lfo(move |t| {
        let wave = (sin_hz(1.0 / wave_period, t) * 0.5 + 0.5) * wave_depth + (1.0 - wave_depth);
        wave * 0.6
    }) * brown();
    let id1 = net.push(Box::new(waves));

    let foam = white() >> highpole_hz(4000.0) * 0.08_f32;
    let id2 = net.push(Box::new(foam));

    let sum = net.push(Box::new(pass() + pass()));
    net.set_source(sum, 0, Source::Local(id1, 0));
    net.set_source(sum, 1, Source::Local(id2, 0));
    net.set_output_source(0, Source::Local(sum, 0));

    Box::new(net)
}

/// Cave: occasional drip sounds + very quiet brown noise.
fn build_cave(drip_rate: f32, _resonance: f32) -> Box<dyn AudioUnit> {
    let drip_freq = 0.1 + drip_rate as f64 * 0.5;
    let mut net = Net::new(0, 1);

    let drips = lfo(move |t| {
        let drip = (sin_hz(drip_freq, t) * 0.5 + 0.5).powf(20.0);
        let drip2 = (sin_hz(drip_freq * 0.7, t + 0.8) * 0.5 + 0.5).powf(25.0);
        0.3 * (drip + drip2 * 0.6)
    }) * (white() >> bandpass_hz(2500.0, 12.0));
    let id_drips = net.push(Box::new(drips));

    let bg = brown() * 0.02_f32;
    let id_bg = net.push(Box::new(bg));

    let sum = net.push(Box::new(pass() + pass()));
    net.set_source(sum, 0, Source::Local(id_drips, 0));
    net.set_source(sum, 1, Source::Local(id_bg, 0));
    net.set_output_source(0, Source::Local(sum, 0));

    Box::new(net)
}

/// Stream: layered noise for flowing water.
fn build_stream(flow_rate: f32) -> Box<dyn AudioUnit> {
    let cutoff = 800.0 + flow_rate * 2000.0;
    let cutoff_hi = cutoff * 1.5;
    let mut net = Net::new(0, 1);

    let layer1 = (white() >> lowpole_hz(cutoff)) * 0.4_f32;
    let id1 = net.push(Box::new(layer1));

    let layer2 = (brown() >> lowpole_hz(600.0)) * 0.3_f32;
    let id2 = net.push(Box::new(layer2));

    let layer3 =
        lfo(move |t| 0.1 + 0.05 * sin_hz(0.2, t)) * (white() >> bandpass_hz(cutoff_hi, 2.0));
    let id3 = net.push(Box::new(layer3));

    let sum12 = net.push(Box::new(pass() + pass()));
    net.set_source(sum12, 0, Source::Local(id1, 0));
    net.set_source(sum12, 1, Source::Local(id2, 0));

    let sum123 = net.push(Box::new(pass() + pass()));
    net.set_source(sum123, 0, Source::Local(sum12, 0));
    net.set_source(sum123, 1, Source::Local(id3, 0));
    net.set_output_source(0, Source::Local(sum123, 0));

    Box::new(net)
}

// ---------------------------------------------------------------------------
// Emitter sound graph builders (mono output)
// ---------------------------------------------------------------------------

/// Build a mono FunDSP graph for an emitter sound type.
pub fn build_emitter_graph(sound: &EmitterSound) -> Box<dyn AudioUnit> {
    match sound {
        EmitterSound::Water { turbulence } => build_water(*turbulence),
        EmitterSound::Fire { intensity, crackle } => build_fire(*intensity, *crackle),
        EmitterSound::Hum { frequency, warmth } => build_hum(*frequency, *warmth),
        EmitterSound::Wind { pitch } => build_emitter_wind(*pitch),
        EmitterSound::Custom {
            waveform,
            filter_cutoff,
            filter_type,
        } => build_custom(*waveform, *filter_cutoff, *filter_type),
    }
}

/// Water: white noise → bandpass + brown noise undertone.
fn build_water(turbulence: f32) -> Box<dyn AudioUnit> {
    let cutoff = 1000.0 + turbulence * 2000.0;
    let q = 1.0 + turbulence * 2.0;
    let mut net = Net::new(0, 1);

    let water = lfo(move |t| 0.4 + 0.15 * sin_hz(0.25, t)) * (white() >> bandpass_hz(cutoff, q));
    let id_water = net.push(Box::new(water));

    let undertone = (brown() >> lowpole_hz(400.0)) * 0.15_f32;
    let id_under = net.push(Box::new(undertone));

    let sum = net.push(Box::new(pass() + pass()));
    net.set_source(sum, 0, Source::Local(id_water, 0));
    net.set_source(sum, 1, Source::Local(id_under, 0));
    net.set_output_source(0, Source::Local(sum, 0));

    Box::new(net)
}

/// Fire: brown noise (rumble) + noise bursts (crackle).
fn build_fire(intensity: f32, crackle: f32) -> Box<dyn AudioUnit> {
    let rumble_vol = 0.2 + intensity * 0.3;
    let crackle_f64 = crackle as f64 * 0.4;
    let mut net = Net::new(0, 1);

    let rumble = (brown() >> lowpole_hz(200.0)) * rumble_vol;
    let id_rumble = net.push(Box::new(rumble));

    let crackles = lfo(move |t| {
        let burst1 = (sin_hz(1.3, t) * 0.5 + 0.5).powf(12.0);
        let burst2 = (sin_hz(2.1, t + 0.3) * 0.5 + 0.5).powf(15.0);
        let burst3 = (sin_hz(0.7, t + 1.1) * 0.5 + 0.5).powf(10.0);
        crackle_f64 * (burst1 + burst2 * 0.7 + burst3 * 0.5)
    }) * (white() >> bandpass_hz(3000.0, 5.0));
    let id_crackle = net.push(Box::new(crackles));

    let sum = net.push(Box::new(pass() + pass()));
    net.set_source(sum, 0, Source::Local(id_rumble, 0));
    net.set_source(sum, 1, Source::Local(id_crackle, 0));
    net.set_output_source(0, Source::Local(sum, 0));

    Box::new(net)
}

/// Hum: sine wave + harmonics with slight detune for warmth.
fn build_hum(frequency: f32, warmth: f32) -> Box<dyn AudioUnit> {
    let f = frequency;
    let detune = 0.5 + warmth * 2.0;
    let mut net = Net::new(0, 1);

    let h1 = sine_hz(f) * 0.4_f32;
    let h2 = sine_hz(f * 2.0 + detune) * 0.2_f32;
    let h3 = sine_hz(f * 3.0 - detune * 0.5) * 0.1_f32;
    let h4 = sine_hz(f + detune * 0.3) * 0.15_f32;
    let id1 = net.push(Box::new(h1));
    let id2 = net.push(Box::new(h2));
    let id3 = net.push(Box::new(h3));
    let id4 = net.push(Box::new(h4));

    let s12 = net.push(Box::new(pass() + pass()));
    net.set_source(s12, 0, Source::Local(id1, 0));
    net.set_source(s12, 1, Source::Local(id2, 0));

    let s34 = net.push(Box::new(pass() + pass()));
    net.set_source(s34, 0, Source::Local(id3, 0));
    net.set_source(s34, 1, Source::Local(id4, 0));

    let s_all = net.push(Box::new(pass() + pass()));
    net.set_source(s_all, 0, Source::Local(s12, 0));
    net.set_source(s_all, 1, Source::Local(s34, 0));
    net.set_output_source(0, Source::Local(s_all, 0));

    Box::new(net)
}

/// Wind emitter: directional wind with pitch control.
fn build_emitter_wind(pitch: f32) -> Box<dyn AudioUnit> {
    let cutoff = pitch.max(100.0);
    Box::new(lfo(move |t| 0.5 + 0.2 * sin_hz(0.18, t)) * (pink() >> lowpole_hz(cutoff)))
}

/// Custom: direct waveform → filter → output.
fn build_custom(
    waveform: WaveformType,
    filter_cutoff: f32,
    filter_type: FilterType,
) -> Box<dyn AudioUnit> {
    let mut net = Net::new(0, 1);

    let source: Box<dyn AudioUnit> = match waveform {
        WaveformType::Sine => Box::new(sine_hz(filter_cutoff * 0.5)),
        WaveformType::Saw => Box::new(saw_hz(filter_cutoff * 0.25)),
        WaveformType::Square => Box::new(square_hz(filter_cutoff * 0.25)),
        WaveformType::WhiteNoise => Box::new(white()),
        WaveformType::PinkNoise => Box::new(pink()),
        WaveformType::BrownNoise => Box::new(brown()),
    };

    let filter: Box<dyn AudioUnit> = match filter_type {
        FilterType::Lowpass => Box::new(lowpole_hz(filter_cutoff)),
        FilterType::Highpass => Box::new(highpole_hz(filter_cutoff)),
        FilterType::Bandpass => Box::new(bandpass_hz(filter_cutoff, 1.0)),
    };

    let id_src = net.push(source);
    let id_filt = net.push(filter);
    net.pipe_all(id_src, id_filt);
    net.set_output_source(0, Source::Local(id_filt, 0));

    Box::new(net)
}

// ---------------------------------------------------------------------------
// Auto-inference rules
// ---------------------------------------------------------------------------

/// Keyword patterns → default audio emitter sound + radius.
pub fn infer_emitter_from_name(name: &str) -> Option<(EmitterSound, f32)> {
    let lower = name.to_lowercase();

    for (keywords, sound, radius) in AUDIO_INFERENCE_RULES {
        for keyword in *keywords {
            if lower.contains(keyword) {
                return Some((sound.clone(), *radius));
            }
        }
    }

    None
}

const AUDIO_INFERENCE_RULES: &[(&[&str], EmitterSound, f32)] = &[
    (
        &["waterfall", "fountain"],
        EmitterSound::Water { turbulence: 0.8 },
        15.0,
    ),
    (
        &["river", "water"],
        EmitterSound::Water { turbulence: 0.5 },
        12.0,
    ),
    (
        &["stream", "creek", "brook"],
        EmitterSound::Water { turbulence: 0.3 },
        10.0,
    ),
    (
        &["fire", "campfire", "torch", "flame", "bonfire"],
        EmitterSound::Fire {
            intensity: 0.5,
            crackle: 0.4,
        },
        10.0,
    ),
    (
        &["generator", "machine", "engine", "motor"],
        EmitterSound::Hum {
            frequency: 120.0,
            warmth: 0.5,
        },
        8.0,
    ),
    (
        &["vent", "fan", "wind_turbine"],
        EmitterSound::Wind { pitch: 400.0 },
        6.0,
    ),
];
