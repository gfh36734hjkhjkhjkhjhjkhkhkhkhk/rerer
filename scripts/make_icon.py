"""Рисует иконку Prism (1024x1024): тёмно-серая плитка, белая призма, основание — спектр."""
from PIL import Image, ImageDraw, ImageFilter

S, K = 1024, 4
W = S * K
def P(*xy): return [round(v * K) for v in xy]

# Тёмно-серая плитка с едва заметным вертикальным градиентом
bg = Image.new("RGBA", (W, W))
d = ImageDraw.Draw(bg)
top, bot = (52, 53, 58), (36, 37, 40)
for y in range(W):
    k = y / W
    d.line([(0, y), (W, y)], fill=tuple(int(top[i] + (bot[i] - top[i]) * k) for i in range(3)) + (255,))
mask = Image.new("L", (W, W), 0)
ImageDraw.Draw(mask).rounded_rectangle(P(64, 64, S - 64, S - 64), radius=220 * K, fill=255)

apex, bl, br = (512, 250), (222, 752), (802, 752)
stroke = 62

# Белые грани призмы
edges = Image.new("RGBA", (W, W), (0, 0, 0, 0))
ImageDraw.Draw(edges).line(P(*bl, *apex, *br), fill=(236, 236, 238, 255), width=stroke * K, joint="curve")
for x, y in (bl, apex, br):
    r = stroke / 2
    ImageDraw.Draw(edges).ellipse(P(x - r, y - r, x + r, y + r), fill=(236, 236, 238, 255))

# Основание — спектр
stops = [(255, 97, 87), (255, 164, 58), (245, 213, 71), (71, 209, 127), (59, 167, 245), (142, 123, 247)]
def spec(t):
    t = min(max(t, 0), 1) * (len(stops) - 1)
    i = min(int(t), len(stops) - 2); k = t - i
    return tuple(int(stops[i][c] + (stops[i + 1][c] - stops[i][c]) * k) for c in range(3))
base = Image.new("RGBA", (W, W), (0, 0, 0, 0))
bd = ImageDraw.Draw(base)
x0, x1, y = bl[0], br[0], bl[1]
h = stroke / 2
for xi in range(int(x0 * K), int(x1 * K) + 1):
    t = (xi / K - x0) / (x1 - x0)
    bd.line([(xi, (y - h) * K), (xi, (y + h) * K)], fill=spec(t) + (255,))
cap = Image.new("L", (W, W), 0)
ImageDraw.Draw(cap).rounded_rectangle(P(x0 - h, y - h, x1 + h, y + h), radius=h * K, fill=255)
for xi, t in ((x0 - h, 0), (x1, 1)):
    ImageDraw.Draw(base).rectangle(P(xi, y - h, xi + h, y + h), fill=spec(t) + (255,))
base.putalpha(Image.composite(base.getchannel("A"), Image.new("L", (W, W), 0), cap))
glow = base.filter(ImageFilter.GaussianBlur(30 * K))
glow.putalpha(glow.getchannel("A").point(lambda a: int(a * 0.55)))

img = bg
for layer in (glow, edges, base):
    img = Image.alpha_composite(img, layer)
out = Image.new("RGBA", (W, W), (0, 0, 0, 0))
out.paste(img, (0, 0), mask)
out.resize((S, S), Image.LANCZOS).save("app-icon.png")
print("app-icon.png готов")
