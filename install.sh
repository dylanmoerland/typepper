#!/bin/sh
# typepper installer, served from https://typepper.com/install.sh
# Hands off to the latest release's installer (built by cargo-dist), so this
# short URL never changes between versions.
set -eu
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/leodiegues/typepper/releases/latest/download/typepper-installer.sh | sh
