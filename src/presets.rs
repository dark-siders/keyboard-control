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

/// Параметры host-эффекта, пригодные для сохранения в пресет.
///
/// Сериализуются в TOML как:
/// ```toml
/// type = "wave"          # snake_case из types::RgbMode
/// color = "#FF5A00"
/// brightness = 200
/// speed = 60
/// frame_ms = 40
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeParams {
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
}

impl Default for ModeParams {
    fn default() -> Self {
        Self {
            r#type: RgbMode::Wave,
            color: "#FF5A00".to_string(),
            brightness: 200,
            speed: 60,
            frame_ms: DEFAULT_FRAME_MS,
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

    /// Существует ли пресет с таким именем.
    pub fn has(&self, name: &str) -> bool {
        self.path(name).is_file()
    }

    /// Имена всех пресетов (из *.toml-файлов), отсортированы.
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
        names.sort();
        names
    }

    /// Загрузить пресет по имени.
    pub fn load(&self, name: &str) -> anyhow::Result<Preset> {
        let p = self.path(name);
        let raw = std::fs::read_to_string(&p)
            .map_err(|e| anyhow::anyhow!("не удалось прочитать пресет {}: {e}", p.display()))?;
        let params: ModeParams = toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("не удалось разобрать пресет {}: {e}", p.display()))?;
        Ok(Preset {
            name: sanitize(name),
            params,
        })
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

    /// Удалить пресет.
    pub fn remove(&self, name: &str) -> anyhow::Result<()> {
        let p = self.path(name);
        std::fs::remove_file(&p)
            .map_err(|e| anyhow::anyhow!("не удалось удалить пресет {}: {e}", p.display()))?;
        Ok(())
    }

    /// Гарантировать пресет: если его нет — создать (для «default») и вернуть.
    /// Для любого другого имени без файла — ошибка с подсказкой.
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
                r#type: RgbMode::Wave,
                color: "#00FF00".to_string(),
                brightness: 255,
                speed: 90,
                frame_ms: 30,
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
}