import { Box } from '@mui/material';
import { alpha } from '@mui/material/styles';
import Prism from 'prismjs';
import { useMemo, useState } from 'react';

import 'prismjs/components/prism-rust';
import 'prismjs/components/prism-wasm';

import type { Theme } from '@mui/material/styles';

/**
 * Above this source size highlighting is skipped and plain text renders
 * instead: Prism tokenization is synchronous, and the largest WAT outputs
 * (~600 KB for a 100 KB wasm) would block the main thread for long enough
 * to jank. The threshold comfortably covers every Rust output seen in the
 * task-0465 mainnet sweep (max ~130 KB). Line numbers render regardless.
 */
const HIGHLIGHT_LIMIT = 400_000;

/** Fixed metrics so the gutter, the text and the line-highlight band stay
 *  aligned; `pre` never wraps (horizontal overflow scrolls instead). */
const LINE_HEIGHT = 21;
const PAD_Y = 16;

/**
 * Token palette mapped onto the explorer theme, following the syntax-color
 * conventions of the Interface tab (`ContractInterface.typeColor`): types
 * blue, literals green. Keywords are the theme accent in dark mode, but
 * the accent yellow is illegible on the light surface — light mode uses
 * the violet scale instead. Macros get the warning color on purpose:
 * `todo!()` holes are the honesty signal of the whole feature and should
 * pop out of the reconstructed source.
 */
const tokenSx = (theme: Theme) => {
  const keyword =
    theme.palette.mode === 'light'
      ? theme.palette.violet[600]
      : theme.palette.text.accent;
  return {
    '& .token.comment': {
      color: theme.palette.text.tertiary,
      fontStyle: 'italic',
    },
    '& .token.keyword': { color: keyword },
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
    '& .token.lifetime-annotation': { color: keyword },
    '& .token.punctuation, & .token.operator': {
      color: theme.palette.text.secondary,
    },
  };
};

/**
 * Scrollable source viewer: Prism-highlighted Rust/WAT, a sticky
 * line-number gutter (excluded from text selection so copied code stays
 * clean), and click-a-number line highlighting. Loaded lazily from
 * `ContractCode` so Prism stays out of the main bundle.
 */
export default function CodeHighlight({
  source,
  language,
}: {
  source: string;
  language: 'rust' | 'wasm';
}) {
  const [selectedLine, setSelectedLine] = useState<number | null>(null);

  const html = useMemo(() => {
    if (source.length > HIGHLIGHT_LIMIT) return null;
    const grammar = Prism.languages[language];
    if (grammar == null) return null;
    return Prism.highlight(source, grammar, language);
  }, [source, language]);

  const lineCount = useMemo(() => source.split('\n').length, [source]);

  return (
    <Box
      sx={(theme) => ({
        borderRadius: `${theme.shape.radius.s}px`,
        border: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMainAlt,
        overflow: 'auto',
        maxHeight: 640,
        fontFamily: 'monospace',
        fontSize: 13,
        lineHeight: `${LINE_HEIGHT}px`,
      })}
    >
      <Box sx={{ display: 'flex', minWidth: 'max-content' }}>
        <Box
          aria-hidden
          sx={(theme) => ({
            position: 'sticky',
            left: 0,
            zIndex: 1,
            flexShrink: 0,
            userSelect: 'none',
            textAlign: 'right',
            py: `${PAD_Y}px`,
            px: 1.5,
            color: theme.palette.text.tertiary,
            backgroundColor: theme.palette.surface.grayMainAlt,
            borderRight: `1px solid ${theme.palette.stroke.default}`,
          })}
        >
          {Array.from({ length: lineCount }, (_, i) => i + 1).map((n) => (
            <Box
              key={n}
              onClick={() => setSelectedLine((prev) => (prev === n ? null : n))}
              sx={(theme) => ({
                cursor: 'pointer',
                '&:hover': { color: theme.palette.text.primary },
                ...(n === selectedLine && {
                  color: theme.palette.text.primary,
                }),
              })}
            >
              {n}
            </Box>
          ))}
        </Box>
        <Box
          sx={{
            position: 'relative',
            flex: 1,
            py: `${PAD_Y}px`,
            pl: 1.5,
            pr: 2,
          }}
        >
          {selectedLine != null && (
            <Box
              sx={(theme) => ({
                position: 'absolute',
                left: 0,
                right: 0,
                top: PAD_Y + (selectedLine - 1) * LINE_HEIGHT,
                height: LINE_HEIGHT,
                backgroundColor: alpha(theme.palette.text.accent, 0.14),
                pointerEvents: 'none',
              })}
            />
          )}
          {html == null ? (
            <Box component="pre" sx={{ m: 0, font: 'inherit' }}>
              {source}
            </Box>
          ) : (
            // Prism escapes the input during tokenization, so the produced
            // HTML is safe to inject; this is the library's canonical usage.
            <Box
              component="pre"
              sx={(theme) => ({ m: 0, font: 'inherit', ...tokenSx(theme) })}
              dangerouslySetInnerHTML={{ __html: html }}
            />
          )}
        </Box>
      </Box>
    </Box>
  );
}
