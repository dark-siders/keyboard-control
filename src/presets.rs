// --- Пресеты подсветки: сохранение параметров host-эффектов в TOML ---
// `kbc daemon` применяет пресет (Wave/цвет/яркость/скорость/период кадра),
// сохраняя состояние между запусками без GUI. Один пресет = один TOML-файл
// в `~/.config/keyboard-center/presets/`.

use crate::types::RgbMode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Имя пресета по умолчанию (создаётся при первом обращении `ensure`).
pub const DEFAULT_NAME: &str = "default";

/// Период кадра по умолчанию (мс). Host-эффекты рендерятся на хосте и
/// гонятся кадрами (keepalive ~40 мс ≈ 20-25 FPS, фикс 10.71).
pub const DEFAULT_FRAME_MS: u64 = 40;

/// Текущая версия схемы TOML-пресета.
pub const PRESET_FORMAT_VERSION: u32 = 1;

/// Дополнительные параметры аудио-реактива (`RgbMode::Audio`). Хранятся
/// отдельной секцией `[audio]` только у аудио-пресетов; у прочих режимов
/// секция отсутствует. Все поля имеют дефолты, сохраняющие текущий вид
/// визуализатора (журнал 10.97–10.99).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioParams {
    /// Общая чувствительность полос (множитель после AGC), 0.1..=2.0.
    #[serde(default = "default_audio_sensitivity")]
    pub sensitivity: f32,
    /// Цвет низких частот (спектр слева/низ), `#RRGGBB`. Дефолт — зелёный.
    #[serde(default = "default_audio_color_low")]
    pub color_low: String,
    /// Цвет высоких частот (спектр справа/верх), `#RRGGBB`. Дефолт — синий.
    #[serde(default = "default_audio_color_high")]
    pub color_high: String,
    /// Стиль визуализации: "row" (верхний ряд, как сейчас) или "bars" (cava-столбики по колонкам).
    #[serde(default = "default_audio_style")]
    pub style: AudioStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioStyle {
    Row,
    Bars,
}

fn default_audio_style() -> AudioStyle {
    AudioStyle::Row
}

fn default_audio_sensitivity() -> f32 {
    crate::audio::DEFAULT_SENSITIVITY
}

fn default_audio_color_low() -> String {
    // hsl(120, 1.0, 0.6) — прежний захардкоженный «зелёный» низ спектра.
    "#33FF33".to_string()
}

fn default_audio_color_high() -> String {
    // hsl(240, 1.0, 0.6) — прежний захардкоженный «синий» верх спектра.
    "#3333FF".to_string()
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            sensitivity: default_audio_sensitivity(),
            color_low: default_audio_color_low(),
            color_high: default_audio_color_high(),
            style: default_audio_style(),
        }
    }
}

/// Параметры host-эффекта, пригодные для сохранения в пресет.
///
/// Сериализуются в TOML как:
/// ```toml
/// version = 1            # формат файла (для миграций)
/// type = "wave"          # snake_case из types::RgbMode
/// color = "#FF5A00"
/// brightness = 200
/// speed = 60
/// frame_ms = 40
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeParams {
    /// Версия формата пресета. Default 1: файлы без поля `version`
    /// (формат 0.x) читаются как v1 (fallback).
    #[serde(default = "default_format_version")]
    pub version: u32,
    /// Режим движка (types::RgbMode, serde snake_case).
    #[serde(rename = "type")]
    pub r#type: RgbMode,
    /// Базовый цвет в виде `#RRGGBB` (для Static/Breathing/Custom).
    pub color: String,
    /// Яркость 0..=255.
    pub brightness: u8,
    /// Скорость 1..=255 (движок нормализует фазу, фикс 10.22).
    pub speed: u8,
    /// Период кадра host-рендеринга в мс (keepalive).
    pub frame_ms: u64,
    /// Настройки аудио-реактива: только для `type = "audio"` (у прочих
    /// режимов секции `[audio]` в файле нет).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioParams>,
}

