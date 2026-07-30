import { Box, Typography } from '@mui/material';
import type { Theme } from '@mui/material/styles';
import { IdentifierWithCopy } from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { Fragment } from 'react';

type TokenKind = 'string' | 'number' | 'bool' | 'null' | 'key';

// Strkey shape (SEP-23): G = account, C = contract, L = liquidity pool —
// 56 chars of base32. Muxed M-addresses have no detail route; leave them
// as plain strings.
const STRKEY_RE = /^[GCL][A-Z2-7]{55}$/;

function strkeyType(value: string): 'account' | 'contract' | 'pool' {
  if (value.startsWith('C')) return 'contract';
  if (value.startsWith('L')) return 'pool';
  return 'account';
}

/** A JSON string that IS an identifier renders as the house address
 *  component — clickable link + copy button — while keeping the JSON
 *  string colour and the surrounding quotes (0460 #14). */
function StrkeyString({ value }: { value: string }) {
  return (
    <Token kind="string">
      {'"'}
      <IdentifierWithCopy
        value={value}
        type={strkeyType(value)}
        tone="inherit"
        fontSize="inherit"
        truncate={false}
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

function Token({ kind, children }: { kind: TokenKind; children: ReactNode }) {
  return (
    <Box component="span" sx={(theme) => ({ color: colorFor(kind, theme) })}>
      {children}
    </Box>
  );
}

function Node({ value, level }: { value: unknown; level: number }): ReactNode {
  if (value === null) return <Token kind="null">null</Token>;
  if (value === undefined) return <Token kind="null">undefined</Token>;
  if (typeof value === 'string')
    return STRKEY_RE.test(value) ? (
      <StrkeyString value={value} />
    ) : (
      <Token kind="string">{`"${value}"`}</Token>
    );
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
            <Node value={item} level={level + 1} />
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
            <Node value={v} level={level + 1} />
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
}

export function HighlightedJson({ value }: HighlightedJsonProps) {
  return (
    <Typography
      component="pre"
      variant="bodyMonoSmMedium"
      sx={(theme) => ({
        m: 0,
        whiteSpace: 'pre-wrap',
        wordBreak: 'break-all',
        color: theme.palette.text.primary,
      })}
    >
      <Node value={value} level={0} />
    </Typography>
  );
}
