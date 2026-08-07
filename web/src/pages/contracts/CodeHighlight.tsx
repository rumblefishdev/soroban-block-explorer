import { Box } from '@mui/material';
import Prism from 'prismjs';
import { useMemo } from 'react';

import 'prismjs/components/prism-rust';
import 'prismjs/components/prism-wasm';

import type { Theme } from '@mui/material/styles';

/**
 * Above this source size highlighting is skipped and the caller's plain
 * text renders instead: Prism tokenization is synchronous, and the largest
 * WAT outputs (~600 KB for a 100 KB wasm) would block the main thread for
 * long enough to jank. The threshold comfortably covers every Rust output
 * seen in the task-0465 mainnet sweep (max ~130 KB).
 */
const HIGHLIGHT_LIMIT = 400_000;

/**
 * Token palette mapped onto the explorer theme, following the syntax-color
 * conventions of the Interface tab (`ContractInterface.typeColor`): types
 * blue, literals green, keywords accent. Macros get the warning color on
 * purpose — `todo!()` holes are the honesty signal of the whole feature
 * and should pop out of the reconstructed source.
 */
const tokenSx = (theme: Theme) => ({
  '& .token.comment': {
    color: theme.palette.text.tertiary,
    fontStyle: 'italic',
  },
  '& .token.keyword': { color: theme.palette.text.accent },
  '& .token.string, & .token.char, & .token.number, & .token.boolean': {
    color: theme.palette.text.success,
  },
  '& .token.class-name, & .token.type-definition, & .token.namespace': {
    color: theme.palette.blue[600],
  },
  '& .token.macro, & .token.macro .token.function': {
    color: theme.palette.warning.main,
  },
  '& .token.attribute': { color: theme.palette.text.tertiary },
  '& .token.lifetime-annotation': { color: theme.palette.text.accent },
  '& .token.punctuation, & .token.operator': {
    color: theme.palette.text.secondary,
  },
});

/**
 * Prism-highlighted source (Rust or WAT). Loaded lazily from
 * `ContractCode` so Prism stays out of the main bundle; the Suspense
 * fallback is the same plain text this component degrades to for
 * oversized input.
 */
export default function CodeHighlight({
  source,
  language,
}: {
  source: string;
  language: 'rust' | 'wasm';
}) {
  const html = useMemo(() => {
    if (source.length > HIGHLIGHT_LIMIT) return null;
    const grammar = Prism.languages[language];
    if (grammar == null) return null;
    return Prism.highlight(source, grammar, language);
  }, [source, language]);

  if (html == null) return <>{source}</>;

  // Prism escapes the input during tokenization, so the produced HTML is
  // safe to inject; this is the library's canonical usage.
  return (
    <Box
      component="span"
      sx={tokenSx}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
