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

## Follow-up, same day

- **One sliding control, not two tabs.** The owner wanted a switch you click
  as a whole, with the highlight sliding across — not "select one, the other
  deselects". Four looks were prototyped (branch `proto/0553-switch-style`);
  the owner chose the two names side by side with a sliding highlight. The
  whole control is one link to the other network: the slide plays (220 ms,
  none under reduced motion), then the other site opens. A modified click
  (new tab) is left to the browser.
- **Touch target.** On phones the control is ~31 px tall; an invisible
  margin makes the target ~43 px without changing its look.
- **Accounts and assets do not exist 1:1 on both networks.** Measured on the
  indexed part of testnet (320k of ~4.6M ledgers, 2026-10-05): 1,117 of
  340,525 testnet accounts (0.33%) and 19 of 5,514 testnet assets (0.34%)
  also exist on mainnet. Landing on the section list stays right for them
  too.
