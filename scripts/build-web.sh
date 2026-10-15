#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
RSI_VERSION_NAME="$(python3 -c 'import json; v=json.load(open("versions.json"))[-1]; print("v{}-{}".format(v["number"], v["name"]))')"
export RSI_VERSION_NAME
cargo build --locked --release --lib --target wasm32-unknown-unknown
mkdir -p dist
cp web/index.html web/engine-worker.js web/sw.js web/manifest.webmanifest web/icon.svg web/apple-touch-icon.png dist/
cp target/wasm32-unknown-unknown/release/shogi_rsi.wasm dist/
python3 - <<'PY'
import json, os
from pathlib import Path

page = Path("dist/index.html")
page.write_text(page.read_text().replace("__RSI_VERSION_NAME__", os.environ["RSI_VERSION_NAME"]))
PY
python3 - <<'PY'
from hashlib import sha256
from pathlib import Path

dist = Path("dist")
assets = ["index.html", "engine-worker.js", "sw.js", "manifest.webmanifest", "icon.svg", "apple-touch-icon.png", "shogi_rsi.wasm"]
digest = sha256()
for name in assets:
    digest.update(name.encode())
    digest.update((dist / name).read_bytes())
worker = dist / "sw.js"
worker.write_text(worker.read_text().replace(
    'const CACHE = CACHE_PREFIX + "development";',
    f'const CACHE = CACHE_PREFIX + "{digest.hexdigest()[:16]}";',
))
PY
echo "Static app built in dist/ (serve over HTTPS for offline installation)."
