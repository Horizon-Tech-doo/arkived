// Arkived — Command palette, Activity bar, Confirmation modal, Status bar
import React, { CSSProperties, useEffect, useRef, useState } from "react";
import {
  IconZap, IconCircleFilled, IconArrowRight, IconShield, IconCheck, IconCopy,
  IconPlus, IconTerminal, IconX, IconShieldCheck,
  IconSearch, IconUpload, IconDownload, IconContainer, IconQueue, IconTable,
  IconKey, IconLock, IconRefresh, IconLoader, IconTrash, IconChevronDown, IconChevronUp,
  IconAlert,
} from "./icons";
import type { Activity } from "./data";
import { Checkbox } from "./content";

export interface CommandItem {
  id: string;
  label: string;
  section?: string;
  hint?: string;
  keywords?: string;
  icon?: React.ReactNode;
  run: () => void;
}
interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  commands: CommandItem[];
}
export function CommandPalette({ open, onClose, commands }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelected(0);
      const handle = setTimeout(() => inputRef.current?.focus(), 10);
      return () => clearTimeout(handle);
    }
  }, [open]);

  const needle = query.trim().toLowerCase();
  const filtered = needle
    ? commands.filter((c) => `${c.label} ${c.section ?? ""} ${c.keywords ?? ""}`.toLowerCase().includes(needle))
    : commands;

  // Keep the selection in range as the filtered list changes.
  useEffect(() => {
    setSelected((current) => (current >= filtered.length ? 0 : current));
  }, [filtered.length]);

  if (!open) return null;

  const runAt = (index: number) => {
    const item = filtered[index];
    if (item) {
      onClose();
      item.run();
    }
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelected((current) => (filtered.length ? (current + 1) % filtered.length : 0));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((current) => (filtered.length ? (current - 1 + filtered.length) % filtered.length : 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      runAt(selected);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  };

  // Render flat, inserting a section header whenever the section changes.
  let lastSection: string | undefined;

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed", inset: 0, zIndex: 100,
        background: "rgba(0,0,0,0.5)",
        backdropFilter: "none",
        display: "flex", alignItems: "flex-start", justifyContent: "center",
        paddingTop: 120,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 620,
          background: "var(--bg-1)",
          border: "1px solid var(--border-2)",
          borderRadius: 6,
          boxShadow: "0 12px 36px rgba(0,0,0,0.45)",
          overflow: "hidden",
        }}
      >
        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          padding: "10px 14px",
          borderBottom: "1px solid var(--border-1)",
        }}>
          <IconSearch size={14} style={{ color: "var(--fg-2)" }} />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Search commands or resources"
            style={{
              flex: 1,
              fontSize: 14,
              color: "var(--fg-0)",
              fontFamily: "var(--sans)",
            }}
          />
          <span className="kbd">esc</span>
        </div>

        <div style={{ maxHeight: 420, overflow: "auto", padding: "4px 0" }}>
          {filtered.map((item, index) => {
            const header = item.section && item.section !== lastSection ? item.section : null;
            lastSection = item.section;
            const isActive = index === selected;
            return (
              <React.Fragment key={item.id}>
                {header && (
                  <div style={{
                    padding: "8px 14px 4px",
                    fontSize: 9, fontWeight: 700, color: "var(--fg-3)",
                    fontFamily: "var(--mono)", textTransform: "uppercase", letterSpacing: "0.08em",
                  }}>{header}</div>
                )}
                <div
                  onClick={() => runAt(index)}
                  onMouseMove={() => setSelected(index)}
                  style={{
                    display: "flex", alignItems: "center", gap: 10,
                    padding: "6px 14px",
                    cursor: "pointer",
                    background: isActive ? "var(--accent-ghost)" : "transparent",
                    borderLeft: isActive ? "2px solid var(--accent)" : "2px solid transparent",
                  }}
                >
                  <span style={{ color: isActive ? "var(--accent)" : "var(--fg-2)", display: "flex" }}>
                    {item.icon ?? <IconTerminal size={12} />}
                  </span>
                  <span style={{ flex: 1, fontSize: 12, color: "var(--fg-0)" }}>{item.label}</span>
                  {item.hint && <span className="kbd">{item.hint}</span>}
                </div>
              </React.Fragment>
            );
          })}
          {filtered.length === 0 && (
            <div style={{ padding: "30px 14px", textAlign: "center", color: "var(--fg-3)", fontSize: 12 }}>
              No matches.
            </div>
          )}
        </div>

        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          padding: "6px 14px",
          borderTop: "1px solid var(--border-1)",
          background: "var(--bg-2)",
          fontSize: 10, fontFamily: "var(--mono)", color: "var(--fg-3)",
        }}>
          <span><span className="kbd">↑↓</span> navigate</span>
          <span><span className="kbd">⏎</span> run</span>
          <span><span className="kbd">esc</span> close</span>
          <span style={{ flex: 1 }} />
          <span>{filtered.length} command{filtered.length === 1 ? "" : "s"}</span>
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// ACTIVITY BAR
// ─────────────────────────────────────────────────────────────
interface ActivityBarProps {
  expanded: boolean;
  expandedHeight: number;
  onToggle: () => void;
  activities: Activity[];
  onResizeStart?: (event: React.MouseEvent<HTMLDivElement>) => void;
  onCancelActivity?: (activityId: string) => void;
  onClearCompleted?: () => void;
  onClearSuccessful?: () => void;
}

