// PROTOTYPE (lore W341, issue #405) — throwaway, never merged.
// Question: how should pool search work — free words, token chips, or both?
// Three variants of the /liquidity-pools search, switched by ?variant=A|B|C
// (dev build only; no ?variant = today's page). Data: every pool of the
// production list (local API on production ClickHouse, 2026-10-08) with the
// issuers' home domains, loaded from /prototype-w341-pools.json. Matching runs
// in the browser; the global-search preview shows what the header dropdown
// would list for the same text.
import {
  Box,
  Button,
  Chip as MuiChip,
  InputAdornment,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
  Typography,
} from '@mui/material';
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { Link, useSearchParams } from 'react-router-dom';

// ---------------------------------------------------------------- data

interface RawLeg {
  t: string | null; // asset_type_name
  c: string | null; // asset_code
  i: string | null; // issuer
  k: string | null; // contract_id
  s: string | null; // symbol
  d?: string | null; // issuer home domain (added by the fixture builder)
}
interface RawPool {
  id: string;
  kind: string;
  fee: string;
  tvl: string | null;
  vol: string | null;
  n: number | null;
  proto: string | null;
  legs: RawLeg[];
}

interface Leg {
  key: string; // asset identity: native | CODE-ISSUER | C…
  label: string;
  domain: string | null;
  address: string | null; // issuer G… or contract C…
  kind: 'native' | 'classic' | 'soroban';
  hay: string; // lowercase code + symbol, matched by substring
}
interface Pool {
  id: string;
  soroban: boolean;
  protocol: string | null;
  fee: string;
  tvl: number | null;
  vol: number | null;
  providers: number | null;
  legs: Leg[];
}

function toLeg(l: RawLeg): Leg {
  if (l.t === 'native') {
    return {
      key: 'native',
      label: 'XLM',
      domain: 'stellar.org',
      address: null,
      kind: 'native',
      hay: 'xlm native lumens',
    };
  }
  const label =
    l.c || l.s || (l.k ? `${l.k.slice(0, 4)}…${l.k.slice(-4)}` : '?');
  const key = l.c && l.i ? `${l.c}-${l.i}` : l.k ?? label;
  return {
    key,
    label,
    domain: l.d ?? null,
    address: l.i ?? l.k,
    kind: l.c && l.i ? 'classic' : 'soroban',
    hay: `${l.c ?? ''} ${l.s ?? ''}`.toLowerCase(),
  };
}

function toPool(p: RawPool): Pool {
  return {
    id: p.id,
    soroban: p.kind !== 'classic',
    protocol: p.proto,
    fee: p.fee,
    tvl: p.tvl == null ? null : Number(p.tvl),
    vol: p.vol == null ? null : Number(p.vol),
    providers: p.n,
    legs: p.legs.map(toLeg),
  };
}

function usePools(): Pool[] | null {
  const [pools, setPools] = useState<Pool[] | null>(null);
  useEffect(() => {
    fetch('/prototype-w341-pools.json')
      .then((r) => r.json())
      .then((raw: RawPool[]) => setPools(raw.map(toPool)));
  }, []);
  return pools;
}

// ---------------------------------------------------------------- matching

const ADDRESS = /^[GCL][A-Z2-7]{55}$/;
const HEX_ID = /^[0-9a-f]{64}$/i;

function words(text: string): string[] {
  return text
    .split(/[\s/,]+/)
    .map((w) => w.trim())
    .filter(Boolean);
}

// A word matches a leg by code/symbol substring, or exactly by its address.
function wordMatchesLeg(word: string, leg: Leg): boolean {
  const w = word.toLowerCase();
  if (ADDRESS.test(word.toUpperCase())) {
    return leg.address?.toUpperCase() === word.toUpperCase();
  }
  return leg.hay.includes(w);
}

// Every condition must hold on a DIFFERENT leg (so "USDC/USDC" needs two USDC
// legs). Tiny bipartite search: at most 4 legs and a few conditions.
function assign(
  conds: Array<(leg: Leg) => boolean>,
  legs: Leg[],
  used = new Set<number>(),
  i = 0
): boolean {
  if (i === conds.length) return true;
  for (let j = 0; j < legs.length; j++) {
    if (used.has(j) || !conds[i](legs[j])) continue;
    used.add(j);
    if (assign(conds, legs, used, i + 1)) return true;
    used.delete(j);
  }
  return false;
}

function byTvl(a: Pool, b: Pool): number {
  return (b.tvl ?? -1) - (a.tvl ?? -1) || (b.vol ?? -1) - (a.vol ?? -1);
}