fn default_format_version() -> u32 {
    PRESET_FORMAT_VERSION
}

impl ModeParams {
    /// Проверить, что формат файла поддерживается текущей версией.
    /// Более новая схема (version выше текущей) — ошибка с подсказкой.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version > PRESET_FORMAT_VERSION {
            return Err(anyhow::anyhow!(
                "пресет создан в более новой схеме (v{}), приложение понимает только v{}. \
                 Обновите keyboard-center",
                self.version,
                PRESET_FORMAT_VERSION
            ));
        }
        Ok(())
    }

    /// Параметры аудио-реактива пресета (или дефолты, если секции `[audio]`
    /// нет — обратная совместимость со старыми пресетами).
    pub fn audio_params(&self) -> AudioParams {
        self.audio.clone().unwrap_or_default()
    }
}

impl Default for ModeParams {
    fn default() -> Self {
        Self {
            version: default_format_version(),
            r#type: RgbMode::Wave,
            color: "#FF5A00".to_string(),
            brightness: 200,
            speed: 60,
            frame_ms: DEFAULT_FRAME_MS,
            audio: None,
        }
    }
}

/// Полный пресет: имя (привязывается к имени файла) + параметры.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    pub params: ModeParams,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            name: DEFAULT_NAME.to_string(),
            params: ModeParams::default(),
        }
    }
}

/// Встроенные («зашитые») пресеты: доступны у ЛЮБОГО пользователя без файлов
/// в `~/.config` — видны в списке GUI/CLI и грузятся из бинарника. Файл с тем
/// же именем перекрывает встроенный (пользователь может переопределить).
pub fn builtin_presets() -> Vec<Preset> {
    vec![
        Preset {
            name: "audio-viz".to_string(),
            params: ModeParams {
                version: PRESET_FORMAT_VERSION,
                r#type: RgbMode::Audio,
                color: "#FF5A00".to_string(),
                brightness: 255,
                speed: 60,
                frame_ms: DEFAULT_FRAME_MS,
                audio: Some(AudioParams::default()),
            },
        },
        Preset {
            name: "cava".to_string(),
            params: ModeParams {
                version: PRESET_FORMAT_VERSION,
                r#type: RgbMode::Audio,
                color: "#FF5A00".to_string(),
                brightness: 255,
                speed: 60,
                frame_ms: DEFAULT_FRAME_MS,
                audio: Some(AudioParams {
                    style: AudioStyle::Bars,
                    ..AudioParams::default()
                }),
            },
        },
    ]
}

/// Найти встроенный пресет по имени.
pub fn builtin(name: &str) -> Option<Preset> {
    let key = sanitize(name);
    builtin_presets().into_iter().find(|p| p.name == key)
}

/// Является ли имя встроенным пресетом.
pub fn is_builtin(name: &str) -> bool {
    builtin(name).is_some()
}

/// Хранилище пресетов: каталог с TOML-файлами (один пресет = один файл).
/// Зеркало `profiles::ProfileStore` (структура/методы один-в-один).
pub struct PresetStore {
    dir: PathBuf,
}

