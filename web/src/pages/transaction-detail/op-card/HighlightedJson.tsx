import { Box, Typography } from '@mui/material';
import type { Theme } from '@mui/material/styles';
import {
  addressType,
  IdentifierWithCopy,
  type AddressType,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { Fragment } from 'react';

type TokenKind = 'string' | 'number' | 'bool' | 'null' | 'key';

// Canonical credit-asset form the parser emits: CODE:ISSUER. The asset
// route spells the same identity CODE-ISSUER, so the href maps the colon
// to a dash while the visible text stays as in the JSON (shortened only in a
// narrow column, see `HighlightedJsonProps.narrow`). The bare
// string "native" is NOT linked — without type context the word could be
// any value, and a wrong link is worse than none.
const ASSET_RE = /^([A-Za-z0-9]{1,12}):(G[A-Z2-7]{55})$/;

function AssetString({
  value,
  truncate,
}: {
  value: string;
  truncate: boolean;
}) {
  const match = ASSET_RE.exec(value)!;
  return (
    <Token kind="string" nowrap={truncate}>
      {'"'}
      <IdentifierWithCopy
        value={value}
        type="asset"
        href={`/assets/${match[1]}-${match[2]}`}
        tone="inherit"
        fontSize="inherit"
        truncate={truncate}
        truncation={{ prefix: match[1].length + 5, suffix: 4 }}
        mono
      />
      {'"'}
    </Token>
  );
}

/** A JSON string that IS an identifier renders as the house address
 *  component — clickable link + copy button — while keeping the JSON
 *  string colour and the surrounding quotes (0460 #14). */
function StrkeyString({
  value,
  type,
  truncate,
}: {
  value: string;
  type: AddressType;
  truncate: boolean;
}) {
  return (
    <Token kind="string" nowrap={truncate}>
      {'"'}
      <IdentifierWithCopy
        value={value}
        type={type}
        tone="inherit"
        fontSize="inherit"
        truncate={truncate}
      />
      {'"'}
    </Token>
  );
}

function colorFor(kind: TokenKind, theme: Theme): string {
  switch (kind) {
    case 'string':
      return theme.palette.text.success;
    case 'number':
      return theme.palette.text.accent;
    case 'bool':
    case 'key':
      return theme.palette.blue[600];
    case 'null':
      return theme.palette.text.tertiary;
  }
}

const INDENT = '  ';

function pad(level: number): string {
  return INDENT.repeat(level);
}

/** `nowrap` keeps a shortened identifier on one line with its quotes: it is
 *  one short word, and without it the opening quote stays behind on the line
 *  above. */
function Token({
  kind,
  nowrap = false,
  children,
}: {
  kind: TokenKind;
  nowrap?: boolean;
  children: ReactNode;
}) {
  return (
    <Box
      component="span"
      sx={(theme) => ({
        color: colorFor(kind, theme),
        whiteSpace: nowrap ? 'nowrap' : undefined,
      })}
    >
      {children}
    </Box>
  );
}

function Node({
  value,
  level,
  truncate,
}: {
  value: unknown;
  level: number;
  truncate: boolean;
}): ReactNode {
  if (value === null) return <Token kind="null">null</Token>;
  if (value === undefined) return <Token kind="null">undefined</Token>;
  if (typeof value === 'string') {
    // A muxed `M…` / `W…` links to the account / contract it stands for.
    const type = addressType(value);
    if (type != null)
      return <StrkeyString value={value} type={type} truncate={truncate} />;
    if (ASSET_RE.test(value))
      return <AssetString value={value} truncate={truncate} />;
    return <Token kind="string">{`"${value}"`}</Token>;
  }
  if (typeof value === 'number' || typeof value === 'bigint')
    return <Token kind="number">{String(value)}</Token>;
  if (typeof value === 'boolean')
    return <Token kind="bool">{String(value)}</Token>;

  if (Array.isArray(value)) {
    if (value.length === 0) return '[]';
    return (
      <>
        {'[\n'}
        {value.map((item, i) => (
          <Fragment key={i}>
            {pad(level + 1)}
            <Node value={item} level={level + 1} truncate={truncate} />
            {i < value.length - 1 ? ',' : ''}
            {'\n'}
          </Fragment>
        ))}
        {pad(level)}]
      </>
    );
  }
  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) return '{}';
    return (
      <>
        {'{\n'}
        {entries.map(([k, v], i) => (
          <Fragment key={k}>
            {pad(level + 1)}
            <Token kind="key">{`"${k}"`}</Token>
            {': '}
            <Node value={v} level={level + 1} truncate={truncate} />
            {i < entries.length - 1 ? ',' : ''}
            {'\n'}
          </Fragment>
        ))}
        {pad(level)}
        {'}'}
      </>
    );
  }
  return String(value);
}

interface HighlightedJsonProps {
  value: unknown;
  /** For a narrow table column (issue #735), where a long word takes the width
   *  the cells beside it need. Addresses show as `GBNM…BOSY` and assets as
   *  `USDC:GA5Z…KZVN` (the link and the copy button keep the full value), and
   *  a line breaks between words first, inside a word only when the word
   *  alone does not fit. */
  narrow?: boolean;
}

export function HighlightedJson({
  value,
  narrow = false,
}: HighlightedJsonProps) {
  return (
    <Typography
      component="pre"
      variant="bodyMonoSmMedium"
      sx={(theme) => ({
        m: 0,
        whiteSpace: 'pre-wrap',
        ...(narrow ? { overflowWrap: 'anywhere' } : { wordBreak: 'break-all' }),
        color: theme.palette.text.primary,
      })}
    >
      <Node value={value} level={0} truncate={narrow} />
    </Typography>
  );
}
