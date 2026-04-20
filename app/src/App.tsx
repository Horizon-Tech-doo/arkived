// Arkived — App root (ported from design/app.jsx)
import React, { useEffect, useState } from "react";
import { TitleBar, Sidebar } from "./chrome";
import { TabsBar, ActionBar, Breadcrumb, BlobTable, Inspector } from "./content";
import { AgentPanel, CommandPalette, ActivityBar, StatusBar, ConfirmModal } from "./panels";
import { IconInfo, IconContainer, IconChevronLeft, IconChevronRight, IconChevronDown } from "./icons";
import { BLOB_ROWS, BREADCRUMB_BLOB, BlobRow } from "./data";
import { fetchBlobs } from "./lib/ipc";

const ACCENT_PRESETS: Record<string, { accent: string; dim: string; ghost: string; ghostStrong: string; label: string }> = {
  rust:    { accent: "#e06c3a", dim: "#a4502b", ghost: "rgba(224,108,58,0.12)",  ghostStrong: "rgba(224,108,58,0.22)",  label: "Rust" },
  amber:   { accent: "#e0a341", dim: "#a87b31", ghost: "rgba(224,163,65,0.12)",  ghostStrong: "rgba(224,163,65,0.22)",  label: "Amber" },
  iron:    { accent: "#8a8a95", dim: "#5a5a65", ghost: "rgba(138,138,149,0.15)", ghostStrong: "rgba(138,138,149,0.25)", label: "Iron" },
  azure:   { accent: "#5aa0e0", dim: "#3e7aad", ghost: "rgba(90,160,224,0.12)",  ghostStrong: "rgba(90,160,224,0.22)",  label: "Azure" },
  moss:    { accent: "#7ec687", dim: "#4f8a5a", ghost: "rgba(126,198,135,0.12)", ghostStrong: "rgba(126,198,135,0.22)", label: "Moss" },
  magenta: { accent: "#d46ba8", dim: "#9b4d7c", ghost: "rgba(212,107,168,0.12)", ghostStrong: "rgba(212,107,168,0.22)", label: "Magenta" },
};

function applyAccent(key: string) {
  const p = ACCENT_PRESETS[key] || ACCENT_PRESETS.rust;
  const root = document.documentElement;
  root.style.setProperty("--accent", p.accent);
  root.style.setProperty("--accent-dim", p.dim);
  root.style.setProperty("--accent-ghost", p.ghost);
  root.style.setProperty("--accent-ghost-strong", p.ghostStrong);
}

