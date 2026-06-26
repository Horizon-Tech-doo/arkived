// Arkived — Title bar (window chrome) + Sidebar (explorer tree)
import React, { CSSProperties, ReactNode } from "react";
import {
  IconLogo, IconAzure, IconCaretDown, IconSearch, IconCircleFilled, IconRefresh, IconSettings,
  IconDatabase, IconPlus, IconFilter, IconContainer, IconQueue, IconTable, IconShare, IconFolderOpen,
  IconKey, IconTerminal, IconLock, IconShieldCheck, IconChevronDown, IconChevronRight,
} from "./icons";

// ─────────────────────────────────────────────────────────────
// TITLE BAR
// ─────────────────────────────────────────────────────────────
interface TitleBarProps {
  onOpenPalette: () => void;
  agentOpen?: boolean;
  onToggleAgent?: () => void;
  activeConnection: string;
  connectionDetail?: string;
  connected?: boolean;
  statusText?: string;
  onRefresh?: () => void;
  onOpenSettings?: () => void;
}
export function TitleBar({
  onOpenPalette,
  agentOpen,
  onToggleAgent,
  activeConnection,
  connectionDetail = "No endpoint selected",
  connected = true,
  statusText = "connected",
  onRefresh,
  onOpenSettings,
}: TitleBarProps) {
  return (
    <div className="titlebar" style={titlebarStyles.root}>
      <div style={titlebarStyles.left}>
        <div style={titlebarStyles.brandBlock}>
          <IconLogo size={14} color="var(--accent)" />
          <span style={titlebarStyles.brandName}>arkived</span>
          <span style={titlebarStyles.version}>v0.3.1</span>
        </div>

        <div style={titlebarStyles.sep} />

        <button style={titlebarStyles.connPill}>
          <IconAzure size={11} />
          <span style={{ color: "var(--fg-1)", fontWeight: 500 }}>{activeConnection}</span>
          <span style={titlebarStyles.connSep}>/</span>
          <span style={{ color: "var(--fg-2)" }}>{connectionDetail}</span>
          <IconCaretDown size={10} style={{ color: "var(--fg-3)", marginLeft: 2 }} />
        </button>
      </div>

      <div style={titlebarStyles.center}>
        <button style={titlebarStyles.paletteBtn} onClick={onOpenPalette}>
          <IconSearch size={11} style={{ color: "var(--fg-2)" }} />
          <span style={{ color: "var(--fg-2)" }}>Search resources</span>
          <span style={{ flex: 1 }} />
          <span className="kbd">Ctrl K</span>
        </button>
      </div>

      <div style={titlebarStyles.right}>
        <div style={titlebarStyles.statusGroup}>
          <IconCircleFilled size={7} color={connected ? "var(--green)" : "var(--yellow)"} />
          <span style={{ color: "var(--fg-2)" }}>{statusText}</span>
        </div>
        <button style={titlebarStyles.iconBtn} title="Refresh" onClick={onRefresh}>
          <IconRefresh size={13} />
        </button>
        {onOpenSettings && (
          <button style={titlebarStyles.iconBtn} title="Settings" onClick={onOpenSettings}>
            <IconSettings size={13} />
          </button>
        )}
        {onToggleAgent && (
          <>
            <div style={titlebarStyles.sep} />
            <button
              style={{
                ...titlebarStyles.agentBtn,
                background: agentOpen ? "var(--bg-3)" : "transparent",
                color: agentOpen ? "var(--fg-0)" : "var(--fg-3)",
                borderColor: agentOpen ? "var(--border-2)" : "var(--border-1)",
              }}
              onClick={onToggleAgent}
            >
              <span>Agent</span>
              <span className="kbd" style={{ marginLeft: 2 }}>Ctrl J</span>
            </button>
          </>
        )}
      </div>
    </div>
  );
}

