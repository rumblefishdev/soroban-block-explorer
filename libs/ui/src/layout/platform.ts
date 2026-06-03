/**
 * Platform detection for keyboard-shortcut hints. macOS shows ⌘, others Ctrl.
 * The handler accepts both (`metaKey || ctrlKey`); this only labels the hint.
 */
export const isMac =
  typeof navigator !== 'undefined' &&
  /mac/i.test(
    navigator.userAgent ||
      (navigator as unknown as { platform?: string }).platform ||
      ''
  );

/** Platform-aware label for the global search shortcut. */
export const searchShortcutLabel = isMac ? '⌘ K' : 'Ctrl + K';
