# Официальные ассеты Ajazz AK820 Pro

Директория для файлов из официального ПО Ajazz, снятых с установщика
`AJAZZ_AK820_Pro_Triple_mode_0.85inch_Screen_RGB_V1.0.0.5.exe` (Inno Setup 6.1.0).

> Для работы Keyboard-Center файлы **не нужны** — программа общается с
> клавиатурой по HID. Ассеты используются только как эталон при реверс-инжиниринге
> и как источник захардкоженных данных (заводская раскладка, порядок светодиодов).
>
> Файлы — проприетарные (Ajazz). Распространение — на усмотрение владельца
> репозитория.

## Состав

### Раскладка — `rgb-keyboard.xml`

| Параметр | Значение |
|---|---|
| Устройство | Ajazz AK820 Pro (ревизия `0C45:8009`, USB) |
| Источник | `layouts/rgb-keyboard.xml` из установщика V1.0.0.5 |
| Формат | XML, 81 клавиша (`<KeyItems>`) |
| Размер файла | 15 588 байт |
| MD5 файла | `de2cb8e9158e3d03aa1af8e038f0b63a` |
| Дата снятия | 2026-11-14 |
| Статус | справочный эталон, без модификаций |

Что содержит и как используется:

- **`code`** — HID usage страницы `0x07` (Keyboard/Keypad), у Fn — вендорский
  `0xAF`. Используется в `HID_USAGE_TO_LED` (`src/devices/ajazz_ak820.rs`) как
  ключ для per-key подсветки.
- **`light_index`** — индекс светодиода; в этом файле совпадает с `key_index`
  (1..=121). Используется как `KeyId` в `factory_keymap_ak820()`.
- **`key_index`** — порядковый индекс клавиши, совпадает с `light_index`.
- **`fnlayer_disable`** — какие клавиши выключены в Fn-слое (`1` — Esc,
  Backspace, `|`, Win_L, Fn).

Глобальные значения из шапки (`<light>`, `<cmd_delaytime>`, `<screen>`):

| Параметр | Значение | Примечание |
|---|---|---|
| `default_mode` | 11 | заводской режим подсветки |
| `default_brightness` | 5 | максимум 5 (не 0..255) |
| `default_speed` | 3 | максимум 5 |
| `cmd_delaytime` | 35 | совпадает с `ctl::PAUSE_MS` в драйвере |
| `screen` | 128×128, gif_headlength 256, gif_maxframes 140 | параметры TFT |

Чего в файле **нет**:

- **Заводской Fn-слой** — в установщике отсутствует (только base). Поэтому
  `factory_keymap_ak820().fn_layer` пустой.
- **Макросы** — хранятся не в XML, а в SQLite-схеме `DeviceDriver.exe`
  (`t_key_macro_data`) на стороне ПК.

## Как получено

```bash
# Inno Setup-установщик распаковывается innoextract (временная оболочка, не в конфиг)
nix-shell -p innoextract --run \
  'innoextract -d /tmp/opencode/ak820_app \
     "AJAZZ_AK820_Pro_Triple_mode_0.85inch_Screen_RGB_V1.0.0.5.exe"'
# интересующий файл:
#   /tmp/opencode/ak820_app/app/layouts/rgb-keyboard.xml
```

## Полезные ссылки

- `deddroid/ak820-rs` — реверс того же лейаута в `keys.rs` (сверено, расхождений
  по значениям нет).
- `Twyx22/ak820-center` — реверс протокола (`PROTOCOL.md`), в т.ч. команд
  keymap `0x11`/`0x27` и макросов `0x19`/`0x15` на рабочем канале `0xFF13`.
