//! ROLE: Click sounds of the pointer effects: two short sounds made here, never recorded ones.
//! OWNER: Presentation plays them on a button press; built once, on the first click.
//! INVARIANTS: Playback is asynchronous and never fails visibly: a missing device stays silent.
use shotoshot_core::pointer::{ClickSound, Volume};
use std::sync::OnceLock;
use windows::{
    Win32::{Media::Audio::*, UI::Shell::FOLDERID_LocalAppData},
    core::PCWSTR,
};

const RATE: u32 = 44_100;

/// A mono 16-bit WAV file of `samples`.
fn wav(samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// The samples of `sound` at `gain`: a soft tick (a short falling tone) or a crisp click (a
/// noise snap over a ringing body). Both last tens of milliseconds: a sound shorter than the
/// time an idle audio device takes to wake up is swallowed whole, as the first 14 ms click was.
pub(super) fn samples(sound: ClickSound, gain: f32) -> Vec<i16> {
    let length_ms = match sound {
        ClickSound::Off => return Vec::new(),
        ClickSound::Soft => 40.,
        ClickSound::Click => 50.,
    };
    let count = (RATE as f32 * length_ms / 1000.) as usize;
    // A fixed scramble instead of a random generator: the click is the same every time.
    let mut noise = 0x2545_f491u32;
    let wave = |pitch: f32, t: f32| (std::f32::consts::TAU * pitch * t).sin();
    (0..count)
        .map(|index| {
            let t = index as f32 / RATE as f32;
            let progress = index as f32 / count as f32;
            let attack = (progress * 60.).min(1.);
            let value = match sound {
                ClickSound::Soft => {
                    (1. - progress).powi(3) * wave(1250. * (1. - 0.25 * progress), t)
                }
                _ => {
                    noise ^= noise << 13;
                    noise ^= noise >> 17;
                    noise ^= noise << 5;
                    let grain = (noise as f32 / u32::MAX as f32) * 2. - 1.;
                    let snap = 0.45 * grain * (-t / 0.0025).exp();
                    let body =
                        0.9 * (-t / 0.015).exp() * (0.7 * wave(2200., t) + 0.3 * wave(3400., t));
                    (snap + body) * (1. - progress)
                }
            };
            (value * attack * gain * i16::MAX as f32) as i16
        })
        .collect()
}

/// Built once per sound and volume; static so that asynchronous playback outlives the call.
fn built(sound: ClickSound, volume: Volume) -> &'static [u8] {
    static SOUNDS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    let all = SOUNDS.get_or_init(|| {
        let mut all = Vec::new();
        for sound in [ClickSound::Soft, ClickSound::Click] {
            for volume in Volume::ALL {
                all.push(wav(&samples(sound, volume.gain())));
            }
        }
        all
    });
    let row = if sound == ClickSound::Soft { 0 } else { 1 };
    &all[row * Volume::ALL.len() + volume.index()]
}

/// Plays the click sound. `click-custom.wav` in Shotoshot's local data folder replaces it, as
/// `screenshot-custom.wav` replaces the capture sound.
pub(super) fn play(sound: ClickSound, volume: Volume) {
    if sound == ClickSound::Off {
        return;
    }
    if let Ok(folder) = crate::settings::known_folder(&FOLDERID_LocalAppData) {
        let custom = folder.join("Shotoshot").join("click-custom.wav");
        if custom.is_file()
            && unsafe {
                PlaySoundW(
                    PCWSTR(crate::wide(&custom).as_ptr()),
                    None,
                    SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
                )
                .as_bool()
            }
        {
            return;
        }
    }
    let bytes = built(sound, volume);
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(bytes.as_ptr().cast()),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_short_audible_and_follow_the_volume() {
        for sound in [ClickSound::Soft, ClickSound::Click] {
            let quiet = samples(sound, Volume::Low.gain());
            let loud = samples(sound, Volume::High.gain());
            assert!(
                !quiet.is_empty() && quiet.len() < RATE as usize / 10,
                "{sound:?}"
            );
            let peak = |s: &[i16]| s.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
            assert!(peak(&quiet) > 1000, "{sound:?} audible");
            assert!(peak(&loud) > peak(&quiet) * 2, "{sound:?} louder");
            assert!(
                quiet.last().is_some_and(|v| v.unsigned_abs() < 200),
                "{sound:?} ends quietly"
            );
        }
        assert!(samples(ClickSound::Off, 1.).is_empty());
    }

    /// An idle device swallows the first milliseconds: a sound must outlast them, with as much
    /// left after 20 ms as the soft tick that is heard.
    #[test]
    fn sounds_survive_a_waking_audio_device() {
        let energy = |s: &[i16]| s.iter().map(|&v| (v as f64).powi(2)).sum::<f64>();
        let late = |sound| {
            let all = samples(sound, Volume::Medium.gain());
            assert!(
                all.len() >= RATE as usize * 30 / 1000,
                "{sound:?} lasts 30 ms"
            );
            energy(&all[RATE as usize * 20 / 1000..])
        };
        let soft = late(ClickSound::Soft);
        assert!(
            late(ClickSound::Click) >= soft,
            "click as present as soft after 20 ms"
        );
    }

    #[test]
    fn wav_header_is_well_formed() {
        let file = wav(&[1, -1]);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(file.len(), 48);
    }
}
