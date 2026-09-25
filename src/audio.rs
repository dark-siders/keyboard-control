// --- AUDIO-REACTIVE (журнал 10.97) ---
// Захват системного звука через PulseAudio/PipeWire монитор (@DEFAULT_MONITOR@)
// и расчёт спектр-полос VU для host-эффекта RgbMode::Audio.
//
// Канал: тот же `kbc daemon` (host-рендеринг direct, keepalive), только источник
// кадра — не таймер, а живой спектр звука. Работает без GUI (daemon/systemd).
// USB-only: у ресивера 2.4G/Ajazz флеш-эффекта «слушать звук» нет — там идёт
// обычный apply_rgb_persistent (или честный Unsupported), НЕ аудио.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use psimple::Simple;
use pulse::sample::{Format, Spec};
use pulse::stream::Direction;
use rustfft::num_complex::Complex;
use rustfft::{FftPlanner, num_traits::Zero};

use crate::presets::AudioStyle;
use crate::types::Color;

/// Длина FFT-окна (степень двойки). При 44.1 кГц это ~23 мс — вдвое быстрее
/// прежнего 2048 (~46 мс), поэтому визуализатор заметно «живее» (2026-09-24:
/// владелец просил добавить скорости — статика еле шевелилась).
const FFT_WINDOW: usize = 1024;
/// Частота дискретизации захвата монитора.
const SAMPLE_RATE: u32 = 44100;
/// Число каналов захвата (стерео; спектр считаем по среднему).
const CHANNELS: u8 = 2;
/// Количество VU-полос спектра (маппятся на колонки клавиатуры).
const VU_BANDS: usize = 16;
/// Порог абсолютной громкости блока (RMS, 0..1): ниже него считаем, что музыки
/// нет, и полосы гасим в ноль. Без этого шум/фон давали «эффект до музыки»
/// (нормировка по RMS раздувала тихий шум — жалоба владельца 2026-09-24).
const RMS_FLOOR: f32 = 0.002;
/// Порог активации полосы (уровень 0..1): ниже — LED активного ряда просто
/// светится статичным базовым цветом (как и буквы). Понижен 0.015→0.006:
/// при реальной музыке полосы редко доходили до порога, ряд «моргал»
/// (жалоба владельца 2026-09-24).
const ACTIVATION: f32 = 0.006;
/// Общая чувствительность полос по умолчанию (множитель после AGC). 1.0 — пик
/// кадра = 1.0; меньше — полосы приглушённее/менее реактивны. 0.75 выбрано
/// владельцем (2026-09-24) и сохранено как дефолт в пресетах.
pub const DEFAULT_SENSITIVITY: f32 = 0.75;

/// Спад между кадрами из «скорости» пресета (0..=255): выше скорость — выше
/// коэффициент (быстрее догоняет цель). Подобрано так, чтобы скорость 60
/// (дефолт пресета) давала прежний 0.6.
pub fn smoothing_for_speed(speed: u8) -> f32 {
    (0.3 + (speed as f32 / 255.0) * 1.275).clamp(0.2, 0.95)
}

/// Живой аудио-захват монитора + расчёт спектра.
pub struct AudioReactive {
    /// PulseAudio simple-поток записи с @DEFAULT_MONITOR@.
    stream: Simple,
    /// FFT-план (переиспользуемый).
    fft: Arc<dyn rustfft::Fft<f32>>,
    /// Временный ре-буфер сэмплов (моно, FFT_WINDOW).
    samples: Vec<f32>,
    /// Временный комплексный буфер FFT.
    spectrum: Vec<Complex<f32>>,
    /// Сглаживание полос между кадрами (0..1: 1 = мгновенно).
    /// Чуть <1 даёт «тянущийся» визуализатор без дрожи.
    smoothing: f32,
    /// Последние значения полос (для лёгкого attack/decay).
    last_bands: Vec<f32>,
    /// Чувствительность (множитель после AGC).
    sensitivity: f32,
}

impl AudioReactive {
    /// Открыть монитор системного звука с дефолтными параметрами (чувствительность
    /// 0.75, скорость 60 → сглаживание 0.6). Для кастомных значений — `open_with`.
    pub fn open() -> Result<Self> {
        Self::open_with(DEFAULT_SENSITIVITY, 60)
    }

