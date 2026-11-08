from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

W, H = 660, 400
ROOT = Path(__file__).resolve().parent
OUT = ROOT / "src-tauri" / "dmg" / "background.png"


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "segoeuib.ttf" if bold else "segoeui.ttf"
    return ImageFont.truetype(str(Path("C:/Windows/Fonts") / name), size)


image = Image.new("RGB", (W, H), "#f8fafc")
draw = ImageDraw.Draw(image)

# Soft background circles.
draw.ellipse((520, -80, 760, 120), fill="#e8efff")
draw.ellipse((-80, 300, 120, 500), fill="#e6f7f4")

# Title.
title = "PeerWay"
title_font = font(46, bold=True)
box = draw.textbbox((0, 0), title, font=title_font)
draw.text(((W - (box[2] - box[0])) / 2, 62), title, fill="#111827", font=title_font)

# Version note.
note = "macOS 12+"
note_font = font(15)
box = draw.textbbox((0, 0), note, font=note_font)
draw.text(((W - (box[2] - box[0])) / 2, 360), note, fill="#6b7280", font=note_font)

# Dashed arrow between the app and Applications icons.
y = 190
for x in range(245, 385, 22):
    draw.line((x, y, min(x + 12, 385), y), fill="#2563eb", width=4)
draw.polygon([(410, y), (388, y - 14), (388, y + 14)], fill="#2563eb")

image.save(OUT)
print(f"DMG background written to {OUT}")