export default function App() {
  const [accent] = useState("rust");
  const [viewportW, setViewportW] = useState<number>(typeof window !== "undefined" ? window.innerWidth : 1440);
  useEffect(() => {
    const onResize = () => setViewportW(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  const narrow = viewportW < 1100;
  const [agentOpenRaw, setAgentOpen] = useState(true);
  const agentOpen = agentOpenRaw && !narrow;
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [activityExpanded, setActivityExpanded] = useState(true);
  const [selected, setSelected] = useState<Set<number>>(new Set([0]));
  const [tabs, setTabs] = useState([
    { id: "welcome", label: "Get Started", icon: <IconInfo size={11} /> },
    { id: "device-twins", label: "device-twins", icon: <IconContainer size={11} />, dirty: true },
  ]);
  const [activeTab, setActiveTab] = useState("device-twins");

  // Blob rows — default to mock, override with IPC when running under Tauri.
  const [rows, setRows] = useState<BlobRow[]>(BLOB_ROWS);
  useEffect(() => {
    let alive = true;
    fetchBlobs("stdlnphoenixproddlp", "device-twins", "device-twins-sync/")
      .then((r) => { if (alive && r && r.length) setRows(r); })
      .catch(() => { /* stay on mock */ });
    return () => { alive = false; };
  }, []);

  useEffect(() => { applyAccent(accent); }, [accent]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const isMod = e.metaKey || e.ctrlKey;
      if (isMod && e.key.toLowerCase() === "k") {
        e.preventDefault(); setPaletteOpen((o) => !o);
      } else if (isMod && e.key.toLowerCase() === "j") {
        e.preventDefault(); setAgentOpen((o) => !o);
      } else if (e.key === "Escape") {
        setPaletteOpen(false); setConfirmOpen(false);
      } else if (e.key === "Backspace" && selected.size > 0 && !paletteOpen) {
        const tag = (document.activeElement as HTMLElement | null)?.tagName;
        if (tag !== "INPUT" && tag !== "TEXTAREA") {
          e.preventDefault(); setConfirmOpen(true);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, paletteOpen]);

  const toggleSelect = (i: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  };
  const selectAll = () => {
    setSelected((prev) => (prev.size === rows.length ? new Set() : new Set(rows.map((_, i) => i))));
  };

  const firstSelected = selected.size > 0 ? rows[Math.min(...selected)] : null;

  return (
    <div style={{
      height: "100vh",
      minWidth: 960,
      display: "flex", flexDirection: "column",
      background: "var(--bg-0)",
      overflow: "hidden",
    }}>
      <TitleBar
        agentOpen={agentOpen}
        onToggleAgent={() => setAgentOpen((o) => !o)}
        onOpenPalette={() => setPaletteOpen(true)}
        activeConnection="stdlnphoenixproddlp"
      />

      <div style={{ flex: 1, display: "flex", minHeight: 0, overflow: "hidden" }}>
        <Sidebar />

        <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0, overflow: "hidden" }}>
          <TabsBar
            tabs={tabs}
            active={activeTab}
            onSelect={setActiveTab}
            onClose={(id) => {
              setTabs((ts) => ts.filter((t) => t.id !== id));
              if (activeTab === id && tabs.length > 1) {
                const next = tabs.find((t) => t.id !== id);
                if (next) setActiveTab(next.id);
              }
            }}
            onNew={() => {}}
          />
          <ActionBar
            selectedCount={selected.size}
            onDelete={() => setConfirmOpen(true)}
            onUpload={() => {}}
          />
          <Breadcrumb path={BREADCRUMB_BLOB} />

          <div style={{ flex: "1 1 0", display: "flex", flexDirection: "column", minHeight: 0, overflow: "hidden" }}>
            <div style={{ flex: "1 1 0", minHeight: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
              <BlobTable
                rows={rows}
                selected={selected}
                onToggleSelect={toggleSelect}
                onSelectAll={selectAll}
                onDelete={() => setConfirmOpen(true)}
              />
            </div>
            <div style={{
              display: "flex", alignItems: "center", gap: 12,
              padding: "4px 10px",
              background: "var(--bg-0)",
              borderTop: "1px solid var(--border-0)",
              fontFamily: "var(--mono)", fontSize: 10,
              color: "var(--fg-2)",
              flexShrink: 0,
            }}>
              <span>Showing {rows.length} of 16 cached items</span>
              <span style={{ color: "var(--fg-3)" }}>· continuation: more available</span>
              <span style={{ flex: 1 }} />
              <button style={{ color: "var(--fg-2)", padding: "2px 6px", display: "flex", alignItems: "center", gap: 4 }}>
                <IconChevronLeft size={9} />
              </button>
              <span>page 1</span>
              <button style={{ color: "var(--fg-2)", padding: "2px 6px", display: "flex", alignItems: "center", gap: 4 }}>
                <IconChevronRight size={9} />
              </button>
              <button style={{ color: "var(--accent)", padding: "2px 8px", display: "flex", alignItems: "center", gap: 4 }}>
                Load more <IconChevronDown size={9} />
              </button>
            </div>
            {firstSelected && (
              <div style={{
                background: "var(--bg-1)",
                borderTop: "1px solid var(--border-0)",
                flexShrink: 0,
                maxHeight: 140,
                overflow: "auto",
              }}>
                <Inspector row={firstSelected} />
              </div>
            )}
          </div>

          <ActivityBar expanded={activityExpanded} onToggle={() => setActivityExpanded((e) => !e)} />
        </div>

        {agentOpen && <AgentPanel onClose={() => setAgentOpen(false)} />}
      </div>

      <StatusBar
        selectedCount={selected.size}
        totalRows={rows.length}
        agentOpen={agentOpen}
        onToggleAgent={() => setAgentOpen((o) => !o)}
      />

      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      <ConfirmModal
        open={confirmOpen}
        onClose={() => setConfirmOpen(false)}
        onConfirm={() => { setConfirmOpen(false); setSelected(new Set()); }}
      />
    </div>
  );
}
