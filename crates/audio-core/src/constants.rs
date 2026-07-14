/// Час (в мілісекундах), після якого нижчий пріоритет
/// починає плавно відновлювати свою гучність до 100%.
pub const RECOVERY_MS: u128 = 1000;

/// Мінімальний рівень пікового індикатора (0–100),
/// щоб вважати додаток активно відтворюючим звук.
pub const ACTIVE_PEAK_THRESHOLD: i32 = 3;

/// Швидкість атаки ducking. Вище = швидше зниження.
pub const ENVELOPE_ATTACK: f32 = 0.70;

/// Швидкість відпускання ducking. Вище = швидше відновлення.
pub const ENVELOPE_RELEASE: f32 = 0.05;

/// Коефіцієнт пропорційності для ducking:
/// V_music = V_voice * peak_voice / peak_music * GAIN_COEFFICIENT
pub const GAIN_COEFFICIENT: f32 = 0.25;

/// Час мовчання (мс), після якого додаток прибирається зі списку.
pub const INACTIVITY_TIMEOUT_MS: u32 = 3000;

/// Порогове стандартне відхилення амплітуди, нижче якого
/// сигнал вважається стаціонарним шумом.
pub const NOISE_STD_THRESHOLD: f32 = 0.005;