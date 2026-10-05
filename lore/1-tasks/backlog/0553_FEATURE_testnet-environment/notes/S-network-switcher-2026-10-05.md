# S — network switcher (PR E2), decided 2026-10-05

The pill from E #600 gives way to the design's "Mainnet / Testnet tabs"
segmented control. The Figma component set has a testnet variant: amber
(`yellow` 100/500/700) where mainnet is blue (`blue` 100/600).

Decided with the owner:

- **Place.** Beside the logo on every page, home included — the pill's
  place, not the design's spot in the stats bar. Home has no stats bar, and
  the stats bar scrolls away; the marker must not.
- **Where a switch lands.** The other network's list of the section you are
  in (`/transactions/<hash>` → `/transactions`), or home for search and other
  pages. A hash or sequence names another record, or none, on the other
  network. For comparison, stellar.expert always lands on home.
- **Current tab** is a label, not a link.
- **Both themes** show the same pastel chip; a hovered other tab keeps the
  design's dark grey label, which stays legible on the pastel fill.
- Kept as they are: the Ctrl+K search shortcut (the design's home page shows
  it; the stats-bar frame's "CTRL + F" is an inconsistency in the design)
  and US number grouping.
