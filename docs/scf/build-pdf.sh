#!/usr/bin/env bash
# Build a milestone evidence PDF from milestone-N-evidence.md.
#
# Usage:
#   ./build-pdf.sh        # milestone 1 (default, kept for backwards compat)
#   ./build-pdf.sh 3      # milestone 3
#
# Requirements (install via Homebrew on macOS):
#   brew install pandoc typst
#   brew install poppler    # optional, for `pdfinfo` page-count check below
#
# Output:
#   docs/scf/milestone-<N>-evidence.pdf
#
# Why pandoc + typst?
#   - Typst (the engine) handles Unicode natively — no LaTeX font fiddling
#     for —, →, ✅, etc.
#   - Faster than xelatex by ~10×; cold render of this doc is under 2 s.
#   - Pandoc 3.1+ ships native `--pdf-engine=typst` support.
#   - GFM input mode gives GitHub-style heading auto-ids, so the in-doc
#     anchor links (`[section 4](#4-...)`) resolve.

set -euo pipefail

cd "$(dirname "$0")"

M="${1:-1}"
SRC="milestone-${M}-evidence.md"
OUT="milestone-${M}-evidence.pdf"

# ---- Tool checks ---------------------------------------------------------
command -v pandoc >/dev/null || { echo "❌ pandoc not found — brew install pandoc"; exit 1; }
command -v typst  >/dev/null || { echo "❌ typst not found  — brew install typst";  exit 1; }
[[ -f "$SRC" ]]               || { echo "❌ source not found: $SRC";                 exit 1; }

# ---- Render --------------------------------------------------------------
echo "→ Rendering $SRC → $OUT"

pandoc "$SRC" -o "$OUT" \
    --pdf-engine=typst \
    --from=gfm+wikilinks_title_after_pipe+attributes+yaml_metadata_block \
    --include-in-header=header.typ \
    --lua-filter=full-width-tables.lua \
    -V linkcolor:0066CC \
    -V urlcolor:0066CC

# ---- Sanity ---------------------------------------------------------------
if command -v pdfinfo >/dev/null; then
    PAGES=$(pdfinfo "$OUT" | awk '/^Pages:/ {print $2}')
    SIZE=$(du -h "$OUT" | awk '{print $1}')
    echo "✓ Built $OUT — $PAGES pages, $SIZE"
else
    SIZE=$(du -h "$OUT" | awk '{print $1}')
    echo "✓ Built $OUT — $SIZE (install poppler for page count)"
fi

# ---- Post-build reminders -------------------------------------------------
TODOS=$(grep -c '<TODO:' "$SRC" || true)
if [[ "$TODOS" -gt 0 ]]; then
    echo ""
    echo "⚠  $TODOS unresolved <TODO:> markers in $SRC."
    echo "   Capture screenshots and replace before final upload to Drive:"
    grep -n '<TODO:' "$SRC" | sed 's/^/     /'
fi
