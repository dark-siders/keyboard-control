// --- Хранилище профилей ---
// Сохранение профилей в локальные TOML-файлы и (де)сериализация
// для последующей записи в EEPROM клавиатуры.

use crate::types::{Color, EncoderAction, Keymap, RgbMode, RgbPreset};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Параметры RGB в профиле.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RgbProfile {
    pub preset: String,
    pub mode: RgbMode,
    pub colors: Vec<Color>,
    pub brightness: u8,
    pub speed: u8,
}

/// Параметры системы в профиле.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemProfile {
    pub polling_rate: u16,
    pub debounce_ms: u16,
    pub sleep_seconds: u16,
}

/// Настройка энкодера в профиле.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderProfile {
    pub action: EncoderAction,
}

/// Полный профиль устройства.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Человекочитаемое имя профиля.
    pub name: String,
    /// Модель, к которой он применим (VID:PID долгая форма).
    pub target_vid: u16,
    pub target_pid: u16,
    /// Версия формата.
    pub schema_version: u32,
    pub rgb: Option<RgbProfile>,
    pub keymap: Option<Keymap>,
    pub encoder: Option<EncoderProfile>,
    pub system: Option<SystemProfile>,
    /// EEPROM-слот, в который пишется профиль (-1 = не назначен).
    pub eeprom_slot: Option<u8>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Новый профиль".into(),
            target_vid: 0,
            target_pid: 0,
            schema_version: 1,
            rgb: None,
            keymap: None,
            encoder: None,
            system: None,
            eeprom_slot: None,
        }
    }
}

impl Profile {
    /// Создать профиль под конкретное устройство.
    pub fn for_device(name: &str, vid: u16, pid: u16) -> Self {
        Self {
            name: name.to_string(),
            target_vid: vid,
            target_pid: pid,
            ..Self::default()
        }
    }

    /// Прочитать профиль из файла (TOML).
    pub fn from_toml(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let profile: Profile = toml::from_str(&raw)?;
        Ok(profile)
    }

    /// Записать профиль в файл (TOML).
    pub fn save_toml(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    /// Собрать Keymap из профиля (для прошивки в устройство).
    pub fn keymap(&self) -> Option<Keymap> {
        self.keymap.clone()
    }

    /// RGB-пресет из профиля.
    pub fn rgb_preset(&self) -> Option<RgbPreset> {
        self.rgb.as_ref().map(|r| RgbPreset {
            name: r.preset.clone(),
            mode: r.mode,
            colors: r.colors.clone(),
            brightness: r.brightness,
            speed: r.speed,
        })
    }
}

/// Хранилище: каталог профилей в ~/.config/keyboard-center/profiles/
#[derive(Debug, Clone)]
pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    /// Каталог по умолчанию.
    pub fn default_dir() -> PathBuf {
        let cfg = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".config"))
                    .unwrap_or_else(|_| PathBuf::from(".config"))
            });
        cfg.join("keyboard-center").join("profiles")
    }

    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    pub fn open(&self) -> Self {
        Self::new(Self::default_dir())
    }

    /// Список сохранённых профилей.
    pub fn list(&self) -> Vec<PathBuf> {
        let mut v = std::fs::read_dir(&self.dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
            .map(|e| e.path())
            .collect::<Vec<_>>();
        v.sort();
        v
    }

    /// Загрузить все профили.
    pub fn load_all(&self) -> Vec<Profile> {
        self.list()
            .iter()
            .filter_map(|p| Profile::from_toml(p).ok())
            .collect()
    }

    /// Путь для нового профиля с уникальным именем.
    pub fn next_path(&self, base: &str) -> PathBuf {
        let mut n = 1;
        loop {
            let name = if n == 1 {
                format!("{base}.toml")
            } else {
                format!("{base}-{n}.toml")
            };
            let p = self.dir.join(&name);
            if !p.exists() {
                return p;
            }
            n += 1;
        }
    }

    /// Сохранить профиль и вернуть путь.
    pub fn store(&self, profile: &Profile) -> anyhow::Result<PathBuf> {
        let base = profile
            .name
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .to_lowercase()
            .trim()
            .to_string();
        let path = self.next_path(&base);
        profile.save_toml(&path)?;
        Ok(path)
    }

    /// Удалить профиль по пути.
    pub fn remove(&self, path: &Path) -> anyhow::Result<()> {
        if path.starts_with(&self.dir) {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}