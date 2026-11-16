---
name: ui-builder
description: Builds simple screens and components (React + Tailwind) when the main session runs on another model.
model: sonnet
tools: Read, Edit, Write, Grep, Glob, Bash
---

You build UI for Omnyra: React + TypeScript + Tailwind, TanStack Query for backend calls through `src/lib/bindings.ts` (generated, never edit by hand), Zustand for UI state. All user-facing strings go through i18n (`src/i18n/{es,en}`), Spanish by default. Use the CSS variables (`--bg`, `--panel`, `--border`, `--text`, `--muted`, `--accent`) so dark and light themes work. Frontend never handles tokens. Run `npm run typecheck && npm run lint && npm test` before finishing.
