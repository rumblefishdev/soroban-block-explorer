---
id: '0084'
title: 'Frontend bootstrap: scaffold apps/web and libs/ui with core dependencies'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0039', '0046', '0047', '0077']
tags: [priority-high, effort-small, layer-frontend-shared]
links: []
history:
  - date: 2026-03-27
    status: backlog
    who: fmazur
    note: 'Task created — discovered missing bootstrap step while reviewing frontend backlog'
  - date: 2026-03-27
    status: active
    who: fmazur
    note: 'Activated — prerequisite for all frontend tasks'
---

# Frontend bootstrap: scaffold apps/web and libs/ui with core dependencies

## Summary

Scaffold the frontend application (`apps/web`) and shared UI library (`libs/ui`) within the Nx workspace, and install all core dependencies needed by downstream frontend tasks. This is the foundational setup that every other frontend task assumes already exists.

## Status: Active

**Current state:** Not started.

## Context

Existing frontend tasks (0039, 0046, 0047, 0077) all create files inside `apps/web/` and `libs/ui/`, but no task exists to scaffold these projects or install the required dependencies. Without this bootstrap step, none of the frontend tasks can begin.

## Implementation Plan

### Step 1: Scaffold `apps/web` (React + Vite via Nx)

Use Nx React application generator to create the web app:

- `pnpm nx g @nx/react:app web --bundler=vite --routing=false --style=none`
- Verify `apps/web/` contains `src/main.tsx`, `index.html`, `vite.config.ts`, `project.json`, `tsconfig.json`
- Remove boilerplate/default content from generated files

### Step 2: Scaffold `libs/ui` (shared UI library via Nx)

Use Nx React library generator:

- `pnpm nx g @nx/react:lib ui --bundler=vite --style=none`
- Verify `libs/ui/` contains `src/index.ts`, `project.json`, `tsconfig.json`
- Clean up default generated content

### Step 3: Install core frontend dependencies

Install all packages required by downstream frontend tasks:

- **MUI:** `@mui/material`, `@mui/icons-material`, `@emotion/react`, `@emotion/styled` (task 0077)
- **React Router:** `react-router-dom` (task 0047)
- **TanStack Query:** `@tanstack/react-query`, `@tanstack/react-query-devtools` (task 0046)

### Step 4: Verify workspace integration

- `pnpm nx build web` succeeds
- `pnpm nx build ui` succeeds
- `pnpm nx lint web` passes
- `pnpm nx lint ui` passes
- `apps/web` can import from `libs/ui` via workspace path alias

### Step 5: Minimal app entry point

Set up a bare-bones `apps/web/src/main.tsx` that renders a React root — just enough to confirm the app starts. No routing, no theme, no providers yet (those are separate tasks).

## Acceptance Criteria

- [ ] `apps/web/` exists as an Nx React app with Vite bundler
- [ ] `libs/ui/` exists as an Nx React library
- [ ] MUI packages installed: `@mui/material`, `@mui/icons-material`, `@emotion/react`, `@emotion/styled`
- [ ] React Router installed: `react-router-dom`
- [ ] TanStack Query installed: `@tanstack/react-query`, `@tanstack/react-query-devtools`
- [ ] `pnpm nx build web` and `pnpm nx build ui` succeed
- [ ] `apps/web` can import from `libs/ui`
- [ ] `pnpm nx serve web` starts dev server and renders the app

## Notes

- This task MUST be completed before tasks 0039, 0046, 0047, and 0077 can start.
- Generator flags may need adjustment based on Nx version — check `--help` before running.
- Do not add routing, theme, or providers in this task — keep it minimal. Those are covered by dedicated tasks.