    /// Открыть с заданными параметрами.
    pub fn open_with(sensitivity: f32, speed: u8) -> Result<Self> {
        let spec = Spec {
            format: Format::F32le,
            channels: CHANNELS,
            rate: SAMPLE_RATE,
        };
        if !spec.is_valid() {
            return Err(anyhow!("недопустимая sample spec для захвата звука"));
        }
        // Явный buffer_attr с маленьким fragsize: серверные дефолты capture
        // на pipewire-pulse могут давать ЛАГ до нескольких секунд (владелец
        // наблюдал 6-8 c после повторного старта музыки). Три параметра:
        //   fragsize  — сколько данных отдавать за раз (~12 мс) — свежее окно;
        //   maxlength — маленький клиентский буфер: старьё выбрасывается, а не
        //               накапливается (иначе после тишины читается «хвост»);
        //   minreq    — подсказка серверу не копить данные сверх фрагмента.
        let attr = pulse::def::BufferAttr {
            maxlength: 4096,          // ~2 фрагмента: старьё не копится
            tlength: u32::MAX,        // (вывод не используется)
            prebuf: u32::MAX,         // (не применимо к record)
            minreq: 1024,             // подсказка серверу
            fragsize: 2048,           // ~12 мс стерео f32 44.1 кГц
        };
        let stream = Simple::new(
            None,                     // default server (PulseAudio или pipewire-pulse)
            "keyboard-center-audio",  // имя приложения
            Direction::Record,        // запись
            Some("@DEFAULT_MONITOR@"), // монитор системного вывода
            "Audio-reactive VU для keyboard-center",
            &spec,
            None, // default channel map
            Some(&attr),
        )
        .map_err(|e| anyhow!("не удалось открыть монитор PulseAudio ({e}); в сессии нужен запущенный PulseAudio/PipeWire"))?;

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_WINDOW);
        Ok(Self {
            stream,
            fft: Arc::from(fft),
            samples: vec![0.0; FFT_WINDOW],
            spectrum: vec![Complex::zero(); FFT_WINDOW],
            smoothing: smoothing_for_speed(speed),
            last_bands: vec![0.0; VU_BANDS],
            sensitivity,
        })
    }

    /// Прочитать один блок с монитора (FFT_WINDOW сэмплов) → спектр-полосы
    /// 0..1 (нормированы по средней громкости блока). Возвращает окно,
    /// если аудио нет/тишина (полосы плавно опадают к нулю).
    pub fn read_vu_bands(&mut self) -> Result<Vec<f32>> {
        // Читаем стерео-блок как f32 (поток F32le). Simple::read хочет байты.
        let bytes = FFT_WINDOW * CHANNELS as usize * std::mem::size_of::<f32>();
        let mut buf = vec![0u8; bytes];
        self.stream
            .read(&mut buf)
            .map_err(|e| anyhow!("ошибка чтения монитора PulseAudio: {e}"))?;

        // Сводим стерео в моно + окно Ханна (подавление утечек спектра).
        for i in 0..FFT_WINDOW {
            let l = f32::from_le_bytes(buf[i * 2 * 4..i * 2 * 4 + 4].try_into().unwrap());
            let r = f32::from_le_bytes(buf[i * 2 * 4 + 4..i * 2 * 4 + 8].try_into().unwrap());
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_WINDOW as f32).cos());
            self.samples[i] = 0.5 * (l + r) * w;
        }
        // Расчёт уровня (RMS) блока для нормализации.
        let rms = (self.samples.iter().map(|s| s * s).sum::<f32>() / FFT_WINDOW as f32).sqrt();

        // FFT.
        self.spectrum
            .iter_mut()
            .zip(self.samples.iter())
            .for_each(|(c, s)| *c = Complex::new(*s, 0.0));
        self.fft.process(&mut self.spectrum);

        // Полосы: логарифмическая шкала частот 20 Гц .. ~20 кГц по 16 полос.
        let nyquist = SAMPLE_RATE as f32 / 2.0;
        let mut out = vec![0.0f32; VU_BANDS];
        // Порог тишины: без него нормировка по RMS «раздувала» фоновый шум и
        // эффект начинал играть ДО музыки (жалоба владельца 2026-09-24).
        let silent = rms < RMS_FLOOR;
        if !silent {
            // AGC (авто-усиление по кадру): считаем «сырые» магнитуды полос,
            // затем нормируем по самой громкой полосе. Нормировка по RMS
            // «залипала»: при малом делителе всё уходило в максимум, при
            // большом — всё ниже порога (жалоба владельца «чисто статика»).
            // Относительная шкала всегда даёт видимый разброс по спектру.
            let mut raw = [0.0f32; VU_BANDS];
            let mut max_mag = 1e-9f32;
            for band in 0..VU_BANDS {
                let f_lo = f_band_edge(band, VU_BANDS, nyquist);
                let f_hi = f_band_edge(band + 1, VU_BANDS, nyquist);
                let bin_lo =
                    ((f_lo / nyquist * (FFT_WINDOW / 2) as f32) as usize).min(FFT_WINDOW / 2 - 1);
                let bin_hi = ((f_hi / nyquist * (FFT_WINDOW / 2) as f32) as usize)
                    .min(FFT_WINDOW / 2)
                    .max(bin_lo + 1);
                let mut sum = 0.0f32;
                let mut n = 0;
                for b in bin_lo..bin_hi {
                    // Амплитуда комплексного бина.
                    sum += (self.spectrum[b].re * self.spectrum[b].re
                        + self.spectrum[b].im * self.spectrum[b].im)
                        .sqrt();
                    n += 1;
                }
                let mag = if n > 0 { sum / n as f32 } else { 0.0 };
                // Небольшой подъём высоких частот (у музыки ВЧ тише).
                let gain = 1.0 + (band as f32 / VU_BANDS as f32) * 1.0;
                let v = mag * gain;
                raw[band] = v;
                if v > max_mag {
                    max_mag = v;
                }
            }
            for band in 0..VU_BANDS {
                out[band] = (raw[band] / max_mag * self.sensitivity).min(1.0);
            }
        }

        // Сглаживание: подъём СРАЗУ (музыка уже играет — не ждём 6 с), спад
        // быстрее (0.45 → 0.6) — динамика «прыгает», а не виснет статикой.
        for (b, v) in out.iter_mut().enumerate() {
            let prev = self.last_bands[b];
            let k = if *v > prev { 1.0 } else { self.smoothing };
            *v = prev + (*v - prev) * k;
            *v = (*v).clamp(0.0, 1.0);
            self.last_bands[b] = *v;
        }
        Ok(out)
    }
}