function activityHeaderButtonStyle(disabled: boolean): CSSProperties {
  return {
    height: 20,
    padding: "0 7px",
    border: "1px solid transparent",
    borderRadius: 3,
    background: disabled ? "transparent" : "var(--bg-2)",
    color: disabled ? "var(--fg-4)" : "var(--accent)",
    fontFamily: "var(--mono)",
    fontSize: 10,
    cursor: disabled ? "default" : "pointer",
  };
}

export function ActivityBar({
  expanded,
  expandedHeight,
  onToggle,
  activities,
  onResizeStart,
  onCancelActivity,
  onClearCompleted,
  onClearSuccessful,
}: ActivityBarProps) {
  const running = activities.filter((a) => a.status === "running");
  const done = activities.filter((a) => a.status !== "running");
  const successful = activities.filter((a) => a.status === "done");
  const activityIcon = (activity: Activity) => {
    if (activity.kind === "delete") {
      return <IconTrash size={11} />;
    }
    if (activity.kind === "download") {
      return <IconDownload size={11} />;
    }
    return <IconUpload size={11} />;
  };

  return (
    <div style={{
      background: "var(--bg-1)",
      borderTop: "1px solid var(--border-0)",
      flexShrink: 0,
      fontFamily: "var(--mono)",
      display: "flex", flexDirection: "column",
      height: expanded ? expandedHeight : 28,
      transition: "height 160ms ease-out",
      position: "relative",
      borderRadius: expanded ? 10 : 0,
      overflow: "hidden",
    }}>
      {expanded && onResizeStart && (
        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize activities pane"
          title="Drag to resize activities"
          onMouseDown={onResizeStart}
          style={{
            height: 6,
            flexShrink: 0,
            cursor: "row-resize",
            background: "var(--border-0)",
          }}
        />
      )}
      <div
        onClick={onToggle}
        style={{
          display: "flex", alignItems: "center", gap: 8,
          height: 28, padding: "0 10px",
          cursor: "pointer",
          borderBottom: expanded ? "1px solid var(--border-0)" : 0,
        }}
      >
        {expanded ? <IconChevronDown size={10} /> : <IconChevronUp size={10} />}
        <span style={{ fontSize: 10, fontWeight: 600, color: "var(--fg-2)", textTransform: "uppercase", letterSpacing: "0.08em" }}>
          Activities
        </span>
        {running.length > 0 && (
          <span style={{
            display: "inline-flex", alignItems: "center", gap: 5,
            padding: "1px 6px", borderRadius: 2,
            background: "var(--accent-ghost)", color: "var(--accent)",
            fontSize: 10, fontWeight: 500,
          }}>
            <IconLoader size={9} />
            {running.length} running
          </span>
        )}
        <span style={{ fontSize: 10, color: "var(--fg-3)" }}>{done.length} completed</span>
        <span style={{ flex: 1 }} />
        {expanded && (
          <>
            <button
              type="button"
              disabled={done.length === 0}
              onClick={(event) => {
                event.stopPropagation();
                onClearCompleted?.();
              }}
              style={activityHeaderButtonStyle(done.length === 0)}
            >
              Clear completed
            </button>
            <button
              type="button"
              disabled={successful.length === 0}
              onClick={(event) => {
                event.stopPropagation();
                onClearSuccessful?.();
              }}
              style={activityHeaderButtonStyle(successful.length === 0)}
            >
              Clear successful
            </button>
          </>
        )}
      </div>

      {expanded && (
        <div style={{ flex: 1, overflow: "auto" }}>
          {activities.length === 0 && (
            <div style={{
              padding: "18px 12px",
              color: "var(--fg-3)",
              fontSize: 11,
              borderBottom: "1px solid var(--border-0)",
            }}>
              Blob uploads, downloads, copies, renames, and deletes will appear here.
            </div>
          )}
          {activities.map((a) => (
            <div key={a.id} style={{
              display: "flex", alignItems: "flex-start", gap: 10,
              padding: "8px 12px",
              borderBottom: "1px solid var(--border-0)",
              fontSize: 11,
            }}>
              <div style={{
                width: 20, height: 20, borderRadius: 3,
                background: a.status === "running" ? "var(--accent-ghost)" : a.status === "cancelled" ? "rgba(255, 193, 7, 0.12)" : a.kind === "delete" ? "var(--red-dim)" : "var(--blue-dim)",
                color: a.status === "running" ? "var(--accent)" : a.status === "cancelled" ? "var(--yellow)" : a.kind === "delete" ? "var(--red)" : "var(--blue)",
                display: "flex", alignItems: "center", justifyContent: "center",
                flexShrink: 0,
              }}>
                {activityIcon(a)}
              </div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ display: "flex", gap: 6, alignItems: "baseline", marginBottom: 2 }}>
                  <span style={{ color: "var(--fg-0)", fontSize: 12, fontWeight: 500, fontFamily: "var(--sans)" }}>{a.title}</span>
                  <span style={{ color: "var(--fg-3)", fontSize: 10, fontFamily: "var(--sans)" }}>{a.detail}</span>
                </div>
                <div style={{ display: "flex", gap: 10, color: "var(--fg-3)", fontSize: 10 }}>
                  <span>started {a.started}</span>
                  {a.duration && <span>· {a.duration}</span>}
                  {a.result && (
                    <span style={{ color: a.status === "running" ? "var(--fg-2)" : a.status === "error" ? "var(--red)" : a.status === "cancelled" ? "var(--yellow)" : "var(--green)" }}>· {a.result}</span>
                  )}
                </div>
                {a.status === "running" && (
                  <div style={{
                    marginTop: 4,
                    height: 2, background: "var(--bg-3)", borderRadius: 2,
                    position: "relative", overflow: "hidden",
                  }}>
                    <div style={{
                      position: "absolute", top: 0, bottom: 0,
                      background: "var(--accent)",
                      width: `${(a.progress || 0) * 100}%`,
                      transition: "width 500ms ease-out",
                    }} />
                    <div style={{
                      position: "absolute", top: 0, bottom: 0, width: "30%",
                      background: "linear-gradient(90deg, transparent, var(--accent-ghost-strong), transparent)",
                      animation: "arkived-progress-indeterminate 1.6s ease-in-out infinite",
                    }} />
                  </div>
                )}
              </div>
              {a.status === "running" && onCancelActivity && (
                <button
                  type="button"
                  onClick={() => onCancelActivity(a.id)}
                  style={{
                    height: 22,
                    padding: "0 7px",
                    borderRadius: 3,
                    border: "1px solid var(--border-1)",
                    background: "var(--bg-2)",
                    color: "var(--fg-2)",
                    fontFamily: "var(--mono)",
                    fontSize: 10,
                    cursor: "pointer",
                  }}
                >
                  Cancel
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// STATUS BAR
// ─────────────────────────────────────────────────────────────
interface StatusBarProps {
  selectedCount: number;
  totalRows: number;
  onToggleAgent: () => void;
  agentOpen: boolean;
}
export function StatusBar({ selectedCount, totalRows, onToggleAgent }: StatusBarProps) {
  interface CellOpts { noBorder?: boolean; color?: string; bg?: string; onClick?: () => void; }
  const cell = (children: React.ReactNode, opts: CellOpts = {}) => (
    <div
      style={{
        display: "flex", alignItems: "center", gap: 5,
        padding: "0 10px", height: "100%",
        borderRight: opts.noBorder ? 0 : "1px solid var(--border-0)",
        color: opts.color || "var(--fg-2)",
        cursor: opts.onClick ? "pointer" : "default",
        background: opts.bg || "transparent",
      }}
      onClick={opts.onClick}
      onMouseEnter={(e) => { if (opts.onClick) (e.currentTarget as HTMLDivElement).style.background = "var(--bg-3)"; }}
      onMouseLeave={(e) => { if (opts.onClick) (e.currentTarget as HTMLDivElement).style.background = opts.bg || "transparent"; }}
    >
      {children}
    </div>
  );

  return (
    <div style={{
      height: 22,
      background: "var(--bg-0)",
      borderTop: "1px solid var(--border-0)",
      display: "flex", alignItems: "center",
      fontSize: 10, fontFamily: "var(--mono)",
      color: "var(--fg-2)",
      flexShrink: 0,
    }}>
      {cell(<><IconShieldCheck size={10} style={{ color: "var(--green)" }} /><span>policy: scoped-rw</span></>)}
      {cell(<><IconCircleFilled size={6} color="var(--green)" /><span>api 14ms</span></>)}
      {cell(<><span style={{ color: "var(--fg-3)" }}>rate</span><span>218/5000</span></>)}
      {cell(<><span style={{ color: "var(--fg-3)" }}>rows</span><span>{totalRows} shown / 16 cached</span></>)}
      {selectedCount > 0 && cell(
        <><span style={{ color: "var(--accent)" }}>●</span><span style={{ color: "var(--fg-0)" }}>{selectedCount} selected</span></>,
        { bg: "var(--accent-ghost)" }
      )}
      <span style={{ flex: 1 }} />
      {cell(<><IconTerminal size={10} /><span>arkivedd v0.3.1</span></>)}
      {cell(<><IconZap size={10} style={{ color: "var(--accent)" }} /><span>MCP · 18 tools</span></>, { onClick: onToggleAgent })}
      {cell(<><span>UTC+02</span></>, { noBorder: true })}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// CONFIRMATION MODAL
// ─────────────────────────────────────────────────────────────
interface ConfirmModalProps {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
}
export function ConfirmModal({ open, onClose, onConfirm }: ConfirmModalProps) {
  if (!open) return null;
  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed", inset: 0, zIndex: 90,
        background: "rgba(0,0,0,0.55)",
        backdropFilter: "none",
        display: "flex", alignItems: "center", justifyContent: "center",
        animation: "arkived-fade-in 120ms ease-out",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 560,
          background: "var(--bg-1)",
          border: "1px solid var(--red)",
          borderRadius: 6,
          boxShadow: "0 12px 36px rgba(0,0,0,0.45)",
          overflow: "hidden",
          fontFamily: "var(--mono)",
        }}
      >
        <div style={{
          padding: "10px 14px",
          background: "var(--red-dim)",
          borderBottom: "1px solid var(--red)",
          display: "flex", alignItems: "center", gap: 8,
        }}>
          <IconAlert size={12} style={{ color: "var(--red)" }} />
          <span style={{ fontSize: 10, fontWeight: 700, color: "var(--red)", textTransform: "uppercase", letterSpacing: "0.08em" }}>
            Destructive action
          </span>
        </div>
        <div style={{ padding: "16px 18px" }}>
          <div style={{ fontFamily: "var(--sans)", fontSize: 15, fontWeight: 600, color: "var(--fg-0)", marginBottom: 4 }}>
            Delete 1 blob?
          </div>
          <div style={{ fontFamily: "var(--sans)", fontSize: 13, color: "var(--fg-2)", marginBottom: 14, lineHeight: 1.5 }}>
            This permanently deletes <span style={{ color: "var(--fg-0)", fontFamily: "var(--mono)", fontSize: 12 }}>deviceSerialNumber_S=DA000405</span> and all nested objects. Soft-delete is enabled — you can undelete within 7 days.
          </div>
          <div style={{
            background: "var(--bg-0)",
            border: "1px solid var(--border-1)",
            borderRadius: 3,
            padding: "8px 10px",
            fontSize: 10,
            color: "var(--fg-1)",
            marginBottom: 14,
          }}>
            <span style={{ color: "var(--accent)" }}>$ </span>
            arkived blob rm <span style={{ color: "var(--blue)" }}>--recursive</span> <br />
            {"  "}<span style={{ color: "var(--blue)" }}>--account</span> stdlnphoenixproddlp <br />
            {"  "}<span style={{ color: "var(--blue)" }}>--container</span> device-twins <br />
            {"  "}<span style={{ color: "var(--fg-0)" }}>'device-twins-sync/.../deviceSerialNumber_S=DA000405'</span>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 14, fontSize: 11, color: "var(--fg-2)" }}>
            <Checkbox checked={false} onChange={() => {}} />
            <span>Also delete snapshots (if any)</span>
          </div>
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
            <button onClick={onClose} style={{
              padding: "6px 14px", borderRadius: 3,
              background: "var(--bg-3)", border: "1px solid var(--border-2)",
              color: "var(--fg-1)", fontSize: 12, fontFamily: "var(--mono)",
            }}>Cancel</button>
            <button onClick={onConfirm} style={{
              padding: "6px 14px", borderRadius: 3,
              background: "var(--red)",
              color: "#0a0a0c", fontSize: 12, fontWeight: 600, fontFamily: "var(--mono)",
              display: "flex", alignItems: "center", gap: 6,
            }}>
              <IconTrash size={11} style={{ strokeWidth: 2 }} />
              Delete
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