const titlebarStyles: Record<string, CSSProperties> = {
  root: {
    height: 36,
    background: "var(--bg-0)",
    borderBottom: "1px solid var(--border-0)",
    display: "flex",
    alignItems: "center",
    padding: "0 10px",
    gap: 10,
    flexShrink: 0,
    fontSize: 11,
    fontFamily: "var(--mono)",
  },
  left: { display: "flex", alignItems: "center", gap: 10, flexShrink: 0 },
  center: { flex: 1, display: "flex", justifyContent: "center", maxWidth: 480 },
  right: { display: "flex", alignItems: "center", gap: 6, flexShrink: 0, marginLeft: "auto" },
  brandBlock: { display: "flex", alignItems: "center", gap: 6 },
  brandName: { color: "var(--fg-0)", fontWeight: 600, letterSpacing: "0.02em" },
  version: { color: "var(--fg-3)", fontSize: 10 },
  sep: { width: 1, height: 18, background: "var(--border-0)" },
  connPill: {
    display: "flex", alignItems: "center", gap: 6,
    padding: "3px 8px", borderRadius: 3,
    background: "var(--bg-1)", border: "1px solid var(--border-1)",
    fontSize: 11, fontFamily: "var(--mono)",
  },
  connSep: { color: "var(--fg-3)" },
  paletteBtn: {
    display: "flex", alignItems: "center", gap: 8,
    width: "100%", maxWidth: 460, height: 24,
    padding: "0 8px 0 10px", borderRadius: 3,
    background: "transparent", border: "1px solid var(--border-0)",
    fontSize: 11, fontFamily: "var(--mono)",
  },
  statusGroup: {
    display: "flex", alignItems: "center", gap: 5,
    padding: "0 6px", fontSize: 10,
  },
  iconBtn: {
    width: 24, height: 24, display: "flex",
    alignItems: "center", justifyContent: "center",
    borderRadius: 4, color: "var(--fg-2)",
  },
  agentBtn: {
    display: "flex", alignItems: "center", gap: 6,
    padding: "3px 8px", borderRadius: 3,
    border: "1px solid", fontSize: 11, fontWeight: 500,
    fontFamily: "var(--mono)",
  },
};

// ─────────────────────────────────────────────────────────────
// SIDEBAR — storage tree
// ─────────────────────────────────────────────────────────────
interface TreeRowProps {
  depth: number;
  expanded?: boolean;
  onToggle?: () => void;
  icon: ReactNode;
  label: string;
  meta?: ReactNode;
  selected?: boolean;
  dim?: boolean;
  badge?: string | null;
  onClick?: () => void;
  action?: ReactNode;
  onAction?: () => void;
  onContextMenu?: (event: React.MouseEvent<HTMLDivElement>) => void;
}
export function TreeRow({
  depth,
  expanded,
  onToggle,
  icon,
  label,
  meta,
  selected,
  dim,
  badge,
  onClick,
  action,
  onAction,
  onContextMenu,
}: TreeRowProps) {
  return (
    <div
      onClick={onClick}
      onContextMenu={onContextMenu}
      style={{
        display: "flex", alignItems: "center", gap: 4,
        height: 22, padding: `0 8px 0 ${6 + depth * 12}px`,
        cursor: "pointer",
        background: selected ? "var(--accent-ghost)" : "transparent",
        borderLeft: selected ? "2px solid var(--accent)" : "2px solid transparent",
        fontSize: 11,
        fontFamily: "var(--mono)",
        color: dim ? "var(--fg-2)" : selected ? "var(--fg-0)" : "var(--fg-1)",
      }}
      onMouseEnter={(e) => { if (!selected) (e.currentTarget as HTMLDivElement).style.background = "var(--bg-2)"; }}
      onMouseLeave={(e) => { if (!selected) (e.currentTarget as HTMLDivElement).style.background = "transparent"; }}
    >
      {onToggle ? (
        <button
          onClick={(e) => { e.stopPropagation(); onToggle(); }}
          style={{ width: 12, height: 12, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg-3)" }}
        >
          {expanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        </button>
      ) : (
        <div style={{ width: 12 }} />
      )}
      <span style={{ color: selected ? "var(--accent)" : dim ? "var(--fg-3)" : "var(--fg-2)", display: "flex" }}>
        {icon}
      </span>
      <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
      {meta != null && <span style={{ color: "var(--fg-3)", fontSize: 10 }}>{meta}</span>}
      {badge && (
        <span style={{
          fontSize: 9, fontWeight: 600,
          padding: "1px 4px", borderRadius: 3,
          background: "var(--bg-3)", color: "var(--fg-2)",
          border: "1px solid var(--border-1)",
        }}>{badge}</span>
      )}
      {action && (
        <button
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            onAction?.();
          }}
          style={{
            width: 16,
            height: 16,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--fg-3)",
            borderRadius: 4,
          }}
        >
          {action}
        </button>
      )}
    </div>
  );
}

interface GroupHeaderProps {
  label: string;
  count?: string | number;
  action?: boolean;
}
export function GroupHeader({ label, count, action }: GroupHeaderProps) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 6,
      padding: "10px 10px 4px", fontSize: 9,
      fontFamily: "var(--mono)", fontWeight: 600,
      color: "var(--fg-3)", textTransform: "uppercase", letterSpacing: "0.08em",
    }}>
      <span>{label}</span>
      {count != null && <span style={{ color: "var(--fg-4)" }}>{count}</span>}
      <span style={{ flex: 1 }} />
      {action && (
        <button style={{ color: "var(--fg-3)", width: 14, height: 14, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <IconPlus size={10} />
        </button>
      )}
    </div>
  );
}
