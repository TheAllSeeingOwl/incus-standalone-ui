#!/usr/bin/env bash
# Build Incus documentation from source and output to incus-docs-build/
# Idempotent: pass --force to rebuild even if output already exists.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$SCRIPT_DIR/.."
SRC="$ROOT/incus-src"
VENV="$ROOT/.docs-venv"
OUT="$ROOT/incus-docs-build"

if [ "${1:-}" != "--force" ] && [ -f "$OUT/index.html" ] && [ "$(wc -c < "$OUT/index.html")" -gt 1024 ]; then
    echo ">>> Docs already built in $OUT (pass --force to rebuild)"
    exit 0
fi

# ── 1. Clone / update the incus repo (sparse: doc/ only) ─────────────────────
if [ ! -d "$SRC/.git" ]; then
    echo ">>> Cloning incus repo (sparse, doc/ only)..."
    git clone --depth 50 --filter=blob:none --sparse \
        https://github.com/lxc/incus.git "$SRC"
    cd "$SRC"
    git sparse-checkout set doc internal/version
else
    echo ">>> Updating incus repo..."
    cd "$SRC"
    git fetch --depth 50 origin HEAD
    git reset --hard FETCH_HEAD
fi

cd "$ROOT"

# Record the last commit that touched doc/ so build.rs can embed it
git -C "$SRC" log -1 --format="%h %ad" --date=short -- doc/ > "$ROOT/.docs-commit"
echo ">>> Docs last commit: $(cat "$ROOT/.docs-commit")"

cd "$ROOT"

# ── 2. Wire up GOPATH so conf.py can find the incus binary ───────────────────
# conf.py does: incus = os.path.join(go env GOPATH, 'bin', 'incus')
GOPATH_DIR="$ROOT/.gopath"
mkdir -p "$GOPATH_DIR/bin"
INCUS_BIN="$(which incus)"
ln -sf "$INCUS_BIN" "$GOPATH_DIR/bin/incus"
export GOPATH="$GOPATH_DIR"

# ── 3. Create Python venv and install Sphinx dependencies ────────────────────
if [ ! -x "$VENV/bin/python" ]; then
    echo ">>> Creating Python venv..."
    python3 -m venv "$VENV"
fi

echo ">>> Installing Sphinx dependencies..."
"$VENV/bin/pip" install -q --upgrade pip
"$VENV/bin/pip" install -q -r "$SRC/doc/.sphinx/requirements.txt"

# ── 4. Build documentation ───────────────────────────────────────────────────
echo ">>> Building documentation..."
"$VENV/bin/sphinx-build" -b html -q "$SRC/doc" "$OUT"

echo ">>> Documentation built to $OUT"
