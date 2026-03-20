# Genome Studio

A fully local DNA analysis application for 23andMe raw data. User genomic data **never leaves the machine**.

## Architecture

- **Backend**: Tauri 2.0 + Rust (src-tauri/)
- **Frontend**: React 19 + TypeScript + Vite (src/)
- **Database**: SQLite via rusqlite (WAL mode)
- **Styling**: Tailwind CSS v4 with custom Feltron/Tufte design tokens
- **Visualizations**: D3.js + Observable Plot
- **State**: Zustand

## Commands

```bash
npm run dev          # Start Vite dev server only
npm run tauri dev    # Start full Tauri app (Rust + frontend)
npm run build        # Build frontend
npm run tauri build  # Production build (creates installer)
```

## Project Structure

- `src-tauri/src/` — Rust backend: parser, database, analysis modules, Tauri commands
- `src/` — React frontend: pages, design system, stores, hooks, visualizations
- `src-tauri/migrations/` — SQLite migration SQL files

## Key Conventions

- All heavy computation in Rust, frontend is purely rendering
- IPC via `#[tauri::command]` + `invoke()` from frontend
- Streaming progress via `tauri::ipc::Channel`
- Design: Feltron aesthetics + Tufte data-ink principles (no chartjunk, no icons in nav)
- Color encodes data only — never decorative
- Inter font for UI, JetBrains Mono for genomic data
- Privacy: CSP restricts network to specific public research APIs. No user data ever sent.

## Privacy Rules

- NEVER send user genotype data over the network
- Only public rsID identifiers may be used in API queries (they are public database keys, not personal data)
- All analysis happens locally in Rust
- Research feed is one-way fetch only (GET public APIs, match locally)
- Tauri capabilities whitelist specific domains only
