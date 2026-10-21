#!/bin/sh
# Records typepper's demo GIFs and videos. The autopilot types (--autopilot),
# --script presses the menu keys on a timer, asciinema records the terminal
# stream with exact timing, and agg renders it in Departure Mono:
#   - GIF at 11px (the font's own pixel grid: crisp at 1x, small) for the README
#   - MP4 at 22px (2x, every font pixel a 2x2 block) for posts and video sites
# Needs: asciinema, agg, ffmpeg, gifsicle (brew install …) and Departure Mono
# installed (assets/fonts, or ~/Library/Fonts). Usage: demo/record.sh [name…]
set -eu
cd "$(dirname "$0")/.."
cargo build --release -q

FONTS="${FONTS:-$HOME/Library/Fonts}"
THEME=000000,c8c4cc,000000,dc2828,3cb43c,e6c800,3a6bd6,dc00c8,00bec8,bebebe,787878,ff6464,78e678,ffe650,6e9bff,ff50e6,50e6f0,f0f0f0
TMP="$(mktemp -d)"
mkdir -p assets/gifs

# record NAME WPM SIZE SCRIPT KEEP: play SCRIPT (timed keys) with the
# autopilot typing from WPM in a SIZE (COLSxROWS) terminal; KEEP is the time
# range of the recording to render, in seconds (e.g. 1.2..14.2)
record() {
  name=$1 wpm=$2 size=$3 script=$4 keep=$5
  # a fresh install: no saved bests or settings from your own games
  data="$(mktemp -d)"
  asciinema rec --headless --overwrite --window-size "$size" \
    -c "env XDG_DATA_HOME=$data ./target/release/typepper --dark --fps 30 --autopilot $wpm --script '$script'" \
    "$TMP/$name.cast" >/dev/null
  render() { # SIZE FPS OUT
    agg --font-dir "$FONTS" --font-family "Departure Mono" --font-size "$1" --fps-cap "$2" \
      --theme "$THEME" --select "$keep" --last-frame-duration 1.5 "$TMP/$name.cast" "$3" >/dev/null
  }
  render 11 20 "$TMP/$name.gif"
  gifsicle -O3 --lossy=20 "$TMP/$name.gif" -o "assets/gifs/$name.gif"
  render 22 30 "$TMP/$name-2x.gif"
  ffmpeg -v error -y -i "$TMP/$name-2x.gif" -movflags +faststart -pix_fmt yuv420p \
    -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2:flags=neighbor" \
    -c:v libx264 -crf 18 -preset slow "assets/gifs/$name.mp4"
  echo "$name: $(du -h "assets/gifs/$name.gif" | cut -f1) gif, $(du -h "assets/gifs/$name.mp4" | cut -f1) mp4"
}

# which to record: all by default, or the names given
want() { [ -z "$ONLY" ] || case " $ONLY " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }
ONLY="$*"

# The menu starts on BLITZ; cards go SPRINT BLITZ ZEN SURVIVAL LAVA PURSUIT.

# hero: a swipe through the carousel, then BLITZ climbing to reaper and fever
want hero && record hero 200 100x45 "2:right 3:left 4:enter 15:esc 15.4:esc" 1.2..14.2

# menu: a slow lap of all six cards (scenery cross-fades, blur, title decode)
want menu && record menu 200 100x45 \
  "1.5:right 3:right 4.5:right 6:right 7.5:right 9:right 10.5:esc" 0.8..10.3

# lava: FLOOR IS LAVA from level 10 with a typist too slow to keep up, so
# words pile up and land, the lava rises, and the run ends cooked
want lava && record lava 30 100x45 \
  "1:right 1.6:right 2.2:right 3:enter 4:right 4.5:right 5:right 6:enter 23:esc 23.4:esc" 3.6..21

# streak: a close-up of a hot run (caret flame, fever trail), then one wrong
# key at the peak for the combo shatter
want streak && record streak 200 90x24 "1:enter 9.5:q 13:esc 13.4:esc" 5..12.5

exit 0
