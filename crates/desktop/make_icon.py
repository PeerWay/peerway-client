from pathlib import Path

from PIL import Image, ImageDraw

# Canonical 1024x1024 source icon. After changing this file run:
#   cd crates/desktop
#   python make_icon.py
#   npx tauri icon src-tauri/app-icon.png -o src-tauri/icons
#
# The source already includes the transparent margin and rounded-square shape
# used by macOS, so Finder/Dock do not clip the artwork.

S = 2048
F = S / 1024.0
ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "src-tauri" / "app-icon.png"


def px(value: float) -> int:
    return int(round(value * F))


def gradient(size: int) -> Image.Image:
    image = Image.new("RGB", (size, size))
    pixels = image.load()
    stops = [
        (0.0, (31, 80, 239)),
        (0.55, (59, 130, 246)),
        (1.0, (129, 140, 248)),
    ]
    for y in range(size):
        for x in range(size):
            t = (x * 0.5 + y * 0.5) / size
            for index in range(len(stops) - 1):
                start, color_a = stops[index]
                end, color_b = stops[index + 1]
                if start <= t <= end:
                    ratio = (t - start) / (end - start)
                    pixels[x, y] = tuple(
                        int(color_a[i] + (color_b[i] - color_a[i]) * ratio)
                        for i in range(3)
                    )
                    break
    return image


background = Image.new("RGBA", (S, S), (0, 0, 0, 0))
mask = Image.new("L", (S, S), 0)
draw = ImageDraw.Draw(mask)
draw.rounded_rectangle([px(96), px(96), px(928), px(928)], radius=px(185), fill=255)
background.paste(gradient(S), (0, 0), mask)

# Paper-plane outline from site/favicon.svg, scaled into the rounded square.
def plane_point(x: float, y: float) -> tuple[int, int]:
    return (px(96 + x * 13), px(96 + y * 13))


outline = [
    plane_point(49, 15),
    plane_point(38, 46),
    plane_point(32, 32),
    plane_point(18, 26),
    plane_point(49, 15),
]
fold = [plane_point(49, 15), plane_point(30, 34)]
stroke = px(4.5 * 13)
draw = ImageDraw.Draw(background)
draw.line(outline, fill=(255, 255, 255, 255), width=stroke, joint="curve")
draw.line(fold, fill=(255, 255, 255, 255), width=stroke, joint="curve")
radius = stroke / 2
for point in set(outline + fold):
    draw.ellipse(
        [point[0] - radius, point[1] - radius, point[0] + radius, point[1] + radius],
        fill=(255, 255, 255, 255),
    )

background.resize((1024, 1024), Image.LANCZOS).save(SOURCE)
print(f"Source icon written to {SOURCE}")
print("Regenerate platform icons with:")
print("  cd crates/desktop && npx tauri icon src-tauri/app-icon.png -o src-tauri/icons")
