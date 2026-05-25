# H — Security subset (1.17, Wave 3)

Grep-driven over `web/src`, `libs/ui/src`, `libs/api-types/src`. Plus
one live XSS probe via search (see L-3). Scope per audit brief: console
secret leak, XSS sinks, link injection, iframe sandboxing,
localStorage/cookie content, API key handling.

## Findings

### F-H-1 ✓ PASS — Zero `console.*` calls in shipped code

```
grep -rn "console\.(log|warn|error|debug|info)" web/src libs/ui/src
```

Result: zero matches outside `__tests__`. Confirms Wave 1 `P-code-quality`
note. No risk of secret leak via console.

### F-H-2 ✓ PASS — Zero `dangerouslySetInnerHTML` / `innerHTML =` / `eval(` / `new Function(`

```
grep -rn "dangerouslySetInnerHTML|innerHTML\s*=|eval\(|new Function\(" web/src libs/ui/src
```

Result: zero matches. No XSS sinks in the codebase.

### F-H-3 ✓ PASS — Live XSS probe escaped

See `L-search-functional.md` F-L-3. `/search?q=<script>alert(1)</script>`
rendered the literal text inside a `<span>`, no script tag injected, no
alert dialog. React JSX default escaping confirmed working.

### F-H-4 ✓ PASS — Link injection guard via `safeHttpUrl`

`web/src/pages/url.ts` exports `safeHttpUrl(url)` that whitelists only
`http:` / `https:` URLs and returns `null` for `javascript:` or any
other scheme.

Used at:
- `web/src/pages/assets/AssetIcon.tsx:22` — `src` for asset icon image
  (TOML-sourced, attacker-controlled).
- `web/src/pages/assets/AssetMetadata.tsx:31` — `href` for asset
  homepage (TOML-sourced).

Both attacker-controlled inputs gated. Good.

### F-H-5 ✓ PASS — Only one `target="_blank"`, has `rel="noopener noreferrer"`

```
grep -rn "target=\"_blank\"" web/src libs/ui/src
```

Single hit: `web/src/pages/assets/AssetMetadata.tsx:37`. Adjacent line 38:
`rel="noopener noreferrer"`. Correct.

### F-H-6 ✓ PASS — Zero `<iframe` in FE code

```
grep -rn "<iframe" web/src libs/ui/src
```

Result: zero matches. NFT media currently not rendered via iframe (would
need sandbox attribute when added — note for future).

### F-H-7 ✓ PASS — `localStorage` usage minimal + non-sensitive

```
grep -rn "localStorage\." web/src libs/ui/src
```

Result: 2 matches, both in `libs/ui/src/theme/ThemeProvider.tsx:33,63`.
Stores user color-mode preference ('light' | 'dark') under
`soroban-explorer.color-mode`. Try/catch wraps both reads + writes so
private-mode storage failures don't crash. No PII, no secrets.

### F-H-8 ✓ PASS — Zero `sessionStorage` usage

```
grep -rn "sessionStorage\." web/src libs/ui/src
```

Result: zero matches.

### F-H-9 ✓ PASS — Zero `document.cookie` usage

```
grep -rn "document\.cookie" web/src libs/ui/src
```

Result: zero matches. No cookie state managed by FE.

### F-H-10 ✓ PASS — Authorization / Bearer / x-api-key only in generated SDK

```
grep -rn "Authorization|Bearer|x-api-key" web/src libs/ui/src libs/api-types/src
```

Hits:
- `libs/api-types/src/generated/core/auth.gen.ts:15,34` — generic SDK
  helper, `@default 'Authorization'`, `Bearer ${token}` (template).
- `libs/api-types/src/generated/client/utils.gen.ts:151` — generated
  client utility.

Both inside generated SDK; **no application code wires a token in**.
The explorer is read-only public API; no auth headers ever set. Zero
hardcoded keys.

### F-H-11 ✓ PASS — Env vars constrained + validated

```
grep -rn "import\.meta\.env" web/src libs/ui/src
```

Result: 2 matches.
- `web/src/api/config.ts:1` reads `VITE_API_BASE_URL`, throws if unset,
  validates URL parses, strips trailing `/`.
- `web/src/api/QueryProvider.tsx:28` `import.meta.env.DEV` gate for
  React Query devtools (already confirmed Wave 1 F-AI-5).

No `VITE_*` echoed to user UI or logs.

### F-H-12 🟢 LOW `[Class D, Severity LOW]` — Color-mode storage key naming inconsistency

`soroban-explorer.color-mode` localStorage key uses dot-separated
naming. Project elsewhere uses kebab-case (route paths) or camelCase
(JS). Cosmetic. Could be `sbe.color-mode` or `sbe:theme` for shorter +
more conventional namespace prefix. Catalog-only.

## Class breakdown for H (Wave 3 1.17)

| Class | Count |
|---|---:|
| A | 0 |
| B | 0 |
| C | 0 |
| D — catalog-only | 1 (H-12) |
| E — off-band | 0 |
| ✓ pass | 11 |

## Severity breakdown

| Severity | Count |
|---|---:|
| 🔴 CRITICAL | 0 |
| 🟠 HIGH | 0 |
| 🟡 MEDIUM | 0 |
| 🟢 LOW | 1 (H-12) |

Security baseline is **clean** for the scoped subset. No off-band fixes
needed. Real CSP / CSRF / clickjacking / iframe-embed audit is out of
scope for this sub-phase — flagged for future task if NFT iframe media
ever lands.
