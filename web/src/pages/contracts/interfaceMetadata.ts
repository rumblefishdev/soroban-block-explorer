/** `outputs` joined, or `void` when empty. */
export function formatReturnType(outputs: string[]): string {
  return outputs.length > 0 ? outputs.join(', ') : 'void';
}