impl PresetStore {
    /// Каталог пресетов: `$XDG_CONFIG_HOME/keyboard-center/presets`
    /// либо `$HOME/.config/keyboard-center/presets`.
    pub fn default_dir() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."));
            home.join(".config")
        });
        base.join("keyboard-center").join("presets")
    }

    /// Создать хранилище по каталогу (каталог создаётся при необходимости).
    pub fn new(dir: PathBuf) -> Self {
        if !dir.is_dir() {
            let _ = std::fs::create_dir_all(&dir);
        }
        Self { dir }
    }

    /// Открыть хранилище в каталоге по умолчанию.
    pub fn open() -> Self {
        Self::new(Self::default_dir())
    }

    /// Каталог хранилища.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Путь к TOML-файлу пресета (имя очищается `sanitize`).
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.toml", sanitize(name)))
    }

    /// Существует ли пресет с таким именем (файл или встроенный).
    pub fn has(&self, name: &str) -> bool {
        self.path(name).is_file() || is_builtin(name)
    }

    /// Имена всех пресетов: файлы из `*.toml` + встроенные, без дублей,
    /// отсортированы. Файл с именем встроенного перекрывает встроенный.
    pub fn list(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("toml") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        names.push(stem.to_string());
                    }
                }
            }
        }
        for p in builtin_presets() {
            if !names.iter().any(|n| n == &p.name) {
                names.push(p.name);
            }
        }
        names.sort();
        names
    }

    /// Загрузить пресет по имени: файл имеет приоритет, иначе встроенный.
    pub fn load(&self, name: &str) -> anyhow::Result<Preset> {
        let p = self.path(name);
        if p.is_file() {
            let raw = std::fs::read_to_string(&p).map_err(|e| {
                anyhow::anyhow!("не удалось прочитать пресет {}: {e}", p.display())
            })?;
            let params: ModeParams = toml::from_str(&raw)
                .map_err(|e| anyhow::anyhow!("не удалось разобрать пресет {}: {e}", p.display()))?;
            params.validate()?;
            return Ok(Preset {
                name: sanitize(name),
                params,
            });
        }
        if let Some(p) = builtin(name) {
            return Ok(p);
        }
        Err(anyhow::anyhow!(
            "пресет «{}» не найден ({}) — создайте его: kbc preset save {}",
            name,
            p.display(),
            name
        ))
    }

    /// Сохранить пресет в TOML-файл. Возвращает путь к файлу.
    pub fn save(&self, preset: &Preset) -> anyhow::Result<PathBuf> {
        let p = self.path(&preset.name);
        let raw = toml::to_string_pretty(&preset.params)
            .map_err(|e| anyhow::anyhow!("не удалось сериализовать пресет: {e}"))?;
        std::fs::write(&p, raw)
            .map_err(|e| anyhow::anyhow!("не удалось записать пресет {}: {e}", p.display()))?;
        Ok(p)
    }

    /// Удалить пресет. Встроенный без файла-переопределения удалить нельзя;
    /// файл-переопределение встроенного удаляется (возвращается встроенный).
    pub fn remove(&self, name: &str) -> anyhow::Result<()> {
        let p = self.path(name);
        if !p.is_file() {
            if is_builtin(name) {
                return Err(anyhow::anyhow!(
                    "пресет «{}» встроенный — удалить нельзя (можно переопределить, сохранив свой под тем же именем)",
                    sanitize(name)
                ));
            }
            return Err(anyhow::anyhow!(
                "пресет «{}» не найден ({})",
                name,
                p.display()
            ));
        }
        std::fs::remove_file(&p)
            .map_err(|e| anyhow::anyhow!("не удалось удалить пресет {}: {e}", p.display()))?;
        Ok(())
    }

    /// Гарантировать пресет: файл/встроенный/создать «default».
    pub fn ensure(&self, name: &str) -> anyhow::Result<Preset> {
        if self.has(name) {
            return self.load(name);
        }
        if name == DEFAULT_NAME {
            let p = Preset::default();
            self.save(&p)?;
            return Ok(p);
        }
        Err(anyhow::anyhow!(
            "пресет «{name}» не найден ({}) — создайте его: kbc preset save {name}",
            self.path(name).display()
        ))
    }
}

/// Очистить имя пресета для имени файла: оставить A-Za-z0-9_-,
/// остальное заменить на `_`, обрезать `_`, пусто → «default».
pub fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() || !s.chars().any(|c| c.is_ascii_alphanumeric()) {
        DEFAULT_NAME.to_string()
    } else {
        s
    }
}

