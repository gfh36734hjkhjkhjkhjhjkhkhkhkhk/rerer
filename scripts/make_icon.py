"""Рисует иконку Prism (1024x1024): чёрная плитка и белый контур треугольника. Без цвета."""
from PIL import Image, ImageDraw

S, K = 1024, 4
W = S * K
def P(*xy): return [round(v * K) for v in xy]

tile = Image.new("RGBA", (W, W), (0, 0, 0, 0))
d = ImageDraw.Draw(tile)
d.rounded_rectangle(P(64, 64, S - 64, S - 64), radius=220 * K, fill=(0, 0, 0, 255))

apex, bl, br = (512, 262), (232, 748), (792, 748)
stroke = 64
d.line(P(*bl, *apex, *br, *bl, *apex), fill=(255, 255, 255, 255), width=stroke * K, joint="curve")
for x, y in (bl, apex, br):
    r = stroke / 2
    d.ellipse(P(x - r, y - r, x + r, y + r), fill=(255, 255, 255, 255))

tile.resize((S, S), Image.LANCZOS).save("app-icon.png")
print("app-icon.png готов")
