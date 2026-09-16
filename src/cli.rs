// --- CLI-бинарник: kbc ---
// Командная строка для управления клавиатурами без GUI.
//
//   kbc list                         — показать найденные устройства
//   kbc info <vid:pid>               — детали устройства
//   kbc rgb  <vid:pid> --mode wave --brightness 180 --speed 120
//   kbc keymap <vid:pid> --list
//   kbc screen <vid:pid> --image file.gif
//   kbc time  <vid:pid>
//   kbc battery <vid:pid>
//   kbc set-poll <vid:pid> 1000
//   kbc set-debounce <vid:pid> 4
//   kbc set-sleep <vid:pid> 600
//   kbc profile save <path> --device <vid:pid> --name "Pro"
//   kbc profile load <path>
//   kbc profile eeprom-save <slot>
//   kbc monitor                        — горячая замена (watch)
//   kbc probe <vid:pid>                — разведка протокола (скан feature-репортов)
//   kbc raw <vid:pid> feature-get <id> <len>
//   kbc raw <vid:pid> feature-send <hex>
//   kbc raw <vid:pid> output-send <hex>
//   kbc raw <vid:pid> input-read <len> <timeout>

use clap::{Args, Parser, Subcommand};
use hidapi::HidApi;
use std::process::ExitCode;

use keyboard_center::devices::hidshell;
use keyboard_center::devices::scanner::Scanner;
use keyboard_center::devices::{ConnectionType, KeyboardDevice, RgbMode};

const LOGO: &str = "Keyboard-Center kbc";

#[derive(Parser)]
#[command(name = "kbc", version, about = "Keyboard-Center: управление клавиатурами AULA/Ajazz", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Список подключённых устройств
    List,
    /// Информация об устройстве по VID:PID (hex или dec)
    Info(DeviceArg),
    /// Управление RGB-подсветкой
    Rgb(RgbArgs),
    /// Загрузка изображения/GIF на экран (Ajazz AK820 Pro)
    Screen(ScreenArgs),
    /// Синхронизировать время ПК с экраном
    Time(DeviceArg),
    /// Уровень заряда батареи
    Battery(DeviceArg),
    /// Записать текущий конфиг подсветки в память клавиатуры
    /// (свет будет гореть без запущенного приложения)
    Save(SaveArgs),
    /// Установить частоту опроса
    SetPoll(SetRate),
    /// Установить задержку дребезга
    SetDebounce(SetMs),
    /// Установить таймер автоотключения
    SetSleep(SetSec),
    /// Работа с профилями
    Profile(ProfileArgs),
    /// Отслеживание горячей замены устройств
    Monitor,
    /// Разведка HID-протокола: интерфейсы + скан feature-репортов
    Probe(ProbeArgs),
    /// Raw-доступ к HID (диагностика протокола)
    Raw(RawArgs),
}

#[derive(Args, Clone)]
struct DeviceArg {
    /// VID:PID например 0328:8207
    #[arg(value_name = "VID:PID")]
    device: String,
}

#[derive(Args)]
struct RgbArgs {
    #[arg(value_name = "VID:PID")]
    device: String,
    #[arg(long)]
    mode: Option<String>,
    #[arg(long)]
    brightness: Option<u8>,
    #[arg(long)]
    speed: Option<u8>,
    /// Покрасить все LED в один цвет прямым кадром: RRGGBB (hex)
    #[arg(long)]
    color: Option<String>,
    /// Удерживать поток кадров N мс после отправки (direct-режим требует keepalive)
    #[arg(long, value_name = "ms")]
    hold: Option<u64>,
    /// [2.4G] Выключить свет в память ресивера (останется выключенным без приложения)
    #[arg(long)]
    light_off: bool,
    /// [2.4G] Включить свет в память ресивера (полной яркостью)
    #[arg(long)]
    light_on: bool,
}

#[derive(Args)]
struct ScreenArgs {
    #[arg(value_name = "VID:PID")]
    device: String,
    #[arg(long)]
    image: String,
}

#[derive(Args)]
struct SetRate {
    #[arg(value_name = "VID:PID")]
    device: String,
    /// 125 | 250 | 500 | 1000
    #[arg(value_name = "Hz")]
    rate: u16,
}

#[derive(Args)]
struct SetMs {
    #[arg(value_name = "VID:PID")]
    device: String,
    #[arg(value_name = "ms")]
    ms: u16,
}