/// Левая граница полосы спектра (лог-шкала): [40 Гц, nyquist].
/// 40 Гц (а не 20) — при окне 1024 бины по ~43 Гц, ниже 40 Гц полоса
/// упиралась бы в DC-бин и «залипала».
fn f_band_edge(band: usize, total: usize, nyquist: f32) -> f32 {
    const F_MIN: f32 = 40.0;
    let t = band as f32 / total as f32;
    F_MIN * (nyquist / F_MIN).powf(t)
}

/// Построить кадр host-рендеринга из VU-полос.
///
/// Два стиля:
/// - `Row`: реагирует ТОЛЬКО верхний ряд (Esc + F1..F12). Цвет — интерполяция
///   color_low → color_high по частоте. Остальные LED — статичный базовый цвет.
/// - `Bars`: cava-стиль. Каждая колонка матрицы = одна полоса спектра.
///   LED в колонке зажигаются снизу вверх (row 5 → row 0) пропорционально уровню.
///   Цвет — интерполяция color_low → color_high по колонке.
///
/// `geometry` — срез `LED index -> (row, col)` для устройства (из `LED_GEOMETRY`).
/// LED с `(0xFF, 0xFF)` пропускаются (индикаторы/лого).
/// Если geometry пуст или стиль Row — работает как раньше (верхний ряд).
pub fn audio_frame(
    base: Color,
    brightness: u8,
    vu: &[f32],
    led_count: usize,
    active_leds: &[usize],
    color_low: Color,
    color_high: Color,
    style: AudioStyle,
    geometry: &[(u8, u8)],
) -> Vec<Color> {
    let bright_k = brightness.max(1) as f32 / 255.0;
    // Статичный базовый цвет (буквы и «тихие» LED).
    let static_col = |k: f32| {
        Color::new(
            ((base.r as f32) * k).clamp(0.0, 255.0) as u8,
            ((base.g as f32) * k).clamp(0.0, 255.0) as u8,
            ((base.b as f32) * k).clamp(0.0, 255.0) as u8,
        )
    };
    // Интерполяция цвета низ/верх по позиции (linear в RGB).
    let interp = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        let r = ((color_low.r as f32) * (1.0 - t) + (color_high.r as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8;
        let g = ((color_low.g as f32) * (1.0 - t) + (color_high.g as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8;
        let b = ((color_low.b as f32) * (1.0 - t) + (color_high.b as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8;
        Color::new(r, g, b)
    };

    let has_geometry = geometry
        .iter()
        .any(|(row, col)| *row != 0xFF && *col != 0xFF);
    match style {
        AudioStyle::Bars if has_geometry => {
            audio_frame_bars(vu, led_count, geometry, bright_k, &interp, &static_col)
        }
        _ => audio_frame_row(vu, led_count, active_leds, bright_k, &interp, &static_col),
    }
}

/// Старый стиль: только верхний ряд реагирует.
fn audio_frame_row(
    vu: &[f32],
    led_count: usize,
    active_leds: &[usize],
    bright_k: f32,
    interp: &dyn Fn(f32) -> Color,
    static_col: &dyn Fn(f32) -> Color,
) -> Vec<Color> {
    let bands = vu.len().max(1);
    let n_active = active_leds.len().max(1);
    let mut out = Vec::with_capacity(led_count);
    for i in 0..led_count {
        let pos = active_leds.iter().position(|&x| x == i);
        if let Some(pos) = pos {
            let bi = (pos * bands / n_active).min(bands - 1);
            let level = vu[bi];
            if level >= ACTIVATION {
                let t = if bands > 1 { bi as f32 / (bands - 1) as f32 } else { 0.0 };
                let color = interp(t);
                let (r, g, b) = (color.r, color.g, color.b);
                let eff = level.sqrt();
                let k = (bright_k * (0.45 + 0.55 * eff)).clamp(0.0, 1.0);
                out.push(Color::new(
                    ((r as f32) * k).clamp(0.0, 255.0) as u8,
                    ((g as f32) * k).clamp(0.0, 255.0) as u8,
                    ((b as f32) * k).clamp(0.0, 255.0) as u8,
                ));
                continue;
            }
        }
        out.push(static_col(bright_k));
    }
    out
}

/// Новый стиль: cava-столбики по колонкам.
fn audio_frame_bars(
    vu: &[f32],
    led_count: usize,
    geometry: &[(u8, u8)],
    bright_k: f32,
    interp: &dyn Fn(f32) -> Color,
    static_col: &dyn Fn(f32) -> Color,
) -> Vec<Color> {
    let bands = vu.len().max(1);
    let mut out = vec![static_col(bright_k); led_count];

    // Группируем LED по колонкам: col -> Vec<(row, led_index)>
    let max_col = geometry
        .iter()
        .filter(|(r, c)| *r != 0xFF && *c != 0xFF)
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(0) as usize;
    let mut cols: Vec<Vec<(u8, usize)>> = vec![Vec::new(); max_col + 1];

    for (led_idx, &(row, col)) in geometry.iter().enumerate() {
        if row == 0xFF || col == 0xFF || led_idx >= led_count {
            continue; // неизвестная позиция или за пределами
        }
        if col as usize <= max_col {
            cols[col as usize].push((row, led_idx));
        }
    }

    // В каждой колонке сортируем LED снизу вверх (row 5 → 0 = низ → верх)
    for col_leds in &mut cols {
        col_leds.sort_by_key(|&(row, _)| std::cmp::Reverse(row as usize));
    }

    // Для каждой колонки берём соответствующую полосу спектра
    for (col_idx, col_leds) in cols.iter().enumerate() {
        if col_leds.is_empty() {
            continue;
        }
        let bi = (col_idx * bands / (max_col + 1)).min(bands - 1);
        let level = vu[bi].clamp(0.0, 1.0);
        if level < ACTIVATION {
            continue; // колонка остаётся базовой
        }
        // Цвет колонки = интерполяция по позиции колонки
        let t = if max_col > 0 { col_idx as f32 / max_col as f32 } else { 0.0 };
        let col_color = interp(t);
        // Зажигаем LED снизу вверх пропорционально level
        let n_leds = col_leds.len();
        let lit = ((level * n_leds as f32).ceil() as usize).min(n_leds);
        for (idx, &(_, led_idx)) in col_leds.iter().enumerate() {
            if idx < lit {
                let eff = (idx as f32 + 1.0) / n_leds as f32 * level;
                let k = (bright_k * (0.45 + 0.55 * eff.sqrt())).clamp(0.0, 1.0);
                out[led_idx] = Color::new(
                    ((col_color.r as f32) * k).clamp(0.0, 255.0) as u8,
                    ((col_color.g as f32) * k).clamp(0.0, 255.0) as u8,
                    ((col_color.b as f32) * k).clamp(0.0, 255.0) as u8,
                );
            }
        }
    }
    out
}

/// Расчёт полос из сэмплов без живой PulseAudio (для тестов): вход — иот
/// целый поток флоатов (стерео уже свёрнутый моно) + RMS. Чистая функция
/// спектра — unit-тест на искусственное синус-вход.
#[cfg(test)]
fn compute_bands_from_samples(samples: &[f32]) -> Vec<f32> {
    let mut s = vec![0.0f32; FFT_WINDOW];
    for (i, x) in samples.iter().take(FFT_WINDOW).enumerate() {
        let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_WINDOW as f32).cos());
        s[i] = *x * w;
    }
    let rms = (s.iter().map(|x| x * x).sum::<f32>() / FFT_WINDOW as f32).sqrt();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_WINDOW);
    let mut spectrum: Vec<Complex<f32>> = s.iter().map(|x| Complex::new(*x, 0.0)).collect();
    fft.process(&mut spectrum);
    let nyquist = SAMPLE_RATE as f32 / 2.0;
    let mut out = vec![0.0f32; VU_BANDS];
    if rms < RMS_FLOOR {
        return out;
    }
    let mut raw = [0.0f32; VU_BANDS];
    let mut max_mag = 1e-9f32;
    for band in 0..VU_BANDS {
        let f_lo = f_band_edge(band, VU_BANDS, nyquist);
        let f_hi = f_band_edge(band + 1, VU_BANDS, nyquist);
        let bin_lo = ((f_lo / nyquist * (FFT_WINDOW / 2) as f32) as usize).min(FFT_WINDOW / 2 - 1);
        let bin_hi = ((f_hi / nyquist * (FFT_WINDOW / 2) as f32) as usize)
            .min(FFT_WINDOW / 2)
            .max(bin_lo + 1);
        let mut sum = 0.0f32;
        let mut n = 0;
        for b in bin_lo..bin_hi {
            sum += (spectrum[b].re * spectrum[b].re + spectrum[b].im * spectrum[b].im).sqrt();
            n += 1;
        }
        let mag = if n > 0 { sum / n as f32 } else { 0.0 };
        let gain = 1.0 + (band as f32 / VU_BANDS as f32) * 1.0;
        let v = mag * gain;
        raw[band] = v;
        if v > max_mag {
            max_mag = v;
        }
    }
    for band in 0..VU_BANDS {
        out[band] = (raw[band] / max_mag * DEFAULT_SENSITIVITY).min(1.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f_band_edges_are_monotonic() {
        let nyq = SAMPLE_RATE as f32 / 2.0;
        let mut prev = 0.0;
        for b in 0..=VU_BANDS {
            let f = f_band_edge(b, VU_BANDS, nyq);
            assert!(f > prev, "полоса {b}: {f} должна расти");
            assert!(f <= nyq, "полоса {b} не должна превышать nyquist");
            prev = f;
        }
    }

    #[test]
    fn silence_gives_low_bands() {
        let v = compute_bands_from_samples(&[0.0; FFT_WINDOW]);
        assert_eq!(v.len(), VU_BANDS);
        // Тишина → спектр ~0 (нормировка rms≈0 защищена epsilon).
        for x in &v {
            assert!(*x <= 0.02, "тишина: полоса {x} должна быть ~0");
        }
    }

    #[test]
    fn low_freq_sine_boosts_low_bands() {
        // Синус 100 Гц: энергия в басовых полосах заметно выше высоких.
        let mut sig = vec![0.0f32; FFT_WINDOW];
        for i in 0..FFT_WINDOW {
            sig[i] =
                0.5 * (2.0 * std::f32::consts::PI * 100.0 * i as f32 / SAMPLE_RATE as f32).sin();
        }
        let v = compute_bands_from_samples(&sig);
        let bass: f32 = v[0..3].iter().sum();
        let treble: f32 = v[10..].iter().sum();
        assert!(
            bass > treble * 3.0,
            "бас должен доминировать: бас={bass} вч={treble}"
        );
    }

    #[test]
    fn audio_frame_shape() {
        let active = &[0usize, 12, 18, 24, 30, 36, 42, 48, 54, 60, 66, 72, 78];
        let vu = vec![0.0f32; VU_BANDS];
        let low = Color::new(51, 255, 51);
        let high = Color::new(51, 51, 255);
        let style = AudioStyle::Row;
        let geometry = &[];
        let frame = audio_frame(Color::new(255, 0, 0), 200, &vu, 90, active, low, high, style, geometry);
        assert_eq!(frame.len(), 90);
        let hi = vec![1.0f32; VU_BANDS];
        let frame_hi = audio_frame(Color::new(255, 0, 0), 200, &hi, 90, active, low, high, style, geometry);
        assert_eq!(frame_hi.len(), 90);
    }

    #[test]
    fn audio_frame_only_active_row_reacts() {
        // Полная громкость: LED верхнего ряда цветные (интерполяция low→high),
        // «буквы» — базовый цвет. При нулевых полосах весь кадр статичен (базовый цвет).
        let active = &[0usize, 12, 18, 24, 30, 36, 42, 48, 54, 60, 66, 72, 78];
        let base = Color::new(255, 255, 255);
        let low = Color::new(51, 255, 51);
        let high = Color::new(51, 51, 255);
        let style = AudioStyle::Row;
        let geometry = &[];
        let hi = vec![1.0f32; VU_BANDS];
        let frame = audio_frame(base, 255, &hi, 90, active, low, high, style, geometry);
        // Активный LED (слот 12 = F1) с полосой 1.0 — цветной (не белый базовый).
        assert_ne!(frame[12], base, "F1 должна реагировать цветом");
        // Неактивный LED (например слот 7 = «1») — статичный базовый цвет.
        assert_eq!(frame[7], base, "клавиша «1» должна быть статичной");
        // Тишина → ряд тоже базовый.
        let zero = vec![0.0f32; VU_BANDS];
        let frame0 = audio_frame(base, 255, &zero, 90, active, low, high, style, geometry);
        assert_eq!(frame0[12], base, "в тишине F1 статична");
        assert_eq!(frame0[0], base, "в тишине Esc статичен");
    }

    #[test]
    fn audio_frame_bars_reacts_by_column() {
        let geometry = [(5, 0), (4, 0), (0, 0), (5, 1)];
        let vu = vec![1.0f32; VU_BANDS];
        let base = Color::new(255, 255, 255);
        let low = Color::new(51, 255, 51);
        let high = Color::new(51, 51, 255);
        let frame = audio_frame(
            base,
            255,
            &vu,
            geometry.len(),
            &[],
            low,
            high,
            AudioStyle::Bars,
            &geometry,
        );
        assert!(frame.iter().all(|color| *color != base));
    }

    #[test]
    fn audio_frame_bars_without_geometry_falls_back_to_row() {
        let active = &[0usize, 1];
        let vu = vec![0.5f32; VU_BANDS];
        let base = Color::new(255, 255, 255);
        let low = Color::new(51, 255, 51);
        let high = Color::new(51, 51, 255);
        let row = audio_frame(
            base,
            255,
            &vu,
            2,
            active,
            low,
            high,
            AudioStyle::Row,
            &[],
        );
        let bars = audio_frame(
            base,
            255,
            &vu,
            2,
            active,
            low,
            high,
            AudioStyle::Bars,
            &[],
        );
        assert_eq!(bars, row);
    }
}
