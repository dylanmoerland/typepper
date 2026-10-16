"""Export typepper's link-preview card (1200x630) as an SVG.

Usage: python3 og.py assets/logo
PNG:   rsvg-convert og.svg -o og.png, with Departure Mono visible to fontconfig
       (it ships in assets/fonts; see the FONTCONFIG_FILE note below)
"""
import os, re, sys

OUT = sys.argv[1]
BG, INK, DIM = '#0b0a0c', '#c8c4cc', '#6e6a74'
# the heat ranks, bell pepper → reaper (matches fx::RANK_RGB)
RANKS = ['#787878', '#469646', '#6ed750', '#ffd700', '#ff8c00', '#ff321e', '#ff00c8']
MODES = [('SPRINT', '#6ed750'), ('BLITZ', '#ffd700'), ('ZEN', '#3cc8be'), ('SURVIVAL', '#ff321e'), ('LAVA', '#ff6e14')]
# Departure Mono is a pixel font on an 11px grid: sizes are multiples of 11.
# Without installing it: FONTCONFIG_FILE=<conf with <dir>assets/fonts</dir>>
FONT = 'font-family="Departure Mono"'

lockup = open(os.path.join(OUT, 'lockup-dark.svg')).read()
inner = re.sub(r'^<svg[^>]*>|</svg>\s*$', '', lockup.strip())
S = 7  # whole-pixel scale keeps the pixel art crisp
w, h = 124 * S, 16 * S

bar_w = 840
seg = bar_w / len(RANKS)
bar = ''.join(f'<rect x="{180 + i * seg:.0f}" y="420" width="{seg - 6:.0f}" height="18" fill="{c}"/>'
              for i, c in enumerate(RANKS))
# explicit offsets: renderers collapse the spaces around the dots
modes = [f'<tspan dx="{0 if i == 0 else 22}" fill="{color}">{name}</tspan>' for i, (name, color) in enumerate(MODES)]
mode_line = f'<tspan dx="22" fill="{DIM}">·</tspan>'.join(modes)

svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
<rect width="1200" height="630" fill="{BG}"/>
<svg x="{(1200 - w) // 2}" y="150" width="{w}" height="{h}" viewBox="0 0 124 16" shape-rendering="crispEdges">{inner}</svg>
<text x="600" y="355" text-anchor="middle" {FONT} font-size="33" fill="{INK}">a spicy little typing game · terminal &amp; browser</text>
{bar}
<text x="600" y="505" text-anchor="middle" {FONT} font-size="33">{mode_line}</text>
<text x="600" y="585" text-anchor="middle" {FONT} font-size="22" fill="{DIM}">typepper.com</text>
</svg>
'''
open(os.path.join(OUT, 'og.svg'), 'w').write(svg)
