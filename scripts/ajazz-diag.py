#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ajazz-diag.py — автономная диагностика Ajazz AK820 Pro (USB 0C45:8009)
Канал подсветки: 0xFF13 (feature reports, unnumbered, 64 Б).

Требует: python3, python3-hidapi (dnf install python3-hidapi)
Запуск: python3 ajazz-diag.py [--write-tests] [--framing a|b] [--device 0C45:8009]

FRAMING (критично для 0xFF13, журнал 10.112):
  a — 64 б, buf[0] = report id (0x04 для команд, код эффекта для mode-data)
  b — 65 б, ведущий 0x00 + 64-б пакет (hidapi на hidraw требует report id 0x00)
Дефолт: b (подтверждено на живом железе 0C45:8009).
"""

import os
import sys
import time
import struct
import argparse
import subprocess
from pathlib import Path
from datetime import datetime

try:
    import hid
except ImportError:
    print("Ошибка: python3-hidapi не установлен")
    print("Fedora: sudo dnf install python3-hidapi")
    sys.exit(1)

# --- константы протокола 0xFF13 ---
CTL_PACKET_LEN = 64
REPORT_ID_CMD = 0x04
DELIM_LO = 0xAA
DELIM_HI = 0x55

CMD_START = 0x18
CMD_MODE  = 0x13
CMD_TIME  = 0x28
CMD_SLEEP = 0x17
CMD_SAVE  = 0x02
CMD_FINISH = 0xF0

MODES = {
    0x00: "Off", 0x01: "Static", 0x02: "SingleOn", 0x03: "SingleOff",
    0x04: "Glittering", 0x05: "Falling", 0x06: "Colourful", 0x07: "Breath",
    0x08: "Spectrum", 0x09: "Outward", 0x0A: "Scrolling", 0x0B: "Rolling",
    0x0C: "Rotating", 0x0D: "Explode", 0x0E: "Launch", 0x0F: "Ripples",
    0x10: "Flowing", 0x11: "Pulsating", 0x12: "Tilt", 0x13: "Shuttle",
}

GREEN = "\033[0;32m"
YELLOW = "\033[0;33m"
RED = "\033[0;31m"
NC = "\033[0m"

def ok(msg): print(f"{GREEN}  ✓ {msg}{NC}")
def warn(msg): print(f"{YELLOW}  ! {msg}{NC}")
def fail(msg): print(f"{RED}  ✗ {msg}{NC}")
def hdr(msg): print(f"\n{GREEN}== {msg} =={NC}")

def hexb(v):
    if isinstance(v, str):
        v = v.lstrip("0x")
        return f"{int(v, 16):02X}"
    return f"{v:02X}"

def decb(v):
    return f"{int(v):02X}"

def frame_pkt(pkt_hex: str, framing: str) -> str:
    """pkt_hex — 64-байтный hex-стринг с пробелами. Возвращает framed hex."""
    if framing == "a":
        return pkt_hex
    return "00 " + pkt_hex

def ctl_packet(cmd, b2, b8):
    parts = [f"04 {hexb(cmd)} {hexb(b2)} 00 00 00 00 00 {hexb(b8)}"]
    parts += ["00"] * (64 - 9)
    return " ".join(parts)

def mode_data_packet(mode, r, g, b, brightness, speed, rainbow=0, direction=0):
    parts = [
        hexb(mode), hexb(r), hexb(g), hexb(b),
        "00", "00", "00", "00",
        hexb(rainbow), hexb(brightness), hexb(speed), hexb(direction),
        "00", "00", "00", hexb(DELIM_HI), hexb(DELIM_LO)
    ]
    parts += ["00"] * (64 - len(parts))
    return " ".join(parts)

def time_data_packet(y, mo, d, h, mi, s):
    parts = [
        "00", "01", "5A",
        decb(y), decb(mo), decb(d), decb(h), decb(mi), decb(s),
        "00", "04"
    ]
    parts += ["00"] * (62 - len(parts))
    parts += [hexb(DELIM_LO), hexb(DELIM_HI)]
    return " ".join(parts)

def hexstr_to_bytes(s: str) -> bytes:
    return bytes(int(x, 16) for x in s.split())

def find_ff13_hidraw() -> str:
    """Находит hidraw с usage page 0xFF13 в report_descriptor."""
    base = Path("/sys/class/hidraw")
    for d in base.iterdir():
        if not d.is_dir():
            continue
        rd = d / "device" / "report_descriptor"
        if not rd.exists():
            continue
        try:
            data = rd.read_bytes().hex().upper()
            if "0613FF" in data:
                return f"/dev/{d.name}"
        except Exception:
            pass
    return ""

def list_0c45_devices():
    """Возвращает список VID:PID 0C45:XXXX из hidraw uevent."""
    base = Path("/sys/class/hidraw")
    found = set()
    for d in base.iterdir():
        ue = d / "device" / "uevent"
        if not ue.exists():
            continue
        try:
            txt = ue.read_text()
            for line in txt.splitlines():
                if line.startswith("HID_ID="):
                    hid_id = line.split("=")[1]
                    if hid_id.startswith("0003:00000C45:"):
                        vid = "0C45"
                        pid = hid_id.split(":")[-1]
                        found.add(f"{vid}:{pid}")
        except Exception:
            pass
    return sorted(found)

def send_feature(dev: hid.device, pkt_hex: str):
    data = hexstr_to_bytes(pkt_hex)
    try:
        dev.send_feature_report(data)
        return True, None
    except Exception as e:
        return False, str(e)

def get_feature(dev: hid.device, report_id: int, length: int) -> bytes:
    try:
        # hidapi get_feature_report ожидает буфер с report_id в первом байте
        buf = bytearray(length + 1)
        buf[0] = report_id
        dev.get_feature_report(buf)
        return bytes(buf)
    except Exception as e:
        return b"ERROR: " + str(e).encode()

def ask_yn(prompt: str, default_no: bool = True) -> bool:
    if default_no:
        prompt += " [y/N] "
    else:
        prompt += " [Y/n] "
    try:
        ans = input(prompt).strip().lower()
        if default_no:
            return ans in ("y", "yes", "да")
        return ans not in ("n", "no", "нет")
    except (EOFError, KeyboardInterrupt):
        return False

def test_transport(dev, framing):
    hdr(f"T1: Транспорт feature-send (framing {framing})")
    pkt = ctl_packet(CMD_START, 0x00, 0x01)
    framed = frame_pkt(pkt, framing)
    print(f"  Пакет: {framed}")
    ok_send, err = send_feature(dev, framed)
    if ok_send:
        ok(f"framing {framing} — feature-send ПРОШЁЛ")
        # GET_REPORT 0x04 (хендшейк)
        resp = get_feature(dev, 0x04, 64)
        if resp and not resp.startswith(b"ERROR"):
            ok(f"GET_REPORT 0x04: {resp.hex().upper()}")
        else:
            warn(f"GET_REPORT 0x04: {resp.decode() if isinstance(resp, bytes) else resp}")
    else:
        fail(f"framing {framing} — feature-send ОТКЛОНЁН: {err}")
    return ok_send

def test_lighting(dev, framing, final_cmd, label, r, g, b, brightness, speed):
    hdr(f"T2: Подсветка {label} (framing={framing}, final={hex(final_cmd)})")
    send_feature(dev, frame_pkt(ctl_packet(CMD_START, 0x00, 0x01), framing))
    send_feature(dev, frame_pkt(ctl_packet(CMD_MODE, 0x00, 0x01), framing))
    mode_pkt = mode_data_packet(0x01, r, g, b, brightness, speed)  # Static=0x01
    send_feature(dev, frame_pkt(mode_pkt, framing), hs=False)
    send_feature(dev, frame_pkt(ctl_packet(final_cmd, 0x00, 0x01), framing))
    time.sleep(1)

def test_time_sync(dev, framing):
    hdr(f"T4: Синхронизация времени (framing={framing})")
    now = datetime.now()
    y, mo, d = now.year() % 100, now.month, now.day
    h, mi, s = now.hour, now.minute, now.second
    fmt = f"{h:02d}:{mi:02d}:{s:02d}"
    print(f"  Время: {fmt} (десятичное, не BCD)")

    send_feature(dev, frame_pkt(ctl_packet(CMD_START, 0x00, 0x01), framing))
    send_feature(dev, frame_pkt(ctl_packet(CMD_TIME, 0x00, 0x01), framing))
    tp = time_data_packet(y, mo, d, h, mi, s)
    send_feature(dev, frame_pkt(tp, framing), hs=False)
    send_feature(dev, frame_pkt(ctl_packet(CMD_SAVE, 0x00, 0x00), framing))

def main():
    parser = argparse.ArgumentParser(description="Ajazz AK820 Pro диагностика (0xFF13)")
    parser.add_argument("--device", default="0C45:8009", help="VID:PID (default: 0C45:8009)")
    parser.add_argument("--framing", choices=["a", "b"], default="b", help="framing: a=64б, b=65б (default: b)")
    parser.add_argument("--write-tests", action="store_true", help="включить write-тесты (подсветка/время)")
    parser.add_argument("--auto", action="store_true", help="не задавать вопросы, пропускать write-тесты")
    parser.add_argument("--list", action="store_true", help="показать найденные 0C45:XXXX и hidraw узлы")
    args = parser.parse_args()

    print("Keyboard-Center — автономная диагностика Ajazz AK820 Pro (0xFF13)")
    print(f"Ищем устройство: {args.device}")

    # список 0C45 устройств
    devices = list_0c45_devices()
    if devices:
        print(f"Найдено 0C45: {', '.join(devices)}")
    else:
        warn("Устройства 0C45:XXXX не найдены в /sys/class/hidraw")

    if args.list:
        # показать все hidraw с дескрипторами
        base = Path("/sys/class/hidraw")
        for d in base.iterdir():
            rd = d / "device" / "report_descriptor"
            if rd.exists():
                try:
                    desc = rd.read_bytes().hex().upper()[:64]
                    print(f"  /dev/{d.name}: {desc}...")
                except Exception:
                    pass
        return

    # поиск control-узла 0xFF13
    ctrl_path = find_ff13_hidraw()
    if not ctrl_path:
        fail("control-узел 0xFF13 НЕ найден по report_descriptor")
        print("Проверьте: клавиатура подключена? dmesg | grep -i hidraw")
        sys.exit(1)
    ok(f"control-узел 0xFF13: {ctrl_path}")

    # открыть устройство
    try:
        # hidapi открывает по VID:PID, но нам нужен конкретный hidraw интерфейс
        # используем open_path (Linux-specific)
        dev = hid.device()
        dev.open_path(ctrl_path)
        ok(f"Устройство открыто: {ctrl_path}")
    except Exception as e:
        fail(f"Не удалось открыть {ctrl_path}: {e}")
        sys.exit(1)

    # read-only: GET feature 0x04
    hdr("Read-only: GET feature 0x04")
    resp = get_feature(dev, 0x04, 64)
    if resp and not resp.startswith(b"ERROR"):
        ok(f"GET_REPORT 0x04: {resp.hex().upper()}")
    else:
        warn(f"GET_REPORT 0x04: {resp.decode() if isinstance(resp, bytes) else resp}")

    if not args.write_tests:
        warn("Write-тесты не запущены (нужен флаг --write-tests)")
        print("\nДля запуска write-тестов:")
        print(f"  python3 {sys.argv[0]} --write-tests --framing {args.framing}")
        dev.close()
        return

    if args.auto:
        warn("--auto: write-тесты пропущены (нужен интерактивный режим для визуальной проверки)")
        dev.close()
        return

    # --- Write-тесты ---
    hdr("Write-тесты (визуальная проверка на клавиатуре)")

    # T1: транспорт
    test_transport(dev, args.framing)

    # T2: подсветка
    print("\n--- T2: проверка подсветки ---")
    print("Будут проверены варианты framing/final. Смотрите на клавиатуру!")

    # framing b, final 0xF0 (основной рабочий вариант из журнала 10.112)
    test_lighting(dev, args.framing, CMD_FINISH, "Static FF0000", 0xFF, 0x00, 0x00, 2, 2)
    if ask_yn("Засветилось КРАСНЫМ (Static, яркость 2)?"):
        ok(f"РАБОТАЕТ: framing={args.framing}, final=0xF0")
        print(f"\n✓ ИТОГ: рабочий вариант — framing={args.framing}, FINAL=0xF0")
        # T3: обход эффектов
        for code, name in [(0x07, "Breath"), (0x08, "Spectrum")]:
            if ask_yn(f"Проверить эффект {name} (0x{code:02X})?"):
                test_lighting(dev, args.framing, CMD_FINISH, name, 0xFF, 0x00, 0x00, 2, 2)
        # вернуть статичный приглушённый свет
        test_lighting(dev, args.framing, CMD_FINISH, "Static white dim", 0xFF, 0xFF, 0xFF, 1, 0)
        ok("Клавиатура оставлена в Static белый (яркость 1)")

        # T4: время
        if ask_yn("Проверить синхронизацию времени?"):
            test_time_sync(dev, args.framing)
            if ask_yn("На экране клавиатуры обновилось время?"):
                ok("Синхронизация времени РАБОТАЕТ")
            else:
                warn("Время не обновилось")

        dev.close()
        print("\n✓ Диагностика завершена успешно")
        return

    # интерактивный перебор вариантов
    print("\nИнтерактивный перебор (если основной вариант не сработал):")
    for framing in ["b", "a"]:
        for final, fname in [(CMD_FINISH, "FINISH(0xF0)"), (CMD_SAVE, "SAVE(0x02)")]:
            test_lighting(dev, framing, final, f"Static ({fname})", 0xFF, 0x00, 0x00, 2, 2)
            if ask_yn(f"framing={framing} final={fname} — зажёгся красным?"):
                ok(f"Найден рабочий: framing={framing} final={fname}")
                print(f"\n✓ ИТОГ: framing={framing}, final={fname}")
                dev.close()
                return

    warn("Ни один вариант не зажёг подсветку")
    dev.close()
    sys.exit(1)

if __name__ == "__main__":
    main()