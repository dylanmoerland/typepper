"""Export typepper's 16x16 pixel chili as SVGs: pepper, square + rounded app icon.

Usage: python3 icons.py assets/logo
PNGs: rsvg-convert -w 1024 -h 1024 icon-square.svg -o icon-1024.png (etc.)
"""
import sys, os
OUT = sys.argv[1]

PX = ['................', '............GG..', '...........GG...', '...........G....', '.........gGGGg..',
      '.......rRRRRRRr.', '.....rRRRRHHHRr.', '....rRRRRHHRRRr.', '...rRRRRRRRRRr..', '..rRRRRRRRRRr...',
      '.rRRRRRRRrr.....', '.rRRRRRrr.......', 'rRRRRr..........', 'rRRRr...........', '.rRr............', '..r.............']
PC = {'g': '#1f7a2a', 'G': '#2daf37', 'R': '#e0301f', 'r': '#a3160e', 'H': '#ff8a6a'}
INK = '#1c1a1e'

def pixels():
    return ''.join(f'<rect x="{x}" y="{y}" width="1" height="1" fill="{PC[c]}"/>'
                   for y, row in enumerate(PX) for x, c in enumerate(row) if c in PC)

def svg(vb, body, w=None, h=None):
    size = f' width="{w}" height="{h}"' if w else ''
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}"{size} shape-rendering="crispEdges">{body}</svg>\n'

def write(name, text):
    open(os.path.join(OUT, name), 'w').write(text)

# pepper alone, transparent, 16-unit grid (favicons: 16px = 1px/pixel, 32px = 2px/pixel)
write('pepper.svg', svg('0 0 16 16', pixels()))
# app icon: 16 pixels at 48px each inside 1024 → integer pixels at 1024/512/256/128
p = 128 / 48
vb = f'{-p:.4f} {-p:.4f} {16 + 2 * p:.4f} {16 + 2 * p:.4f}'
write('icon-square.svg', svg(vb, f'<rect x="{-p}" y="{-p}" width="{16 + 2 * p}" height="{16 + 2 * p}" fill="{INK}"/>' + pixels()))
write('icon-rounded.svg', svg(vb, f'<rect x="{-p}" y="{-p}" width="{16 + 2 * p}" height="{16 + 2 * p}" rx="{(16 + 2 * p) * 0.22}" fill="{INK}"/>' + pixels()))
