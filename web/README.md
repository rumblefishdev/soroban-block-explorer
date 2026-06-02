# Web

React SPA for the Soroban Block Explorer frontend.

## Stack

- **React 19** with `react-dom/client` (`createRoot`)
- **Vite** as dev server and production bundler
- **MUI** as base component library and accessibility layer
- **React Router** for client-side routing
- **TanStack Query** for server-state fetching and caching

## Development

```bash
npx nx dev @rumblefish/soroban-block-explorer-web   # dev server on localhost:4200
npx nx build @rumblefish/soroban-block-explorer-web  # production build to dist/
npx nx lint @rumblefish/soroban-block-explorer-web
npx nx typecheck @rumblefish/soroban-block-explorer-web
```

## Structure

```text
web/
  index.html              # SPA entry point
  vite.config.ts          # Vite app config with React plugin
  .env.development        # Local dev API URL (committed)
  .env.example            # Template for new contributors
  src/
    main.tsx              # React root render (StrictMode + QueryProvider)
    app.tsx               # Root App component
    vite-env.d.ts         # Types for import.meta.env
    api/
      index.ts            # Public surface; side-effect-imports client.ts
      client.ts           # Configures the generated @hey-api/client-fetch
      config.ts           # Reads + validates VITE_API_BASE_URL
      QueryProvider.tsx   # QueryClient + ReactQueryDevtools (dev only)
      polling.ts          # Per-resource staleTime / refetchInterval policies
      queryKeys.ts        # invalidateResource() / matchResource() helpers
      hooks/              # Thin wrappers around generated *Options
```

## Environment configuration

The frontend reads `VITE_API_BASE_URL` from `import.meta.env`. Vite loads
`.env.<mode>` files based on the build mode:

- `.env.development` (committed) — used by `vite dev`; points at `http://localhost:9000`.
- `.env.local` / `.env.development.local` (gitignored) — personal overrides.
- **Staging and production builds** — `VITE_API_BASE_URL` is injected by CI/CD
  at build time (e.g. `VITE_API_BASE_URL=https://api.example.com npx nx build`).
  No staging/production URL is committed to the repo; the deployment pipeline
  owns those values.

If `VITE_API_BASE_URL` is missing at build time, [src/api/config.ts](src/api/config.ts)
throws with an explicit error — fail-fast instead of shipping a bundle that
points nowhere.

## Data Layer

API requests, caching, polling and invalidation all flow through TanStack Query.
Page tasks consume hook wrappers from `src/api/hooks/` — they should never call
`fetch` or the generated SDK directly. The hooks compose the auto-generated
`@hey-api/openapi-ts` `@tanstack/react-query` plugin output from `libs/api-types`
with a per-resource cache policy from `src/api/polling.ts`.

Architecture detail: [docs/architecture/frontend/frontend-overview.md §8](../docs/architecture/frontend/frontend-overview.md#8-data-fetching-and-view-state-model).

## Workspace Imports

Uses `@rumblefish/soroban-block-explorer-ui` for shared UI components and
`@rumblefish/api-types` for OpenAPI-derived types, SDK and TanStack Query hooks.
Cross-project imports resolve via the `soroban-block-explorer-source` export condition.