#[derive(Args)]
struct SetSec {
    #[arg(value_name = "VID:PID")]
    device: String,
    #[arg(value_name = "sec")]
    sec: u16,
}

#[derive(Args)]
struct ProfileArgs {
    #[command(subcommand)]
    action: ProfileAction,
}

#[derive(Subcommand)]
enum ProfileAction {
    /// Сохранить текущее состояние в файл
    Save(ProfileSave),
    /// Загрузить профиль из файла на устройство
    Load(ProfileLoad),
    /// Записать профиль во встроенную память
    EepromSave(ProfileEeprom),
}

#[derive(Args)]
struct ProfileSave {
    #[arg(value_name = "PATH")]
    path: String,
    #[arg(long, value_name = "VID:PID")]
    device: String,
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args)]
struct ProfileLoad {
    #[arg(value_name = "PATH")]
    path: String,
    #[arg(long, value_name = "VID:PID")]
    device: String,
}

#[derive(Args)]
struct ProfileEeprom {
    #[arg(value_name = "VID:PID")]
    device: String,
    /// Номер слота EEPROM (0..=7)
    #[arg(value_name = "slot")]
    slot: u8,
}

#[derive(Args)]
struct ProbeArgs {
    #[arg(value_name = "VID:PID")]
    device: String,
    /// Максимальная длина буфера фичур-репорта (по умолчанию 63)
    #[arg(long, default_value_t = 63)]
    len: usize,
}

#[derive(Args)]
struct RawArgs {
    #[arg(value_name = "VID:PID")]
    device: String,
    /// Путь к конкретному интерфейсу (напр. /dev/hidraw2); иначе первый
    #[arg(long)]
    path: Option<String>,
    #[command(subcommand)]
    op: RawOp,
}

#[derive(Subcommand)]
enum RawOp {
    /// Прочитать feature-репорт: id и длина данных
    FeatureGet { id: u8, len: usize },
    /// Отправить feature-репорт: HEX (первый байт = report id)
    FeatureSend(HexArg),
    /// Отправить output-репорт: HEX
    OutputSend(HexArg),
    /// Прочитать input-репорт: len — длина данных, timeout — мс
    InputRead { len: usize, timeout_ms: i32 },
}

#[derive(Args)]
struct HexArg {
    /// HEX-байты: "02 01 00" или "020100"
    #[arg(value_name = "HEX")]
    hex: String,
}

#[derive(Args)]
struct SaveArgs {
    /// VID:PID например 258A:010C
    #[arg(value_name = "VID:PID")]
    device: String,
    /// Код аппаратного эффекта (hex, например 0B) — сохранение эффекта
    /// прошивки в память. Без флага — сохранить конфиг без смены эффекта.
    #[arg(long)]
    effect: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match exec(cli.command) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("ошибка: {e}");
            ExitCode::FAILURE
        }
    }
}

fn exec(cmd: Commands) -> anyhow::Result<u8> {
    match cmd {
        Commands::List => cmd_list(),
        Commands::Info(dev) => cmd_info(parse_id(&dev.device)?),
        Commands::Rgb(args) => cmd_rgb(args),
        Commands::Screen(args) => cmd_screen(args),
        Commands::Time(dev) => {
            let (vid, pid) = parse_id(&dev.device)?;
            let dev = open(vid, pid)?;
            dev.sync_system_time()?;
            println!("время синхронизировано");
            Ok(0)
        }
        Commands::Battery(dev) => {
            let (vid, pid) = parse_id(&dev.device)?;
            let dev = open(vid, pid)?;
            match dev.get_battery_level()? {
                Some(l) => println!("батарея: {l}%"),
                None if dev.connection_type() == ConnectionType::Wireless2_4G => {
                    println!("батарея: нет ответа (клавиатура спит на линке — нажмите клавишу и повторите)");
                }
                None if dev.connection_type() == ConnectionType::Usb => {
                    println!(
                        "батарея по USB не читается (проводной канал без данных о заряде) — \
                         переключите клавиатуру на 2.4G/BT"
                    );
                }
                None => println!("батарея не поддерживается устройством"),
            }
            Ok(0)
        }
        Commands::Save(a) => cmd_save(a),
        Commands::SetPoll(a) => {
            let (vid, pid) = parse_id(&a.device)?;
            let dev = open(vid, pid)?;
            dev.set_polling_rate(a.rate)?;
            println!("polling rate: {} Гц", a.rate);
            Ok(0)
        }
        Commands::SetDebounce(a) => {
            let (vid, pid) = parse_id(&a.device)?;
            let dev = open(vid, pid)?;
            dev.set_debounce(a.ms)?;
            println!("debounce: {} мс", a.ms);
            Ok(0)
        }
        Commands::SetSleep(a) => {
            let (vid, pid) = parse_id(&a.device)?;
            let dev = open(vid, pid)?;
            dev.set_sleep_timer(a.sec)?;
            println!("sleep timer: {} с", a.sec);
            Ok(0)
        }
        Commands::Profile(p) => cmd_profile(p),
        Commands::Monitor => cmd_monitor(),
        Commands::Probe(a) => cmd_probe(&a.device, a.len),
        Commands::Raw(a) => cmd_raw(a),
    }
}

