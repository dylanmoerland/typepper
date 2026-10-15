"""Outline "TYPEPPER" in Silkscreen Bold on the pixel chili's grid (1 font pixel = 2 chili pixels).

Usage: python3 wordmark.py Silkscreen-Bold.ttf assets/logo assets/logo/pepper.svg  (needs fonttools)
Font: https://github.com/google/fonts/tree/main/ofl/silkscreen (OFL)
"""
import sys, os, re
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

FONT, OUT, PEPPER_SVG = sys.argv[1:4]
INK, FG_DARK = '#1c1a1e', '#eeeae2'
HEAT = ['#2daf37', '#8cc63f', '#ffd700', '#ff7d00', '#ff4a1e', '#d72319']
UNIT = 125               # Silkscreen draws on a 125-unit pixel grid (cap height = 5 px)
S = 2 / UNIT             # one font pixel → two chili pixels
CAP = 5 * 2              # cap height in chili pixels

font = TTFont(FONT)
gs, cmap = font.getGlyphSet(), font.getBestCmap()

def word(fg, x0=0.0, y0=0.0):
    """Paths for TYPEPPER with baseline at y0 + CAP; returns (svg, width)."""
    out, x = [], 0
    for i, ch in enumerate('TYPEPPER'):
        g = gs[cmap[ord(ch)]]
        pen = SVGPathPen(gs)
        g.draw(TransformPen(pen, (S, 0, 0, -S, x0 + x * S, y0 + CAP)))
        out.append(f'<path fill="{fg if i < 2 else HEAT[i - 2]}" d="{pen.getCommands()}"/>')
        x += g.width
    return ''.join(out), (x - UNIT) * S - 2  # drop trailing side bearing (1 font px) and the leading one

lead = 2  # leading side bearing (1 font px = 2 chili px)
pepper = re.search(r'shape-rendering="crispEdges">(.*)</svg>', open(PEPPER_SVG).read()).group(1)

def svg(w, h, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:g} {h:g}" width="{w:g}" height="{h:g}" '
            f'shape-rendering="crispEdges">{body}</svg>\n')

for name, fg in [('dark', FG_DARK), ('light', INK)]:
    body, w = word(fg, x0=-lead)
    open(os.path.join(OUT, f'wordmark-{name}.svg'), 'w').write(svg(w, CAP, body))
    gap = 4
    body, w = word(fg, x0=16 + gap - lead, y0=(16 - CAP) / 2)
    open(os.path.join(OUT, f'lockup-{name}.svg'), 'w').write(svg(16 + gap + w, 16, pepper + body))
