# Arkived Desktop App

Stage 3 scaffold: Tauri 2 + Vite + React + TypeScript. The UI is a pixel-ported
version of the design prototype in `.design-source/`, now with a `StorageBackend`-style
stub IPC layer in `src-tauri/src/commands.rs` that will eventually delegate to
`arkived-core`.

## Status

**Pre-release.** Destructive-op policy flow, real Azure wiring, MCP/ACP surfaces —
all deferred. What works today:

- Full desktop chrome (title bar, sidebar, tabs, blob table, inspector, agent panel, command palette, activity bar, status bar, destructive-action modal)
- Keyboard shortcuts: `⌘K` palette, `⌘J` agent, `⌫` delete, `Esc` close overlays
- Stub Tauri IPC commands (`list_blobs`, `list_activities`, …) returning mock data
- Frontend falls back to inline mock data when running in a plain browser

## Develop

```bash
cd app
npm install
npm run tauri:dev    # full Tauri desktop dev
# or
npm run dev          # frontend-only, opens at http://localhost:1420
```

## Build

```bash
npm run build            # typecheck + vite build → dist/
npm run tauri:build      # full desktop bundle (requires icons/)
```

## Layout

```
app/
├── package.json           # frontend deps + scripts
├── index.html             # Vite entry
├── src/
│   ├── main.tsx           # React mount
│   ├── App.tsx            # App shell, keyboard shortcuts, accent tokens
│   ├── styles.css         # design tokens (dark, rust-orange, JetBrains Mono)
│   ├── icons.tsx          # inline SVG icons
│   ├── data.ts            # mock data + shared types
│   ├── chrome.tsx         # TitleBar + Sidebar
│   ├── content.tsx        # TabsBar, ActionBar, Breadcrumb, BlobTable, Inspector
│   ├── panels.tsx         # AgentPanel, CommandPalette, ActivityBar, StatusBar, ConfirmModal
│   └── lib/ipc.ts         # invoke() wrapper with browser fallback
└── src-tauri/
    ├── Cargo.toml         # Tauri 2 backend (excluded from root workspace)
    ├── rust-toolchain.toml  # stable (Tauri 2 needs ≥1.77, workspace is 1.75)
    ├── tauri.conf.json
    ├── capabilities/default.json
    ├── icons/icon.png     # placeholder (replace before bundle)
    └── src/
        ├── main.rs        # Windows subsystem guard + lib entry
        ├── lib.rs         # Builder + invoke_handler
        └── commands.rs    # stub IPC — mock data mirroring the design
```
