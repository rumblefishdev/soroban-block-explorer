import { useEffect, useState } from 'react';

/**
 * Local-draft-with-debounced-commit pattern for filter inputs.
 *
 * Holds an editable `draft` mirroring the committed `value`, re-syncing
 * whenever `value` changes externally (e.g. a "Clear filters" action or
 * browser back/forward). After the user pauses typing for `delay` ms the
 * draft is committed via `onChange` — so filtering does not refetch on every
 * keystroke. No commit fires while the draft already equals the committed
 * value.
 *
 * Returns the standard `[draft, setDraft]` tuple.
 */
export function useDebouncedDraft<T>(
  value: T,
  onChange: (next: T) => void,
  delay: number
): [T, (next: T) => void] {
  const [draft, setDraft] = useState(value);

  // Re-sync the local draft when the committed value changes externally.
  useEffect(() => {
    setDraft(value);
  }, [value]);

  // Commit the draft after the user pauses, skipping no-op commits.
  useEffect(() => {
    if (draft === value) return;
    const id = setTimeout(() => onChange(draft), delay);
    return () => clearTimeout(id);
  }, [draft, value, onChange, delay]);

  return [draft, setDraft];
}
