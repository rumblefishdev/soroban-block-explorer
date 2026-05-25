export interface MemoDescription {
  typeLabel: string | null;
  content: string | null;
}

const LABELS: Record<string, string> = {
  text: 'Text',
  id: 'ID',
  hash: 'Hash',
  return: 'Return',
};

export function describeMemo(
  memoType: string | null | undefined,
  memo: string | null | undefined
): MemoDescription {
  if (!memoType || memoType === 'none') {
    return { typeLabel: null, content: null };
  }
  return {
    typeLabel: LABELS[memoType] ?? memoType,
    content: memo ?? '',
  };
}
