#!/usr/bin/env bash
# Régénère tous les assets : timings et niveau → modèles Blender → bruitages.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run -q --bin export_timings > tools/blender/timings.json
for s in player boss weapons hound puppet arena; do
    echo "== $s"
    blender -b --factory-startup -P "tools/blender/$s.py" 2>&1 | grep -E "\[assets\]|Error|Traceback" || true
done
python3 tools/sfx.py
python3 tools/pixel_font.py
