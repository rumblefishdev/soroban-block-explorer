# SCF Submission — Documents & Build Pipeline

This directory holds the Stellar Community Fund **Deliverable Verification**
package for the Soroban Block Explorer project, plus the reproducible
toolchain that builds it.

## File inventory

| File                       | Role                                                                                   |
| -------------------------- | -------------------------------------------------------------------------------------- |
| `milestone-1-form-text.md` | Short text to paste into each field of the SCF Deliverable Verification web form.      |
| `milestone-1-evidence.md`  | Full evidence companion to the M1 submission video — source of truth.                  |
| `milestone-1-evidence.pdf` | PDF render of the above, attached in Google Drive next to the video. **Build output.** |
| `build-pdf.sh`             | Reproducible PDF render script (`pandoc + typst`).                                     |
| `README.md`                | This file.                                                                             |

The companion video script lives outside the repo at
`~/Downloads/milestone_one_video_scenario/m1-scenario.md`.

## Build the PDF

One-time tooling install (macOS, Homebrew):

```bash
brew install pandoc typst    # core
brew install poppler         # optional — enables pdfinfo page-count check
```

Render:

```bash
./build-pdf.sh
```

Output: `milestone-1-evidence.pdf` (~11 pages, ~2.2 MB). The script also lists
any unresolved `<TODO:>` markers in the source so you remember to fill in
screenshots before the final upload.

### Why this toolchain

- **Typst** as the engine: handles Unicode (`→`, `—`, `✅`, en-dashes,
  monospace ASCII diagrams) natively — no LaTeX font fiddling.
- **GFM input mode** (`--from=gfm+wikilinks_title_after_pipe`): gives
  GitHub-style heading auto-IDs so in-doc anchor links resolve.
- ~10× faster than xelatex for this doc; cold render under 2 s.

## Submission workflow (end-to-end)

```
1. Iterate on  milestone-1-evidence.md  (source of truth).
2. Capture screenshots for every <TODO:> marker (paired with the video
   recording session — same AWS Console views).
3. Replace each <TODO:> with a Markdown image embed:
       ![ECS service running](./screenshots/ecs-service.png)
4. Run  ./build-pdf.sh  to regenerate the PDF.
5. Upload the PDF (+ the recorded video) to a Google Drive folder with
   link-sharing set to "anyone with the link can view".
6. Open  milestone-1-form-text.md  and replace every <ANGLE_BRACKET>
   placeholder (Drive folder link, video URL).
7. Copy the four field blocks into the SCF Deliverable Verification form
   and submit.
```

## Known limitations of the current render

- **Long URLs in tables** (section 6 "Live endpoints and access") wrap
  mid-domain. Acceptable for a reviewer but not pretty. Fix would be a
  small typst template; not blocking.
- **`<TODO: screenshot>` markers** render as inline text. Once images are
  embedded with standard Markdown syntax, typst will lay them out
  inline. Until then they intentionally stay visible as placeholders.

## Reverting / regenerating from scratch

The PDF is a build artifact and is reproducible from the `.md` source. If
the PDF gets out of sync (e.g. someone edits the markdown but forgets to
re-render), just run `./build-pdf.sh` again.

`.gitignore` should treat `milestone-1-evidence.pdf` as either:

- **Committed** — for reviewer ease, ship the PDF in the repo so anyone
  cloning sees the same artifact the SCF reviewer sees. _(Default.)_
- **Ignored** — keep the repo source-only, build on demand. Add
  `docs/scf/*.pdf` to `.gitignore`.
