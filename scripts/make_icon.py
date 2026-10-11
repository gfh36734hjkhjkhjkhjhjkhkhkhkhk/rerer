"""Рисует иконку Prism (1024x1024): как схема в программе — луч входит в призму и выходит спектром."""
from PIL import Image, ImageDraw, ImageFilter

S = 1024
K = 4                      # рисуем крупнее и уменьшаем для сглаживания
W = S * K
def P(*xy): return [v * K for v in xy]

# Светлая плитка цвета гипса, как фон программы
bg = Image.new("RGBA", (W, W))
d = ImageDraw.Draw(bg)
top, bot = (240, 241, 237), (214, 216, 209)
for y in range(W):
    k = y / W
    d.line([(0, y), (W, y)], fill=tuple(int(top[i] + (bot[i] - top[i]) * k) for i in range(3)) + (255,))
mask = Image.new("L", (W, W), 0)
ImageDraw.Draw(mask).rounded_rectangle(P(64, 64, S - 64, S - 64), radius=230 * K, fill=255)

art = Image.new("RGBA", (W, W), (0, 0, 0, 0))
apex, bl, br = (470, 262), (250, 643), (690, 643)
def on_left(y): t = (y - apex[1]) / (bl[1] - apex[1]); return apex[0] + (bl[0] - apex[0]) * t
def on_right(y): t = (y - apex[1]) / (br[1] - apex[1]); return apex[0] + (br[0] - apex[0]) * t
by = 548
entry = (on_left(by), by)

# Лучи спектра
rays = [((229, 72, 77), 572, (1100, 520)), ((232, 131, 26), 586, (1100, 690)), ((24, 160, 106), 600, (1100, 860)),
        ((28, 149, 218), 614, (1060, 1040)), ((88, 101, 242), 628, (900, 1100))]
glow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
gd, ad = ImageDraw.Draw(glow), ImageDraw.Draw(art)
for col, ey, end in rays:
    ex = on_right(ey)
    gd.line(P(ex, ey, *end), fill=col + (90,), width=70 * K)
    ad.line(P(ex, ey, *end), fill=col + (255,), width=30 * K)
glow = glow.filter(ImageFilter.GaussianBlur(26 * K))

# Входящий луч графитом
beam = Image.new("RGBA", (W, W), (0, 0, 0, 0))
bd = ImageDraw.Draw(beam)
bd.line(P(0, by, *entry), fill=(29, 35, 43, 40), width=60 * K)
beam_glow = beam.filter(ImageFilter.GaussianBlur(22 * K))
ImageDraw.Draw(art).line(P(0, by, *entry), fill=(29, 35, 43, 255), width=30 * K)

# Стекло: полупрозрачная заливка, внутри веер, белая грань
glass = Image.new("RGBA", (W, W), (0, 0, 0, 0))
ImageDraw.Draw(glass).polygon(P(*apex, *br, *bl), fill=(255, 255, 255, 190))
wedge = Image.new("RGBA", (W, W), (0, 0, 0, 0))
wd = ImageDraw.Draw(wedge)
for i, (col, ey, _) in enumerate(rays):
    wd.polygon(P(*entry, on_right(ey - 7), ey - 7, on_right(ey + 7), ey + 7), fill=col + (230,))
edge = Image.new("RGBA", (W, W), (0, 0, 0, 0))
ImageDraw.Draw(edge).line(P(*apex, *br, *bl, *apex, *br), fill=(29, 35, 43, 255), width=26 * K, joint="curve")

shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
ImageDraw.Draw(shadow).polygon(P(apex[0] + 22, apex[1] + 30, br[0] + 22, br[1] + 30, bl[0] + 22, bl[1] + 30), fill=(29, 35, 43, 60))
shadow = shadow.filter(ImageFilter.GaussianBlur(24 * K))
img = Image.alpha_composite(bg, shadow)
for layer in (glow, beam_glow, art, glass, wedge, edge):
    img = Image.alpha_composite(img, layer)
out = Image.new("RGBA", (W, W), (0, 0, 0, 0))
out.paste(img, (0, 0), mask)
out = out.resize((S, S), Image.LANCZOS)
out.save("app-icon.png")
print("app-icon.png готов")
