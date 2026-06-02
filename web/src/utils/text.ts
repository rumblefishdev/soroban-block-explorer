/** Upper-cases the first character, leaving the rest unchanged. Empty → `''`. */
export function capitalize(value: string): string {
  return value.length > 0 ? value[0].toUpperCase() + value.slice(1) : value;
}
