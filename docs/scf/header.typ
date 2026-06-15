// Typst styling overrides for SCF Milestone 1 evidence PDF.
// Loaded via pandoc `--include-in-header=header.typ`.

// Tables: visible row separators + left-aligned cells + a touch more padding.
#set table(
  stroke: (x, y) => (
    bottom: 0.5pt + rgb("#999999"),
    top: if y == 0 { 0.8pt + black } else { none },
  ),
  inset: (x: 8pt, y: 6pt),
)

// Force left-align on every cell, overriding pandoc's per-cell `align:` arg.
// `set par(justify: false)` disables justified spacing inside cells so
// wrapped lines don't have stretched gaps between words — wrap points stay
// at natural word boundaries and the text remains cleanly copy-pasteable.
// Also shrink inline monospace (backtick-quoted) ~12 % so long URLs and
// identifiers fit cell widths without overflowing into the next column.
#show table.cell: it => {
  set par(justify: false)
  show raw.where(block: false): set text(size: 0.88em)
  align(left + horizon, it)
}

// Header row: bold.
#show table.cell.where(y: 0): set text(weight: "bold")

// Image centering is handled by the Lua filter (see full-width-tables.lua,
// Para handler), which wraps image-only paragraphs in `#align(center)[…]`.
// No typst-side override needed here.