function exactPool(pools: Pool[], text: string): Pool | undefined {
  const t = text.trim();
  if (!(ADDRESS.test(t.toUpperCase()) || HEX_ID.test(t))) return undefined;
  return pools.find((p) => p.id.toUpperCase() === t.toUpperCase());
}

// Distinct assets across all pools, with their pool count and summed TVL —
// the suggestion list of variants B and C.
interface Asset {
  key: string;
  leg: Leg;
  pools: number;
  tvl: number;
}
function assetIndex(pools: Pool[]): Asset[] {
  const m = new Map<string, Asset>();
  for (const p of pools) {
    for (const leg of p.legs) {
      const a = m.get(leg.key) ?? { key: leg.key, leg, pools: 0, tvl: 0 };
      a.pools += 1;
      a.tvl += p.tvl ?? 0;
      m.set(leg.key, a);
    }
  }
  return [...m.values()].sort((a, b) => b.tvl - a.tvl || b.pools - a.pools);
}

function suggest(assets: Asset[], word: string, exclude: string[]): Asset[] {
  if (!word) return [];
  return assets
    .filter((a) => !exclude.includes(a.key) && wordMatchesLeg(word, a.leg))
    .slice(0, 8);
}

// ---------------------------------------------------------------- display

const usd = (v: number | null) =>
  v == null
    ? '—'
    : v >= 1e6
    ? `$${(v / 1e6).toFixed(2)}M`
    : v >= 1e3
    ? `$${(v / 1e3).toFixed(1)}K`
    : `$${v.toFixed(2)}`;
const short = (a: string) => `${a.slice(0, 4)}…${a.slice(-4)}`;

function LegLine({ leg }: { leg: Leg }) {
  return (
    <Box sx={{ lineHeight: 1.2 }}>
      <Typography component="span" sx={{ fontWeight: 600, fontSize: 14 }}>
        {leg.label}
      </Typography>
      <Typography
        component="span"
        sx={(t) => ({
          fontSize: 12,
          color: t.palette.text.secondary,
          ml: 0.75,
        })}
      >
        {leg.kind === 'native'
          ? 'native'
          : [leg.domain, leg.address && short(leg.address)]
              .filter(Boolean)
              .join(' · ') || 'no issuer domain'}
      </Typography>
    </Box>
  );
}

