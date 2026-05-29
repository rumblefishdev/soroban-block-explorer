import KeyboardArrowDownIcon from '@mui/icons-material/KeyboardArrowDown';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import {
  Accordion,
  AccordionDetails,
  AccordionSummary,
  Box,
  Stack,
  Typography,
} from '@mui/material';
import type { Theme } from '@mui/material/styles';
import {
  CardSkeleton,
  classifyError,
  EmptyState,
  GenericErrorState,
  RateLimitState,
  TransientErrorState,
} from '@rumblefish/soroban-block-explorer-ui';

import { useContractInterface } from '../../api/index.js';

import type { ContractFunctionSig } from '@rumblefish/api-types';

import { formatReturnType } from './interfaceMetadata.js';

const INT_TYPE = /^[iu](8|16|32|64|128|256)$/;

/**
 * Blue for reference / struct types (`Address`, `Symbol`, custom). The DS
 * has no `text` blue token, so this mirrors the design-system blue/600 —
 * the same value the Chip `color="blue"` variant renders.
 */
const TYPE_REF_COLOR = '#155dfc';

/**
 * Syntax colour for a Soroban type token, matching the Figma interface
 * panel.
 *
 * - `arg` position: integer types green, `bool` accent-yellow, reference
 *   types (`Address`, `Symbol`, custom structs) blue.
 * - `return` position: everything except `void` is accent-yellow, so the
 *   return value reads as the function's "output highlight" regardless of
 *   the underlying primitive (Figma: `returns i128`, `returns bool` both
 *   render yellow).
 * - `void` / empty always dimmed.
 */
function typeColor(
  theme: Theme,
  type: string,
  position: 'arg' | 'return'
): string {
  if (type === '' || type === 'void') return theme.palette.text.tertiary;
  if (position === 'return') return theme.palette.text.accent;
  if (type === 'bool') return theme.palette.text.accent;
  if (INT_TYPE.test(type)) return theme.palette.text.success;
  return TYPE_REF_COLOR;
}

/** Syntax-coloured type token. */
function TypeTok({
  type,
  position,
}: {
  type: string;
  position: 'arg' | 'return';
}) {
  return (
    <Box
      component="span"
      sx={(theme) => ({ color: typeColor(theme, type, position) })}
    >
      {type}
    </Box>
  );
}

/** One function signature row — expanded by default, like the Figma panel. */
function FunctionRow({ fn }: { fn: ContractFunctionSig }) {
  const params = fn.inputs
    .map((param) => `${param.name}: ${param.type_name}`)
    .join(', ');
  const returnType = formatReturnType(fn.outputs);
  return (
    <Accordion
      defaultExpanded
      disableGutters
      square
      elevation={0}
      sx={(theme) => ({
        backgroundColor: 'transparent',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&::before': { display: 'none' },
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <AccordionSummary
        expandIcon={<KeyboardArrowDownIcon fontSize="small" />}
        sx={{ flexDirection: 'row-reverse', gap: 1 }}
      >
        <Stack
          direction="row"
          spacing={1}
          alignItems="baseline"
          sx={{ minWidth: 0 }}
        >
          <Typography
            component="span"
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {fn.name}
          </Typography>
          <Typography
            component="span"
            variant="bodySmMedium"
            sx={(theme) => ({
              color: theme.palette.text.tertiary,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
            })}
          >
            ({params}) → {returnType}
          </Typography>
        </Stack>
      </AccordionSummary>
      <AccordionDetails>
        <Box
          sx={(theme) => ({
            p: 1,
            borderRadius: '8px',
            border: `1px solid ${theme.palette.stroke.default}`,
            // Inset code block uses the darker surface (Figma).
            backgroundColor: theme.palette.surface.grayMainAlt,
          })}
        >
          {fn.doc !== '' && (
            <Typography
              variant="bodyXsRegular"
              sx={(theme) => ({
                color: theme.palette.text.tertiary,
                mb: 1,
                display: 'block',
              })}
            >
              {fn.doc}
            </Typography>
          )}
          {fn.inputs.length === 0 ? (
            <Typography
              component="div"
              variant="bodyMonoSmMedium"
              sx={(theme) => ({ color: theme.palette.text.tertiary })}
            >
              (no parameters)
            </Typography>
          ) : (
            fn.inputs.map((param, index) => (
              <Typography
                key={`${param.name}-${index}`}
                component="div"
                variant="bodyMonoSmMedium"
                sx={(theme) => ({ color: theme.palette.text.tertiary })}
              >
                {param.name}: <TypeTok type={param.type_name} position="arg" />
              </Typography>
            ))
          )}
          <Box
            sx={(theme) => ({
              borderTop: `1px solid ${theme.palette.stroke.default}`,
              my: 1,
            })}
          />
          <Typography
            component="div"
            variant="bodyMonoSmMedium"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            returns <TypeTok type={returnType} position="return" />
          </Typography>
        </Box>
      </AccordionDetails>
    </Accordion>
  );
}

/**
 * Interface tab — the contract's declared public functions, rendered as a
 * readable accordion list. SAC and pre-upload contracts carry no WASM
 * interface metadata and show an empty state instead.
 */
export function ContractInterface({ contractId }: { contractId: string }) {
  const { data, isLoading, isError, error, refetch } =
    useContractInterface(contractId);

  if (isLoading) {
    return (
      <Box sx={{ p: 2 }}>
        <CardSkeleton />
      </Box>
    );
  }

  if (isError) {
    const kind = classifyError(error);
    const retry = () => void refetch();
    return kind === 'rate-limit' ? (
      <RateLimitState onRetry={retry} />
    ) : kind === 'transient' ? (
      <TransientErrorState onRetry={retry} />
    ) : (
      <GenericErrorState onRetry={retry} />
    );
  }

  // `interface_metadata` is `null` for SAC / pre-upload / stub rows.
  const parsed = data?.interface_metadata ?? null;
  if (parsed == null || parsed.functions.length === 0) {
    return (
      <EmptyState
        icon={<InfoOutlinedIcon fontSize="small" />}
        title="No public interface"
        description="Stellar Asset Contracts and pre-upload contracts expose no WASM interface metadata."
      />
    );
  }

  return (
    <Box>
      {parsed.functions.map((fn, index) => (
        <FunctionRow key={`${fn.name}-${index}`} fn={fn} />
      ))}
    </Box>
  );
}
