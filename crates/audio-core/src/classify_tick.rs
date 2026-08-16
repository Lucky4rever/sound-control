use crate::AmplitudeBuffer;
use crate::SoundType;
use crate::settings::Settings;

pub fn classify_tick(amp_buffer: &AmplitudeBuffer, settings: &Settings) -> SoundType {
    let mean_amp = amp_buffer.mean();

    if mean_amp < 0.003 {
        return SoundType::None;
    }

    if amp_buffer.len() < 3 {
        return SoundType::None;
    }

    let corr = amp_buffer.correlation();
    let std_amp = amp_buffer.std();

    if std_amp < settings.runtime.noise_std_threshold_f32() && mean_amp > 0.003 {
        return SoundType::Noise;
    }

    if mean_amp < 0.01 && std_amp < 0.003 {
        return SoundType::None;
    }

    if corr > 0.5 {
        SoundType::Music
    } else if corr < 0.25 {
        SoundType::Voice
    } else {
        if std_amp < mean_amp * 0.25 {
            SoundType::Music
        } else {
            SoundType::Voice
        }
    }
}