function PoolTable({ pools, total }: { pools: Pool[]; total: number }) {
  const [shown, setShown] = useState(25);
  useEffect(() => setShown(25), [pools]);
  return (
    <Box>
      <Typography
        sx={(t) => ({
          fontSize: 13,
          color: t.palette.text.secondary,
          px: 2,
          py: 1,
        })}
      >
        {total.toLocaleString('en-US')} pools match · sorted by TVL (the list
        API serves no volume)
      </Typography>
      <Box sx={{ overflowX: 'auto' }}>
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>
                Pool (each leg: code · issuer domain · address)
              </TableCell>
              <TableCell>Type</TableCell>
              <TableCell align="right">Fee</TableCell>
              <TableCell align="right">TVL</TableCell>
              <TableCell align="right">Providers</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {pools.slice(0, shown).map((p) => (
              <TableRow key={p.id} hover>
                <TableCell>
                  <Link
                    to={`/liquidity-pools/${p.id}`}
                    style={{ textDecoration: 'none', color: 'inherit' }}
                  >
                    <Stack spacing={0.25}>
                      {p.legs.map((l, i) => (
                        <LegLine key={i} leg={l} />
                      ))}
                    </Stack>
                  </Link>
                </TableCell>
                <TableCell sx={{ fontSize: 13 }}>
                  {p.soroban
                    ? `Soroban${p.protocol ? ` · ${p.protocol}` : ''}`
                    : 'Classic'}
                </TableCell>
                <TableCell align="right">{p.fee}%</TableCell>
                <TableCell align="right">{usd(p.tvl)}</TableCell>
                <TableCell align="right">{p.providers ?? '—'}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Box>
      {shown < pools.length && (
        <Button sx={{ m: 1 }} onClick={() => setShown(shown + 50)}>
          Show 50 more
        </Button>
      )}
    </Box>
  );
}

function JumpCard({ pool }: { pool: Pool }) {
  return (
    <Paper variant="outlined" sx={{ p: 2, m: 2 }}>
      <Typography sx={{ fontSize: 13, mb: 1 }}>
        Exact pool id — in the real page Enter opens it directly:
      </Typography>
      <Link to={`/liquidity-pools/${pool.id}`}>
        {pool.legs.map((l) => l.label).join(' / ')} → open pool {short(pool.id)}
      </Link>
    </Paper>
  );
}

// What the header search would list for the same text: pools (top 5 by
// TVL) and assets (top 5 by TVL), each with its domain/address.
function GlobalPreview({
  pools,
  assets,
  text,
}: {
  pools: Pool[];
  assets: Asset[];
  text: string;
}) {
  const ws = words(text);
  // The header box has no chips: it always matches the typed words, A's rule.
  const conds = ws.map((w) => (leg: Leg) => wordMatchesLeg(w, leg));
  pools = pools.filter((p) => assign(conds, p.legs)).sort(byTvl);
  if (!ws.length) return null;
  const assetHits =
    ws.length === 1 ? suggest(assets, ws[0], []).slice(0, 5) : [];
  return (
    <Paper
      variant="outlined"
      sx={{
        p: 1.5,
        width: { xs: '100%', lg: 360 },
        boxSizing: 'border-box',
        flexShrink: 0,
        alignSelf: 'flex-start',
      }}
    >
      <Typography sx={{ fontSize: 12, fontWeight: 700, mb: 1 }}>
        Global search dropdown (header) for “{text}”
      </Typography>
      {assetHits.length > 0 && (
        <>
          <Typography
            sx={(t) => ({ fontSize: 11, color: t.palette.text.secondary })}
          >
            ASSETS
          </Typography>
          {assetHits.map((a) => (
            <Box key={a.key} sx={{ py: 0.5 }}>
              <LegLine leg={a.leg} />
              <Typography
                sx={(t) => ({ fontSize: 11, color: t.palette.text.secondary })}
              >
                in {a.pools} pools · {usd(a.tvl)} TVL
              </Typography>
            </Box>
          ))}
        </>
      )}
      <Typography
        sx={(t) => ({ fontSize: 11, color: t.palette.text.secondary, mt: 1 })}
      >
        POOLS
      </Typography>
      {pools.slice(0, 5).map((p) => (
        <Box
          key={p.id}
          sx={{ py: 0.5, display: 'flex', justifyContent: 'space-between' }}
        >
          <Box>
            {p.legs.map((l, i) => (
              <LegLine key={i} leg={l} />
            ))}
          </Box>
          <Typography sx={{ fontSize: 12 }}>{usd(p.tvl)}</Typography>
        </Box>
      ))}
      {pools.length > 5 && (
        <Typography sx={{ fontSize: 12, mt: 0.5 }}>
          All {pools.length.toLocaleString('en-US')} pools →
          /liquidity-pools?q=…
        </Typography>
      )}
    </Paper>
  );
}

function useDebounced(value: string, ms = 150): string {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

function Hint({ children }: { children: ReactNode }) {
  return (
    <Typography
      sx={(t) => ({ fontSize: 12, color: t.palette.text.secondary, mt: 0.5 })}
    >
      {children}
    </Typography>
  );
}

const EXAMPLES = [
  'USDC',
  'XLM/USDC',
  'AQUA XLM USDC',
  'USDC/USDC',
  'SolvBTC',
  'EURC USDC',
];

function Examples({ onPick }: { onPick: (s: string) => void }) {
  return (
    <Stack direction="row" spacing={0.5} sx={{ mt: 1, flexWrap: 'wrap' }}>
      {EXAMPLES.map((e) => (
        <MuiChip
          key={e}
          size="small"
          variant="outlined"
          label={e}
          onClick={() => onPick(e)}
        />
      ))}
    </Stack>
  );
}

// ---------------------------------------------------------------- variant A
// One box, any number of words; each word must match a different leg.

function VariantA({ pools, assets }: { pools: Pool[]; assets: Asset[] }) {
  const [text, setText] = useState('');
  const q = useDebounced(text);
  const exact = exactPool(pools, q);
  const result = useMemo(() => {
    const ws = words(q);
    if (!ws.length) return [...pools].sort(byTvl);
    const conds = ws.map((w) => (leg: Leg) => wordMatchesLeg(w, leg));
    return pools.filter((p) => assign(conds, p.legs)).sort(byTvl);
  }, [pools, q]);
  return (
    <Stack direction={{ xs: 'column', lg: 'row' }} spacing={2}>
      <Paper variant="outlined" sx={{ flex: 1, minWidth: 0 }}>
        <Box sx={{ p: 2 }}>
          <TextField
            fullWidth
            size="small"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Assets, pair or pool id — e.g. XLM USDC, AQUA/XLM/USDC, CAS3…"
          />
          <Hint>
            Words split on space, “/” or “,”. Every word must match a different
            leg (code or symbol contains it; a G…/C… address must equal the
            issuer or contract).
          </Hint>
          <Examples onPick={setText} />
        </Box>
        {exact && <JumpCard pool={exact} />}
        <PoolTable pools={result} total={result.length} />
      </Paper>
      <GlobalPreview pools={pools} assets={assets} text={q} />
    </Stack>
  );
}

// ---------------------------------------------------------------- variant B
// Chips: pick concrete assets from suggestions; the table filters only by chips.

function AssetPicker({
  assets,
  chosen,
  onPick,
  text,
  setText,
  placeholder,
}: {
  assets: Asset[];
  chosen: Asset[];
  onPick: (a: Asset) => void;
  text: string;
  setText: (s: string) => void;
  placeholder: string;
}) {
  const ws = words(text);
  const last = ws[ws.length - 1] ?? '';
  const hits = suggest(
    assets,
    last,
    chosen.map((c) => c.key)
  );
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  return (
    <Box ref={ref} sx={{ position: 'relative' }}>
      <TextField
        fullWidth
        size="small"
        value={text}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && hits[0]) onPick(hits[0]);
        }}
        placeholder={placeholder}
        slotProps={{
          input: {
            startAdornment: chosen.length ? (
              <InputAdornment position="start">
                <Stack direction="row" spacing={0.5}>
                  {chosen.map((c) => (
                    <MuiChip
                      key={c.key}
                      size="small"
                      color="primary"
                      label={`${c.leg.label} · ${
                        c.leg.domain ??
                        (c.leg.address ? short(c.leg.address) : 'native')
                      }`}
                    />
                  ))}
                </Stack>
              </InputAdornment>
            ) : undefined,
          },
        }}
      />
      {open && hits.length > 0 && (
        <Paper
          elevation={6}
          sx={{
            position: 'absolute',
            zIndex: 10,
            left: 0,
            right: 0,
            mt: 0.5,
            maxHeight: 420,
            overflow: 'auto',
          }}
        >
          {hits.map((a) => (
            <Box
              key={a.key}
              onMouseDown={() => onPick(a)}
              sx={(t) => ({
                px: 1.5,
                py: 1,
                cursor: 'pointer',
                display: 'flex',
                justifyContent: 'space-between',
                '&:hover': { bgcolor: t.palette.action.hover },
              })}
            >
              <LegLine leg={a.leg} />
              <Typography
                sx={(t) => ({ fontSize: 12, color: t.palette.text.secondary })}
              >
                {a.pools} pools · {usd(a.tvl)}
              </Typography>
            </Box>
          ))}
        </Paper>
      )}
    </Box>
  );
}

function VariantB({ pools, assets }: { pools: Pool[]; assets: Asset[] }) {
  const [chosen, setChosen] = useState<Asset[]>([]);
  const [text, setText] = useState('');
  const exact = exactPool(pools, text);
  const result = useMemo(() => {
    if (!chosen.length) return [...pools].sort(byTvl);
    const conds = chosen.map((c) => (leg: Leg) => leg.key === c.key);
    return pools.filter((p) => assign(conds, p.legs)).sort(byTvl);
  }, [pools, chosen]);
  return (
    <Stack direction={{ xs: 'column', lg: 'row' }} spacing={2}>
      <Paper variant="outlined" sx={{ flex: 1, minWidth: 0 }}>
        <Box sx={{ p: 2 }}>
          <AssetPicker
            assets={assets}
            chosen={chosen}
            text={text}
            setText={setText}
            onPick={(a) => {
              if (chosen.length < 4) setChosen([...chosen, a]);
              setText('');
            }}
            placeholder={
              chosen.length
                ? 'Add another asset…'
                : 'Pick an asset — type USDC, then choose which one'
            }
          />
          <Stack direction="row" spacing={1} sx={{ mt: 1 }}>
            {chosen.length > 0 && (
              <Button size="small" onClick={() => setChosen([])}>
                Clear chips
              </Button>
            )}
          </Stack>
          <Hint>
            Suggestions are concrete assets (code + issuer domain or address),
            ranked by the TVL of their pools. Only chosen chips filter the
            table; typed text alone does not.
          </Hint>
        </Box>
        {exact && <JumpCard pool={exact} />}
        <PoolTable pools={result} total={result.length} />
      </Paper>
      <GlobalPreview pools={pools} assets={assets} text={text} />
    </Stack>
  );
}

// ---------------------------------------------------------------- variant C
// A's free words, plus optional chips: a picked suggestion pins one exact asset.

function VariantC({ pools, assets }: { pools: Pool[]; assets: Asset[] }) {
  const [chosen, setChosen] = useState<Asset[]>([]);
  const [text, setText] = useState('');
  const q = useDebounced(text);
  const exact = exactPool(pools, q);
  const result = useMemo(() => {
    const conds: Array<(leg: Leg) => boolean> = [
      ...chosen.map((c) => (leg: Leg) => leg.key === c.key),
      ...words(q).map((w) => (leg: Leg) => wordMatchesLeg(w, leg)),
    ];
    if (!conds.length) return [...pools].sort(byTvl);
    return pools.filter((p) => assign(conds, p.legs)).sort(byTvl);
  }, [pools, chosen, q]);
  return (
    <Stack direction={{ xs: 'column', lg: 'row' }} spacing={2}>
      <Paper variant="outlined" sx={{ flex: 1, minWidth: 0 }}>
        <Box sx={{ p: 2 }}>
          <AssetPicker
            assets={assets}
            chosen={chosen}
            text={text}
            setText={setText}
            onPick={(a) => {
              if (chosen.length < 4) setChosen([...chosen, a]);
              // the picked word becomes the chip; the other words stay free text
              const ws = words(text);
              setText(ws.slice(0, -1).join(' '));
            }}
            placeholder="Type assets freely (XLM USDC) — or pick a suggestion to pin one exact asset"
          />
          <Stack direction="row" spacing={1} sx={{ mt: 1 }}>
            {chosen.length > 0 && (
              <Button size="small" onClick={() => setChosen([])}>
                Clear chips
              </Button>
            )}
          </Stack>
          <Hint>
            Free words filter as in A, live. Choosing a suggestion (click or
            Enter) turns the last word into a chip for that exact asset — e.g.
            “USDC” → USDC · centre.io.
          </Hint>
          <Examples onPick={setText} />
        </Box>
        {exact && <JumpCard pool={exact} />}
        <PoolTable pools={result} total={result.length} />
      </Paper>
      <GlobalPreview pools={pools} assets={assets} text={q} />
    </Stack>
  );
}

// ---------------------------------------------------------------- switcher

export const VARIANTS = [
  { key: 'A', name: 'Free words, every word a different leg' },
  { key: 'B', name: 'Token chips from suggestions (Raydium/Orca)' },
  { key: 'C', name: 'Free words + optional chips' },
] as const;

function Switcher({ current }: { current: string }) {
  const [params, setParams] = useSearchParams();
  const idx = Math.max(
    0,
    VARIANTS.findIndex((v) => v.key === current)
  );
  const go = (d: number) => {
    const next = VARIANTS[(idx + d + VARIANTS.length) % VARIANTS.length];
    params.set('variant', next.key);
    setParams(params, { replace: true });
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = document.activeElement;
      if (
        el &&
        (el.tagName === 'INPUT' ||
          el.tagName === 'TEXTAREA' ||
          (el as HTMLElement).isContentEditable)
      )
        return;
      if (e.key === 'ArrowLeft') go(-1);
      if (e.key === 'ArrowRight') go(1);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
  return (
    <Box
      sx={{
        position: 'fixed',
        bottom: 16,
        left: '50%',
        transform: 'translateX(-50%)',
        zIndex: 2000,
        bgcolor: '#111',
        color: '#fff',
        borderRadius: 999,
        px: 1,
        py: 0.5,
        display: 'flex',
        alignItems: 'center',
        gap: 1,
        boxShadow: 6,
        fontSize: 14,
      }}
    >
      <Button
        size="small"
        sx={{ color: '#fff', minWidth: 32 }}
        onClick={() => go(-1)}
      >
        ←
      </Button>
      <span>
        {VARIANTS[idx].key} — {VARIANTS[idx].name}
      </span>
      <Button
        size="small"
        sx={{ color: '#fff', minWidth: 32 }}
        onClick={() => go(1)}
      >
        →
      </Button>
    </Box>
  );
}

export function PoolSearchPrototype({ variant }: { variant: string }) {
  const pools = usePools();
  const assets = useMemo(() => (pools ? assetIndex(pools) : []), [pools]);
  return (
    <>
      {!pools ? (
        <Typography>Loading the pool fixture…</Typography>
      ) : variant === 'B' ? (
        <VariantB pools={pools} assets={assets} />
      ) : variant === 'C' ? (
        <VariantC pools={pools} assets={assets} />
      ) : (
        <VariantA pools={pools} assets={assets} />
      )}
      <Switcher current={variant} />
    </>
  );
}