/// Человекочитаемое имя режима (для вывода в `kbc preset list`).
pub fn mode_name(m: RgbMode) -> &'static str {
    match m {
        RgbMode::Static => "статичный",
        RgbMode::Breathing => "дыхание",
        RgbMode::Wave => "волна",
        RgbMode::SpectrumCycle => "спектр",
        RgbMode::Ripple => "рябь",
        RgbMode::Rain => "дождь",
        RgbMode::Reactive => "реактивная",
        RgbMode::Custom => "custom",
        RgbMode::Audio => "аудио-реактив",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_alnum_dash_underscore() {
        assert_eq!(sanitize("My Wave 2"), "My_Wave_2".to_string());
        assert_eq!(sanitize("__ -_"), "default".to_string());
        assert_eq!(sanitize("дыхание"), "default".to_string());
        assert_eq!(sanitize("wave"), "wave".to_string());
    }

    #[test]
    fn preset_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kbc-presets-test-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        let p = Preset {
            name: "wave-turbo".to_string(),
            params: ModeParams {
                version: PRESET_FORMAT_VERSION,
                r#type: RgbMode::Wave,
                color: "#00FF00".to_string(),
                brightness: 255,
                speed: 90,
                frame_ms: 30,
                audio: None,
            },
        };
        let path = store.save(&p).unwrap();
        assert!(path.is_file());

        let loaded = store.load("wave-turbo").unwrap();
        assert_eq!(loaded, p);
        assert!(store.has("wave-turbo"));
        assert!(store.list().contains(&"wave-turbo".to_string()));

        store.remove("wave-turbo").unwrap();
        assert!(!store.has("wave-turbo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_creates_default() {
        let dir = std::env::temp_dir().join(format!("kbc-presets-test2-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        let p = store.ensure("default").unwrap();
        assert_eq!(p, Preset::default());
        assert!(store.has("default"));
        let err = store.ensure("missing-preset").unwrap_err();
        assert!(err.to_string().contains("не найден"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_old_preset_without_version_falls_back_to_v1() {
        // Старый файл формата 0.x — без поля `version`: должен читаться как v1.
        let dir = std::env::temp_dir().join(format!("kbc-presets-test3-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        let p = store.path("legacy");
        std::fs::write(
            &p,
            r##"type = "static"
color = "#FF0000"
brightness = 100
speed = 30
frame_ms = 40
"##,
        )
        .unwrap();

        let loaded = store.load("legacy").unwrap();
        assert_eq!(loaded.params.version, 1);
        assert_eq!(loaded.params.r#type, RgbMode::Static);
        assert_eq!(loaded.params.color, "#FF0000");
        loaded.params.validate().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newer_format_version_is_rejected() {
        let dir = std::env::temp_dir().join(format!("kbc-presets-test4-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        let p = store.path("future");
        std::fs::write(
            &p,
            r##"version = 9
type = "wave"
color = "#FF0000"
brightness = 100
speed = 30
frame_ms = 40
"##,
        )
        .unwrap();

        let err = store.load("future").unwrap_err();
        assert!(err.to_string().contains("новой схеме"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn builtin_audio_viz_visible_and_loadable() {
        let dir = std::env::temp_dir().join(format!("kbc-presets-test5-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        // Встроенный виден в списке даже в пустом хранилище.
        assert!(store.list().contains(&"audio-viz".to_string()));
        let p = store.load("audio-viz").unwrap();
        assert_eq!(p.params.r#type, RgbMode::Audio);
        assert!(is_builtin("audio-viz"));
        assert!(store.has("audio-viz"));
        // Встроенный удалить нельзя.
        let err = store.remove("audio-viz").unwrap_err();
        assert!(err.to_string().contains("встроенный"));
        // ensure на встроенном возвращает его, не создавая файл.
        let e = store.ensure("audio-viz").unwrap();
        assert_eq!(e.params.r#type, RgbMode::Audio);
        assert!(!store.path("audio-viz").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn builtin_cava_visible_and_loadable() {
        let dir = std::env::temp_dir().join(format!("kbc-presets-cava-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        assert!(store.list().contains(&"cava".to_string()));
        let p = store.load("cava").unwrap();
        assert_eq!(p.params.r#type, RgbMode::Audio);
        assert_eq!(p.params.audio.as_ref().unwrap().style, AudioStyle::Bars);
        assert!(is_builtin("cava"));
        assert!(store.has("cava"));
        let err = store.remove("cava").unwrap_err();
        assert!(err.to_string().contains("встроенный"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_file_overrides_builtin() {
        let dir = std::env::temp_dir().join(format!("kbc-presets-test6-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        let over = Preset {
            name: "audio-viz".to_string(),
            params: ModeParams {
                version: PRESET_FORMAT_VERSION,
                r#type: RgbMode::Static,
                color: "#0000FF".to_string(),
                brightness: 10,
                speed: 10,
                frame_ms: 40,
                audio: None,
            },
        };
        store.save(&over).unwrap();
        // Файл с тем же именем перекрывает встроенный.
        let loaded = store.load("audio-viz").unwrap();
        assert_eq!(loaded.params.r#type, RgbMode::Static);
        // Файл-переопределение удаляется, встроенный возвращается.
        store.remove("audio-viz").unwrap();
        let back = store.load("audio-viz").unwrap();
        assert_eq!(back.params.r#type, RgbMode::Audio);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audio_params_roundtrip() {
        // Секция `[audio]` сохраняется и читается; у не-аудио режима её нет.
        let dir = std::env::temp_dir().join(format!("kbc-presets-test7-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        let p = Preset {
            name: "viz".to_string(),
            params: ModeParams {
                version: PRESET_FORMAT_VERSION,
                r#type: RgbMode::Audio,
                color: "#FF5A00".to_string(),
                brightness: 255,
                speed: 60,
                frame_ms: 40,
                audio: Some(AudioParams {
                    sensitivity: 1.25,
                    color_low: "#00C800".to_string(),
                    color_high: "#2864FF".to_string(),
                    style: AudioStyle::Row,
                }),
            },
        };
        store.save(&p).unwrap();
        let raw = std::fs::read_to_string(store.path("viz")).unwrap();
        assert!(raw.contains("[audio]"), "в файле должна быть секция [audio]");
        let loaded = store.load("viz").unwrap();
        assert_eq!(loaded, p);
        assert_eq!(loaded.params.audio_params().sensitivity, 1.25);

        // Не-аудио пресет: секции нет, `audio_params()` даёт дефолты.
        let mut q = p.clone();
        q.name = "wave".to_string();
        q.params.r#type = RgbMode::Wave;
        q.params.audio = None;
        store.save(&q).unwrap();
        let raw = std::fs::read_to_string(store.path("wave")).unwrap();
        assert!(!raw.contains("[audio]"), "у wave секции [audio] быть не должно");
        let loaded = store.load("wave").unwrap();
        assert!(loaded.params.audio.is_none());
        assert_eq!(loaded.params.audio_params(), AudioParams::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audio_params_style_serialization() {
        // Проверка сериализации/десериализации поля style
        let dir = std::env::temp_dir().join(format!("kbc-presets-style-{}", std::process::id()));
        let store = PresetStore::new(dir.clone());
        let p = Preset {
            name: "style-test".to_string(),
            params: ModeParams {
                version: PRESET_FORMAT_VERSION,
                r#type: RgbMode::Audio,
                color: "#FF5A00".to_string(),
                brightness: 255,
                speed: 60,
                frame_ms: 40,
                audio: Some(AudioParams {
                    sensitivity: 0.75,
                    color_low: "#33FF33".to_string(),
                    color_high: "#3333FF".to_string(),
                    style: AudioStyle::Bars,
                }),
            },
        };
        store.save(&p).unwrap();
        let raw = std::fs::read_to_string(store.path("style-test")).unwrap();
        assert!(raw.contains("style = \"bars\""), "в файле должно быть style = \"bars\": {}", raw);
        let loaded = store.load("style-test").unwrap();
        assert_eq!(loaded.params.audio.as_ref().unwrap().style, AudioStyle::Bars);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