/// Разобрать строку "0328:8207" (hex) или "808:33287" (dec).
fn parse_id(s: &str) -> anyhow::Result<(u16, u16)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("ожидается формат VID:PID, получили {s}"));
    }
    let vid = if parts[0].starts_with("0x") {
        u16::from_str_radix(&parts[0][2..], 16)?
    } else if parts[0].len() > 4 {
        parts[0].parse::<u16>()?
    } else {
        u16::from_str_radix(parts[0], 16).or_else(|_| parts[0].parse::<u16>())?
    };
    let pid = if parts[1].starts_with("0x") {
        u16::from_str_radix(&parts[1][2..], 16)?
    } else if parts[1].len() > 4 {
        parts[1].parse::<u16>()?
    } else {
        u16::from_str_radix(parts[1], 16).or_else(|_| parts[1].parse::<u16>())?
    };
    Ok((vid, pid))
}

/// Открыть устройство.
fn open(vid: u16, pid: u16) -> anyhow::Result<Box<dyn KeyboardDevice>> {
    let (tx, _rx) = std::sync::mpsc::channel();
    let scanner = Scanner::new(tx)?;
    scanner
        .open_device(vid, pid, None)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn cmd_list() -> anyhow::Result<u8> {
    println!("{LOGO}");
    println!("Сканирование HID-устройств…\n");
    let (tx, _rx) = std::sync::mpsc::channel();
    let scanner = Scanner::new(tx)?;
    let found = scanner.scan().map_err(|e| anyhow::anyhow!(e.to_string()))?;

    if found.is_empty() {
        println!("Устройства не найдены.");
        println!("Проверьте: udev-правила установлены? (99-keyboard-center.rules)");
        return Ok(0);
    }

    for dev in &found {
        println!(
            "  {:<24} {:04X}:{:04X}  {:<12}  layout={:<5}  path={}",
            dev.info.model,
            dev.info.vid,
            dev.info.pid,
            dev.info.connection.label(),
            if dev.info.layout == keyboard_center::devices::Layout::L75 { "75%" } else { "96%" },
            dev.path.as_deref().unwrap_or("—")
        );
    }
    Ok(0)
}

fn cmd_info((vid, pid): (u16, u16)) -> anyhow::Result<u8> {
    let dev = open(vid, pid)?;
    println!("Модель:     {}", dev.name());
    println!("Подключение: {}", dev.connection_type().label());
    println!("VID:PID:    {:04X}:{:04X}", dev.vid_pid().0, dev.vid_pid().1);
    println!("Подключено: {}", if dev.is_connected() { "да" } else { "нет" });
    match dev.get_battery_level()? {
        Some(l) => println!("Батарея:    {l}%"),
        None => println!("Батарея:    —"),
    }
    Ok(0)
}

fn cmd_rgb(args: RgbArgs) -> anyhow::Result<u8> {
    let (vid, pid) = parse_id(&args.device)?;
    let dev = open(vid, pid)?;

    // Канал света (вкл/выкл) в память: ресивер 2.4G отвечает Ok(true);
    // USB — не умеет через CLI (в GUI — RGB-студия → «Выключить свет»).
    if args.light_off {
        return cmd_light_persistent(&*dev, false);
    }
    if args.light_on {
        return cmd_light_persistent(&*dev, true);
    }

    // Ресивер 2.4G (3554:FA09): RGB записывается СРАЗУ во флеш (протокол
    // 0x13, журнал 10.42) — host-keepalive не нужен, эффект/цвет/яркость/
    // скорость держатся без приложения. Контракт хука: Ok(true) = применено
    // аппаратно; Ok(false) = устройство не умеет (внизу — host-рендеринг).
    if dev.connection_type() == ConnectionType::Wireless2_4G {
        let mode = parse_rgb_mode(args.mode.as_deref())?;
        let color = match &args.color {
            Some(c) => Some(
                keyboard_center::types::Color::from_hex_checked(c)
                    .ok_or_else(|| anyhow::anyhow!("неверный цвет: {c} (ожидается RRGGBB)"))?,
            ),
            None => None,
        };
        let brightness = args.brightness.unwrap_or(128);
        let speed = args.speed.unwrap_or(60);
        return match dev.apply_rgb_persistent(mode, color, brightness, speed)? {
            true => {
                println!(
                    "2.4G: режим {mode:?}, яркость={brightness}, скорость={speed} записаны в память ресивера (держится без приложения)"
                );
                Ok(0)
            }
            false => Err(anyhow::anyhow!(
                "ресивер не поддерживает запись эффекта в память"
            )),
        };
    } else if let Some(color) = &args.color {
        let c = keyboard_center::types::Color::from_hex_checked(color)
            .ok_or_else(|| anyhow::anyhow!("неверный цвет: {color} (ожидается RRGGBB)"))?;
        // Прямой (host-rendering) кадр: заливает ВСЕ LED заданным цветом.
        let n = dev.led_count();
        let frame = vec![c; n];
        match dev.render_frame(&frame) {
            Ok(()) => println!("цвет {color} отправлен прямым кадром ({n} LED)"),
            Err(e) if e.to_string().contains("хост-рендеринг") => {
                // Fallback для устройств без direct-режима: per-key пример.
                dev.set_rgb_mode(RgbMode::Custom)?;
                dev.set_rgb_color(keyboard_center::types::KeyId(0x2C), c)?;
                println!("устройство без direct-режима — per-key цвет применён к пробелу (пример)");
            }
            Err(e) => return Err(anyhow::anyhow!(e)),
        }
        // Direct-режим живёт только пока идёт поток кадров.
        if let Some(hold) = args.hold {
            let interval = std::time::Duration::from_millis(400);
            let end = std::time::Instant::now() + std::time::Duration::from_millis(hold);
            let mut sent = 1;
            while std::time::Instant::now() < end {
                std::thread::sleep(interval);
                dev.render_frame(&frame)?;
                sent += 1;
            }
            println!("keepalive: отправлено {sent} кадров за {hold} мс");
        }
    }
    //
    // Параметры режима/яркости/скорости → host-движок (журнал 10.18).
    // Аппаратной эффект-таблицы у F75 нет (latch эмпирически не найден,
    // журнал 10.14/10.16), поэтому ЛЮБОЙ режим/яркость/скорость рендерим
    // НА ХОСТЕ и гоним кадрами через подтверждённый render_frame()-канал
    // с keepalive — пока CLI жив (Ctrl-C = выход).
    //
    if args.mode.is_some() || args.brightness.is_some() || args.speed.is_some() {
        use keyboard_center::devices::aula_f75::{host_frame_with, HostEffectParams};
        let mode = parse_rgb_mode(args.mode.as_deref())?;
        let base = match &args.color {
            Some(color) => keyboard_center::types::Color::from_hex_checked(color)
                .ok_or_else(|| anyhow::anyhow!("неверный цвет: {color} (ожидается RRGGBB)"))?,
            None => keyboard_center::types::Color::new(255, 90, 0),
        };
        let n = dev.led_count();
        let cols = dev.led_cols();
        let params = HostEffectParams {
            mode,
            base,
            brightness: args.brightness.unwrap_or(255),
            speed: args.speed.unwrap_or(60),
        };
        println!("host-эффект: {mode:?} яркость={} скорость={} (матрица {n}×~{cols}) — keepalive до Ctrl-C",
                 params.brightness, params.speed);
        let mut t_ms = 0u32;
        loop {
            let frame = host_frame_with(&params, t_ms, n, cols);
            dev.render_frame(&frame)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            t_ms += 500;
        }
    }
    Ok(0)
}

/// Парсер host-режима подсветки из строки CLI (общий для USB и 2.4G).
fn parse_rgb_mode(s: Option<&str>) -> anyhow::Result<RgbMode> {
    let s = s.unwrap_or("static").to_lowercase();
    let m = match s.as_str() {
        "static" => RgbMode::Static,
        "breathing" => RgbMode::Breathing,
        "wave" => RgbMode::Wave,
        "spectrum" | "spectrum-cycle" => RgbMode::SpectrumCycle,
        "ripple" => RgbMode::Ripple,
        "rain" => RgbMode::Rain,
        "reactive" => RgbMode::Reactive,
        "custom" => RgbMode::Custom,
        _ => {
            return Err(anyhow::anyhow!(
                "неизвестный режим: {s} (static/breathing/wave/spectrum/ripple/rain/reactive/custom)"
            ))
        }
    };
    Ok(m)
}

/// [2.4G] Включить/выключить свет в память ресивера через apply_light_persistent.
fn cmd_light_persistent(dev: &dyn KeyboardDevice, on: bool) -> anyhow::Result<u8> {
    match dev.apply_light_persistent(on)? {
        true => {
            println!(
                "2.4G: свет {} в память ресивера (держится без приложения)",
                if on { "включён" } else { "выключен" }
            );
            Ok(0)
        }
        false => Err(anyhow::anyhow!(
            "USB-устройство не поддерживает запись света в память через CLI (в GUI: RGB-студия → «Выключить свет»)"
        )),
    }
}

fn cmd_save(a: SaveArgs) -> anyhow::Result<u8> {
    let (vid, pid) = parse_id(&a.device)?;
    let dev = open(vid, pid)?;
    match &a.effect {
        Some(e) => {
            let hex = e.trim_start_matches("0x").trim_start_matches("0X");
            let code =
                u8::from_str_radix(hex, 16).map_err(|_| anyhow::anyhow!("неверный код эффекта: {e} (hex)"))?;
            dev.save_hardware_effect(code)?;
            println!(
                "аппаратный эффект 0x{code:02X} ({:?}) записан в память клавиатуры",
                keyboard_center::devices::aula_f75::hw_effect_name(code)
            );
        }
        None => {
            dev.save_profile_to_eeprom(0)?;
            println!("конфиг подсветки записан в память клавиатуры (свет включён, держится без приложения)");
        }
    }
    Ok(0)
}

fn cmd_screen(args: ScreenArgs) -> anyhow::Result<u8> {
    let (vid, pid) = parse_id(&args.device)?;
    let dev = open(vid, pid)?;

    let bytes = std::fs::read(&args.image)
        .map_err(|e| anyhow::anyhow!("не удалось прочитать {}: {e}", args.image))?;
    let anim = keyboard_center::screen::converter::convert(&bytes)?;
    let frame = anim.first_frame();
    let size = frame.data.len();
    println!(
        "конвертация: {}x{} пикселей, {} кадров, {}, RGB565",
        anim.width, anim.height, anim.frames.len(), size
    );

    let sf = keyboard_center::types::ScreenFrame {
        width: anim.width,
        height: anim.height,
        data_rgb565: frame.data.clone(),
    };
    dev.set_screen_image(sf)?;
    println!("картинка отправлена на экран");
    Ok(0)
}

fn cmd_profile(p: ProfileArgs) -> anyhow::Result<u8> {
    use keyboard_center::profiles::{Profile, ProfileStore};

    let store = ProfileStore::new(ProfileStore::default_dir());
    match p.action {
        ProfileAction::Save(a) => {
            let (vid, pid) = parse_id(&a.device)?;
            let name = a.name.unwrap_or_else(|| "Профиль_по_умолчанию".into());
            let profile = Profile::for_device(&name, vid, pid);
            if let Some(dev) = open(vid, pid).ok() {
                if let Ok(Some(level)) = dev.get_battery_level() {
                    let _ = level;
                }
            }
            let path = store.store(&profile)?;
            println!("профиль сохранён: {}", path.display());
            Ok(0)
        }
        ProfileAction::Load(a) => {
            let (_vid, _pid) = parse_id(&a.device)?;
            let profile = Profile::from_toml(&a.path)
                .map_err(|e| anyhow::anyhow!("не удалось прочитать профиль: {e}"))?;
            println!("профиль «{}»: RGB={}, keymap={}",
                profile.name,
                if profile.rgb.is_some() { "да" } else { "нет" },
                if profile.keymap.is_some() { "да" } else { "нет" });
            Ok(0)
        }
        ProfileAction::EepromSave(a) => {
            let (vid, pid) = parse_id(&a.device)?;
            let dev = open(vid, pid)?;
            dev.save_profile_to_eeprom(a.slot)?;
            println!("профиль записан в слот {}", a.slot);
            Ok(0)
        }
    }
}

fn cmd_monitor() -> anyhow::Result<u8> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut scanner = Scanner::new(tx)?;
    println!("Отслеживание горячей замены. Для выхода Ctrl+C");
    loop {
        scanner.poll_hotplug();
        while let Ok(event) = rx.try_recv() {
            match event {
                keyboard_center::devices::scanner::HotplugEvent::Connected { vid, pid, path } => {
                    println!("подключено: {:04X}:{:04X} {:?}", vid, pid, path);
                }
                keyboard_center::devices::scanner::HotplugEvent::Disconnected { vid, pid, path } => {
                    println!("отключено: {:04X}:{:04X} {:?}", vid, pid, path);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Разведка интерфейсов и фичур-репортов.
fn cmd_probe(device: &str, len: usize) -> anyhow::Result<u8> {
    let (vid, pid) = parse_id(device)?;
    let api = HidApi::new()?;
    let ifaces = hidshell::list_interfaces(&api, vid, pid);

    if ifaces.is_empty() {
        println!("Устройство {vid:04X}:{pid:04X} не найдено.");
        return Ok(0);
    }
    println!("Устройство {vid:04X}:{pid:04X} — интерфейсов: {}\n", ifaces.len());

    for (i, it) in ifaces.iter().enumerate() {
        println!(
            "--- интерфейс #{i}: {}  usage_page=0x{:04X} usage=0x{:04X} iface_num={} ---",
            it.path, it.usage_page, it.usage, it.interface_number
        );
        let dev = match hidshell::open_path(&api, &it.path) {
            Ok(d) => d,
            Err(e) => {
                println!("  не удалось открыть: {e}");
                continue;
            }
        };
        let hits = hidshell::probe_features(&dev, len);
        if hits.is_empty() {
            println!("  фичур-репорты не ответили (попробуйте --len 255 или 1020)");
        }
        for (id, data) in &hits {
            println!("  0x{id:02X}: {}", hidshell::hex_dump(data));
        }
    }
    Ok(0)
}

/// Низкоуровневый raw-доступ.
fn cmd_raw(a: RawArgs) -> anyhow::Result<u8> {
    let (vid, pid) = parse_id(&a.device)?;
    let api = HidApi::new()?;
    let path = match &a.path {
        Some(p) => p.clone(),
        None => {
            let ifaces = hidshell::list_interfaces(&api, vid, pid);
            // Командный канал AULA — интерфейс с вендорским usage page (≥0xFF00,
            // материал: /dev/hidraw6), а не клавиатурный (hidraw5).
            let it = ifaces.iter().find(|i| i.usage_page >= 0xFF00).or_else(|| ifaces.first());
            if let Some(it) = it {
                it.path.clone()
            } else {
                return Err(anyhow::anyhow!(
                    "устройство {vid:04X}:{pid:04X} не найдено; укажите --path"
                ));
            }
        }
    };
    let dev = hidshell::open_path(&api, &path)?;

    match &a.op {
        RawOp::FeatureGet { id, len } => match hidshell::get_feature(&dev, *id, *len) {
            Ok(data) => println!("{}", hidshell::hex_dump(&data)),
            Err(e) => {
                eprintln!("get feature 0x{id:02X}: {e}");
                return Ok(1);
            }
        },
        RawOp::FeatureSend(hex) => {
            let bytes = parse_hex(&hex.hex).map_err(|e| anyhow::anyhow!(e))?;
            hidshell::send_feature(&dev, &bytes)?;
            println!("feature отправлен: {}", hidshell::hex_dump(&bytes));
        }
        RawOp::OutputSend(hex) => {
            let bytes = parse_hex(&hex.hex).map_err(|e| anyhow::anyhow!(e))?;
            hidshell::send_output(&dev, &bytes)?;
            println!("output отправлен: {}", hidshell::hex_dump(&bytes));
        }
        RawOp::InputRead { len, timeout_ms } => {
            match hidshell::read_input(&dev, *len, *timeout_ms) {
                Ok(data) => println!("{}", hidshell::hex_dump(&data)),
                Err(e) => {
                    eprintln!("read input: {e}");
                    return Ok(1);
                }
            }
        }
    }
    Ok(0)
}

/// Распарсить строку HEX-байтов: "02 01 00" или "020100".
fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s.split_whitespace().collect();
    if cleaned.len() % 2 != 0 {
        return Err(format!("нечётное число hex-цифр: {s}"));
    }
    cleaned
        .as_bytes()
        .chunks(2)
        .map(|chunk| {
            let t = std::str::from_utf8(chunk).map_err(|e| e.to_string())?;
            u8::from_str_radix(t, 16).map_err(|e| e.to_string())
        })
        .collect()
}