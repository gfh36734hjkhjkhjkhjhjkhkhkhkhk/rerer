"""Рисует иконку Prism (1024x1024): стеклянная капля с радужной призмой и знаком питания."""
import math
from PIL import Image, ImageDraw, ImageFilter

S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

# Радужный конический градиент
grad = Image.new("RGBA", (S, S))
px = grad.load()
stops = [(0.0, (98, 240, 181)), (0.25, (34, 211, 197)), (0.5, (123, 92, 255)), (0.75, (255, 143, 184)), (1.0, (98, 240, 181))]
def color(t):
    for (a, ca), (b, cb) in zip(stops, stops[1:]):
        if a <= t <= b:
            k = (t - a) / (b - a)
            return tuple(int(ca[i] + (cb[i] - ca[i]) * k) for i in range(3))
    return stops[-1][1]
for y in range(S):
    for x in range(S):
        t = (math.atan2(y - S / 2, x - S / 2) / (2 * math.pi) + 0.5 + 0.3) % 1.0
        px[x, y] = color(t) + (255,)

mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle((64, 64, S - 64, S - 64), radius=230, fill=255)
img.paste(grad, (0, 0), mask)

# Стеклянный блик сверху
hl = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(hl)
d.ellipse((120, 40, S - 120, 560), fill=(255, 255, 255, 90))
hl = hl.filter(ImageFilter.GaussianBlur(40))
img = Image.alpha_composite(img, Image.composite(hl, Image.new("RGBA", (S, S)), mask))

# Знак питания
g = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(g)
w = 64
c = (S // 2, S // 2 + 30)
r = 230
d.arc((c[0] - r, c[1] - r, c[0] + r, c[1] + r), start=-60, end=240, fill=(255, 255, 255, 255), width=w)
d.rounded_rectangle((c[0] - w // 2, c[1] - r - 50, c[0] + w // 2, c[1] - 20), radius=w // 2, fill=(255, 255, 255, 255))
shadow = g.filter(ImageFilter.GaussianBlur(14))
shadow = Image.eval(shadow, lambda v: v)  # тень того же цвета, мягкая
sh = Image.new("RGBA", (S, S), (20, 10, 60, 0))
sh.putalpha(shadow.getchannel("A").point(lambda a: a * 0.35))
img = Image.alpha_composite(img, sh)
img = Image.alpha_composite(img, g)

img.save("app-icon.png")
print("app-icon.png готов")
