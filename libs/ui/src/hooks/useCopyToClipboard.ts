import { useCallback, useEffect, useRef, useState } from 'react';

const COPIED_DURATION_MS = 1500;

/**
 * Clipboard write with a transient `copied` flag that auto-resets after
 * `durationMs`. Uses the async Clipboard API (guaranteed in any secure
 * context — HTTPS and localhost); a failed write leaves `copied` false.
 * The reset timer is cleared on unmount and on repeated copies. Shared by
 * `CopyButton` and the XDR row.
 */
export function useCopyToClipboard(durationMs = COPIED_DURATION_MS) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const copy = useCallback(
    async (value: string) => {
      try {
        await navigator.clipboard.writeText(value);
      } catch {
        return;
      }
      setCopied(true);
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => setCopied(false), durationMs);
    },
    [durationMs]
  );

  return { copied, copy };
}
