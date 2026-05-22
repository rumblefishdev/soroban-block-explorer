/**
 * URL-cursor query-param keys for routes that mount multiple paginated
 * sections simultaneously. Each section needs its own key so a single
 * `?cursor=` doesn't clobber the rest.
 *
 * Add new entries here rather than inlining strings — keeps the
 * collision-avoidance contract in one file. Default `cursor` (used by
 * single-section routes) stays inside `useTableUrlState`.
 */
export const CURSOR_PARAMS = {
  /** Liquidity-pool detail — participants section. */
  POOL_PARTICIPANTS: 'cursor_p',
  /** Liquidity-pool detail — transactions section. */
  POOL_TRANSACTIONS: 'cursor_t',
  /** Contract detail — events tab. */
  CONTRACT_EVENTS: 'cursor_e',
  /** Contract detail — invocations tab. */
  CONTRACT_INVOCATIONS: 'cursor_i',
} as const;
