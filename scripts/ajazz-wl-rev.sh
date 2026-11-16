#!/usr/bin/env bash
# ─── 2.4G-донгл Ajazz 0C45:FDFD: write-тесты SET_LED_EFFECT (ПО РЕШЕНИЮ ВЛАДЕЛЬЦА) ───
# Отправляет 8 вариантов записи подсветки через командный канал 0xFF60 (MI_03,
# 32-байтные AA-кадры + checksum) РАЗНЫМИ ЦВЕТАМИ, чтобы владелец по цвету
# определил, какой вариант прошивка применила визуально.
#
# КАЖДЫЙ ВАРИАНТ — РАЗНЫЙ ЦВЕТ (смотри на клавиатуру!):
#   V1 red     — просто SET_LED_EFFECT (базовый, с checksum)
#   V2 green   — сессия START(1) → SET(35) → END(2), checksum на всех
#   V3 blue    — SET БЕЗ checksum (байт 31 = 0)
#   V4 cyan    — SET с last-флагом = 0
#   V5 magenta — START/END без checksum, SET с checksum
#   V6 yellow  — сессия как V2 + добивочный пинг GET_DEVICE_INFO (сброс отложенного ack)
#   V7 white   — SET с addr = 1
#   V8 orange  — сессия V2 с паузами 100 мс + пинг
#
# Использование:
#   ./scripts/ajazz-wl-rev.sh                 # автодетект узла MI_03
#   ./scripts/ajazz-wl-rev.sh --path /dev/hidraw8
#
# Перед запуском НАЖМИ КЛАВИШУ на клавиатуре (пробудить 2.4G-линк).
set -euo pipefail

NODE=""
case "${1:-}" in
    --path) NODE="${2:?укажите путь}"; shift 2 ;;
    -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
esac

if [[ -z "$NODE" ]]; then
    echo "[i] автодетект MI_03 (usage 0xFF60, report 32 б)…" >&2
    for d in /sys/class/hidraw/hidraw*; do
        [ -e "$d" ] || continue
        ue="$d/device/uevent"
        [[ -f "$ue" ]] || continue
        if grep -qi 'HID_ID=0003:00000C45:0000FDFD' "$ue"; then
            rd="$d/device/report_descriptor"
            if python3 -c "
import sys
b = open('$rd','rb').read()
print('ok' if b'\x06\x60\xff' in b and b'\x95\x20' in b else '')" 2>/dev/null | grep -q ok; then
                NODE="/dev/${d##*/}"
                break
            fi
        fi
    done
    [[ -n "$NODE" ]] || { echo "MI_03 не найден — укажите --path /dev/hidrawN" >&2; exit 1; }
fi
echo "[i] командный узел донгла: $NODE" >&2

python3 - "$NODE" <<'PYEOF'
import os, sys, time, select, struct

node = sys.argv[1]
fd = os.open(node, os.O_RDWR | os.O_CLOEXEC)

START, SET_LED, END, PING = 0x01, 0x23, 0x02, 0x10

def frame(cmd, length, addr=0, payload=b"", last=1, csum=True):
    f = bytearray(32)
    f[0], f[1], f[2] = 0xAA, cmd, length
    f[3], f[4] = addr & 0xFF, (addr >> 8) & 0xFF
    f[6] = last
    f[8:8 + len(payload)] = payload
    if csum:
        f[31] = sum(f[0:31]) & 0xFF
    return b"\x00" + bytes(f)   # report id 0x00 + кадр

def led(mode, rgb):
    p = bytearray(16)
    p[0] = mode                 # 1 = Static
    p[1:4] = bytes(rgb)
    p[4] = 0xFF                 # driverSetting (хардкод вендора)
    p[5:8] = bytes(rgb)         # secondary RGB
    p[8] = 0                    # colorMode: mono
    p[9], p[10] = 5, 3          # brightness 5, speed 3
    p[11] = 0                   # direction
    p[14:16] = b"\xaa\x55"      # checkCode
    return bytes(p)

def send(b):
    os.write(fd, b)

def drain(ms=400):
    end = time.monotonic() + ms / 1000
    while True:
        left = end - time.monotonic()
        if left <= 0:
            break
        r, _, _ = select.select([fd], [], [], left)
        if not r:
            break
        os.read(fd, 64)

def pause(ms):
    time.sleep(ms / 1000)

variants = [
    # (имя, цвет, последовательность кадров)
    ("V1 red    SET (базовый)",
     (0xFF, 0x00, 0x00),
     [frame(SET_LED, 16, payload=led(1, (0xFF, 0x00, 0x00)))]),
    ("V2 green  START→SET→END, checksum на всех",
     (0x00, 0xFF, 0x00),
     [frame(START, 0), frame(SET_LED, 16, payload=led(1, (0x00, 0xFF, 0x00))), frame(END, 0)]),
    ("V3 blue   SET без checksum",
     (0x00, 0x00, 0xFF),
     [frame(SET_LED, 16, payload=led(1, (0x00, 0x00, 0xFF)), csum=False)]),
    ("V4 cyan   SET с last=0",
     (0x00, 0xFF, 0xFF),
     [frame(SET_LED, 16, payload=led(1, (0x00, 0xFF, 0xFF)), last=0)]),
    ("V5 magenta START/END без checksum",
     (0xFF, 0x00, 0xFF),
     [frame(START, 0, csum=False),
      frame(SET_LED, 16, payload=led(1, (0xFF, 0x00, 0xFF))),
      frame(END, 0, csum=False)]),
    ("V6 yellow сессия V2 + пинг",
     (0xFF, 0xFF, 0x00),
     [frame(START, 0), frame(SET_LED, 16, payload=led(1, (0xFF, 0xFF, 0x00))),
      frame(END, 0), frame(PING, 48)]),
    ("V7 white  SET с addr=1",
     (0xFF, 0xFF, 0xFF),
     [frame(SET_LED, 16, addr=1, payload=led(1, (0xFF, 0xFF, 0xFF)))]),
    ("V8 orange сессия V2, паузы 100 мс + пинг",
     (0xFF, 0x80, 0x00),
     [frame(START, 0), frame(SET_LED, 16, payload=led(1, (0xFF, 0x80, 0x00))),
      frame(END, 0), frame(PING, 48)]),
]

print("СТАРТ: нажмите клавишу на клавиатуре (пробудить линк)…", flush=True)
drain()
if sys.stdin.isatty():
    input("нажали? Enter — запустить все 8 вариантов ")
else:
    print("(неинтерактивный запуск — старт без паузы)", flush=True)

for i, (name, rgb, seq) in enumerate(variants, 1):
    print(f"\n=== V{i}: {name} ===", flush=True)
    for k, f in enumerate(seq):
        print(f"  кадр {k}: {f[1:].hex(' ')}", flush=True)
        send(f)
        pause(100 if i == 8 else 50)
    drain(600)
    pause(3500)   # смотри на клавиатуру: загорелся ли цвет {rgb}?

print("\nГОТОВО. Какой цвет видно на клавиатуре? (имя варианта = его рабочая последовательность)")
print("Если ничего — перебор исчерпан на этом уровне: нужен usbmon-захват официального ПО.")
os.close(fd)
PYEOF
