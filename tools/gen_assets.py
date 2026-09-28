#!/usr/bin/env python3
"""生成原创水滴图标和提示音。声音是指数衰减的正弦，没有第三方采样。"""

import math
import struct
import wave
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ICON_DIR = ROOT / "src-tauri" / "icons"
ASSET_DIR = ROOT / "src-tauri" / "assets"


def write_png(path: Path, width: int, height: int, rgba: bytes) -> None:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    raw = b"".join(
        b"\x00" + rgba[y * width * 4 : (y + 1) * width * 4] for y in range(height)
    )
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(png)


def drop_mask(x: float, y: float) -> float:
    """返回 0..1 的覆盖度。坐标系是图标中心附近的归一化平面。"""
    nx = (x - 0.5) / 0.34
    ny = (y - 0.58) / 0.40
    bulb = nx * nx + ny * ny
    # 上半段收成尖角
    tip_y = (0.58 - y) / 0.46
    if y < 0.58:
        half = 0.92 * max(0.0, 1.0 - tip_y) ** 0.72
        width = abs(x - 0.5) / 0.34
        if width <= half and tip_y <= 1:
            return 1.0
    if bulb <= 1.0:
        return 1.0
    return 0.0


def paint(size: int, color: tuple[int, int, int] | None) -> bytes:
    pixels = bytearray(size * size * 4)
    for y in range(size):
        for x in range(size):
            coverage = drop_mask((x + 0.5) / size, (y + 0.5) / size)
            # 简单抗锯齿：再采四个角
            if 0 < coverage < 1 or True:
                samples = [coverage]
                for ox, oy in ((0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)):
                    samples.append(drop_mask((x + ox) / size, (y + oy) / size))
                coverage = sum(samples) / len(samples)
            if coverage <= 0:
                continue
            if color is None:
                t = (y / size - 0.2) / 0.7
                t = max(0.0, min(1.0, t))
                r = int(140 + (14 - 140) * t)
                g = int(236 + (110 - 236) * t)
                b = int(214 + (104 - 214) * t)
            else:
                r, g, b = color
            highlight = 0.0
            hx = (x / size - 0.40) / 0.12
            hy = (y / size - 0.48) / 0.16
            if hx * hx + hy * hy < 1 and color is None:
                highlight = (1 - (hx * hx + hy * hy)) * 0.55
            r = min(255, int(r + (255 - r) * highlight))
            g = min(255, int(g + (255 - g) * highlight))
            b = min(255, int(b + (255 - b) * highlight))
            i = (y * size + x) * 4
            pixels[i : i + 4] = bytes((r, g, b, int(255 * coverage)))
    return bytes(pixels)


def write_wav(path: Path) -> None:
    rate = 44100
    duration = 0.72
    count = int(rate * duration)
    frames = bytearray()
    for i in range(count):
        t = i / rate

        def tone(freq: float, start: float, decay: float, gain: float) -> float:
            if t < start:
                return 0.0
            x = t - start
            return math.sin(2 * math.pi * freq * x) * math.exp(-decay * x) * gain

        sample = (
            tone(988, 0.0, 7.5, 0.46)
            + tone(1480, 0.018, 9.5, 0.22)
            + tone(742, 0.0, 5.5, 0.16)
        )
        if t < 0.012:
            sample += (1 - t / 0.012) * math.sin(2 * math.pi * 2100 * t) * 0.08
        sample = max(-1.0, min(1.0, sample))
        frames += struct.pack("<h", int(sample * 32767))
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "w") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(rate)
        handle.writeframes(frames)


def main() -> None:
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    for size, name in (
        (32, "32x32.png"),
        (128, "128x128.png"),
        (256, "128x128@2x.png"),
        (512, "icon.png"),
        (1024, "icon-source.png"),
    ):
        write_png(ICON_DIR / name, size, size, paint(size, None))
    write_png(ICON_DIR / "trayTemplate.png", 32, 32, paint(32, (0, 0, 0)))
    write_wav(ASSET_DIR / "drop.wav")
    print(f"wrote icons to {ICON_DIR} and {ASSET_DIR / 'drop.wav'}")


if __name__ == "__main__":
    main()
