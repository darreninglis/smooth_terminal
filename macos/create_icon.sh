#!/usr/bin/env bash
# create_icon.sh — generates macos/AppIcon.icns using only stdlib Python, sips, and iconutil.
# All three are always present on macOS; no third-party tools required.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICONSET_DIR="$SCRIPT_DIR/AppIcon.iconset"
SOURCE_PNG="$SCRIPT_DIR/icon_1024.png"
OUTPUT_ICNS="$SCRIPT_DIR/AppIcon.icns"

echo "→ Generating 1024×1024 source PNG…"
# Pass the output path explicitly via an environment variable so that
# __file__ ambiguity inside a heredoc is not an issue.
ICON_OUT="$SOURCE_PNG" python3 - <<'PYEOF'
import struct, zlib, os, math

out = os.environ["ICON_OUT"]

BG      = (30,  30,  46)   # #1e1e2e  Catppuccin Mocha base
SURFACE = (49,  50,  68)   # #313244  surface0
ACCENT  = (203, 166, 247)  # #cba6f7  mauve
FG      = (205, 214, 244)  # #cdd6f4  text

W = H = 1024

def clamp(v):
    return max(0, min(255, int(v)))

def lerp(a, b, t):
    return tuple(clamp(a[i] + (b[i] - a[i]) * t) for i in range(3))

# ── Radial-gradient background ───────────────────────────────────────────────
pixels = bytearray(W * H * 4)
for y in range(H):
    for x in range(W):
        cx, cy = x - W / 2, y - H / 2
        dist = min(math.sqrt(cx * cx + cy * cy) / (W * 0.55), 1.0)
        r, g, b = lerp(SURFACE, BG, dist)
        idx = (y * W + x) * 4
        pixels[idx], pixels[idx+1], pixels[idx+2], pixels[idx+3] = r, g, b, 255

# ── Rounded-rectangle helper ─────────────────────────────────────────────────
def draw_rrect(pixels, x0, y0, x1, y1, radius, color, alpha=255):
    r, g, b = color
    a = alpha / 255.0
    for y in range(max(y0, 0), min(y1, H)):
        for x in range(max(x0, 0), min(x1, W)):
            cx = max(x0 + radius - x, x - (x1 - radius), 0)
            cy = max(y0 + radius - y, y - (y1 - radius), 0)
            if cx * cx + cy * cy > radius * radius:
                continue
            idx = (y * W + x) * 4
            pixels[idx]   = clamp(pixels[idx]   * (1 - a) + r * a)
            pixels[idx+1] = clamp(pixels[idx+1] * (1 - a) + g * a)
            pixels[idx+2] = clamp(pixels[idx+2] * (1 - a) + b * a)
            pixels[idx+3] = 255

# Screen bezel and inner
draw_rrect(pixels, 140, 200, 884, 824, 60, SURFACE, 200)
draw_rrect(pixels, 160, 220, 864, 804, 48, BG,      230)

# ── Tiny 7×9 pixel font for ">_" ─────────────────────────────────────────────
GLYPHS = {
    '>': [0b1000000, 0b0100000, 0b0010000, 0b0001000,
          0b0010000, 0b0100000, 0b1000000, 0, 0],
    '_': [0, 0, 0, 0, 0, 0, 0b1111111, 0, 0],
}

def draw_glyph(pixels, ch, ox, oy, scale, color):
    r, g, b = color
    for row, bits in enumerate(GLYPHS[ch]):
        for col in range(7):
            if bits & (1 << (6 - col)):
                for sy in range(scale):
                    for sx in range(scale):
                        px, py = ox + col * scale + sx, oy + row * scale + sy
                        if 0 <= px < W and 0 <= py < H:
                            idx = (py * W + px) * 4
                            pixels[idx], pixels[idx+1], pixels[idx+2], pixels[idx+3] = r, g, b, 255

SCALE   = 34
total_w = (7 * 2 + 2) * SCALE
start_x = (W - total_w) // 2
start_y = (H - 9 * SCALE) // 2 + 20
draw_glyph(pixels, '>', start_x,                   start_y, SCALE, ACCENT)
draw_glyph(pixels, '_', start_x + (7 + 2) * SCALE, start_y, SCALE, FG)

# ── Minimal PNG writer (no third-party libs) ─────────────────────────────────
def png_chunk(tag, data):
    return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xFFFFFFFF)

def write_png(path, w, h, rgba):
    raw = bytearray()
    for row in range(h):
        raw.append(0)
        raw.extend(rgba[row * w * 4:(row + 1) * w * 4])
    with open(path, 'wb') as f:
        f.write(b'\x89PNG\r\n\x1a\n')
        f.write(png_chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0)))
        f.write(png_chunk(b'IDAT', zlib.compress(bytes(raw), 9)))
        f.write(png_chunk(b'IEND', b''))

write_png(out, W, H, bytes(pixels))
print(f"  Written: {out}")
PYEOF

echo "→ Building iconset (10 PNG files across 5 sizes)…"
rm -rf "$ICONSET_DIR"
mkdir -p "$ICONSET_DIR"

# macOS iconset standard: logical sizes 16, 32, 128, 256, 512
# Each size needs @1x (SIZE×SIZE) and @2x (2SIZE × 2SIZE) variants.
declare -a LOGICAL=(16 32 128 256 512)
for S in "${LOGICAL[@]}"; do
    D=$((S * 2))
    echo "   ${S}×${S} @1x  →  icon_${S}x${S}.png"
    sips -z "$S" "$S" "$SOURCE_PNG" --out "$ICONSET_DIR/icon_${S}x${S}.png"    > /dev/null
    echo "   ${S}×${S} @2x  →  icon_${S}x${S}@2x.png  (${D}×${D} px)"
    sips -z "$D" "$D" "$SOURCE_PNG" --out "$ICONSET_DIR/icon_${S}x${S}@2x.png" > /dev/null
done

echo "→ Converting iconset → AppIcon.icns…"
iconutil -c icns "$ICONSET_DIR" -o "$OUTPUT_ICNS"

echo "→ Cleaning up temporary files…"
rm -rf "$ICONSET_DIR" "$SOURCE_PNG"

echo "✓ Created: $OUTPUT_ICNS"
