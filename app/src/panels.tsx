// Arkived — Agent panel, Command palette, Activity bar, Confirmation modal, Status bar
import React, { CSSProperties, useEffect, useRef, useState } from "react";
import {
  IconZap, IconCircleFilled, IconArrowRight, IconShield, IconCheck, IconCopy,
  IconSparkle, IconPlus, IconTerminal, IconX, IconShieldCheck,
  IconSearch, IconUpload, IconDownload, IconContainer, IconQueue, IconTable,
  IconKey, IconLock, IconRefresh, IconLoader, IconTrash, IconChevronDown, IconChevronUp,
  IconAlert,
} from "./icons";
import { AGENT_TRANSCRIPT, ImpactRow, TranscriptMessage, AssistantMessage, ToolPart, ConfirmPart } from "./data";
import type { Activity } from "./data";
import { Checkbox } from "./content";

// ─────────────────────────────────────────────────────────────
// AGENT PANEL
// ─────────────────────────────────────────────────────────────
interface ToolCardProps {
  name: string;
  status: "ok" | "run" | "err";
  args: Record<string, unknown>;
  result?: string;
  duration: string;
}
export function ToolCard({ name, status, args, result, duration }: ToolCardProps) {
  const statusColor = status === "ok" ? "var(--green)" : status === "run" ? "var(--yellow)" : "var(--red)";
  return (
    <div style={{
      border: "1px solid var(--border-1)",
      borderRadius: 4,
      background: "var(--bg-2)",
      fontFamily: "var(--mono)", fontSize: 10,
      overflow: "hidden",
      margin: "6px 0",
    }}>
      <div style={{
        display: "flex", alignItems: "center", gap: 6,
        padding: "5px 8px",
        background: "var(--bg-3)",
        borderBottom: "1px solid var(--border-1)",
      }}>
        <IconZap size={10} style={{ color: "var(--accent)" }} />
        <span style={{ color: "var(--fg-0)", fontWeight: 600 }}>{name}</span>
        <span style={{ flex: 1 }} />
        <span style={{ color: statusColor, display: "inline-flex", alignItems: "center", gap: 3 }}>
          <IconCircleFilled size={6} color={statusColor} />
          {status === "ok" ? "completed" : status}
        </span>
        <span style={{ color: "var(--fg-3)" }}>· {duration}</span>
      </div>
      <div style={{ padding: "6px 8px", color: "var(--fg-2)" }}>
        {Object.entries(args).map(([k, v]) => (
          <div key={k} style={{ display: "flex", gap: 6, lineHeight: "16px" }}>
            <span style={{ color: "var(--purple)" }}>{k}:</span>
            <span style={{ color: "var(--fg-1)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {typeof v === "string" ? `"${v}"` : String(v)}
            </span>
          </div>
        ))}
      </div>
      {result && (
        <div style={{
          padding: "6px 8px",
          background: "var(--bg-1)",
          borderTop: "1px solid var(--border-1)",
          color: "var(--green)",
          display: "flex", gap: 6, alignItems: "flex-start",
        }}>
          <IconArrowRight size={10} style={{ color: "var(--fg-3)", marginTop: 2 }} />
          <span style={{ color: "var(--fg-0)" }}>{result}</span>
        </div>
      )}
    </div>
  );
}

interface ConfirmCardProps {
  confirm: ConfirmPart;
  onApprove: () => void;
  onReject: () => void;
}
export function ConfirmCard({ confirm, onApprove, onReject }: ConfirmCardProps) {
  const impactColor: Record<ImpactRow["kind"], string> = {
    info: "var(--fg-2)", neutral: "var(--fg-2)", warn: "var(--yellow)", danger: "var(--red)",
  };
  return (
    <div style={{
      border: "1px solid var(--accent-dim)",
      borderRadius: 4,
      background: "var(--bg-2)",
      fontFamily: "var(--mono)", fontSize: 10,
      margin: "6px 0",
      overflow: "hidden",
    }}>
      <div style={{
        display: "flex", alignItems: "center", gap: 6,
        padding: "6px 10px",
        background: "var(--accent-ghost)",
        borderBottom: "1px solid var(--accent-dim)",
        color: "var(--accent)",
        fontWeight: 600, fontSize: 10,
        textTransform: "uppercase", letterSpacing: "0.06em",
      }}>
        <IconShield size={11} />
        <span>Confirmation required</span>
        <span style={{ flex: 1 }} />
        <span style={{ color: "var(--fg-3)", fontWeight: 400, textTransform: "none", letterSpacing: 0 }}>policy: scoped-rw</span>
      </div>

      <div style={{ padding: "10px" }}>
        <div style={{ fontFamily: "var(--sans)", fontSize: 13, fontWeight: 600, color: "var(--fg-0)", marginBottom: 2 }}>
          {confirm.title}
        </div>
        <div style={{ fontFamily: "var(--sans)", fontSize: 12, color: "var(--fg-2)", marginBottom: 10 }}>
          {confirm.summary}
        </div>

        <div style={{
          background: "var(--bg-0)",
          border: "1px solid var(--border-1)",
          borderRadius: 3,
          padding: "8px 10px",
          fontFamily: "var(--mono)", fontSize: 10,
          color: "var(--fg-1)",
          marginBottom: 10,
          whiteSpace: "pre-wrap",
          wordBreak: "break-all",
          position: "relative",
        }}>
          <div style={{
            position: "absolute", top: 6, right: 6,
            display: "flex", gap: 4,
          }}>
            <button style={{
              padding: "2px 5px", fontSize: 9, color: "var(--fg-3)",
              border: "1px solid var(--border-1)", borderRadius: 2,
              background: "var(--bg-2)", display: "flex", alignItems: "center", gap: 3,
            }}>
              <IconCopy size={9} /> copy
            </button>
          </div>
          <span style={{ color: "var(--accent)" }}>$ </span>
          {confirm.cmd.split("\n").map((line, i) => (
            <div key={i} style={{ paddingLeft: i > 0 ? 12 : 0 }}>
              {line.split(/(--[a-z-]+)/).map((tok, j) =>
                tok.startsWith("--")
                  ? <span key={j} style={{ color: "var(--blue)" }}>{tok}</span>
                  : <span key={j}>{tok}</span>
              )}
            </div>
          ))}
        </div>

        <div style={{
          border: "1px solid var(--border-1)", borderRadius: 3,
          overflow: "hidden",
          marginBottom: 10,
        }}>
          {confirm.impact.map((row, i) => (
            <div key={i} style={{
              display: "flex", fontSize: 10,
              borderTop: i === 0 ? 0 : "1px solid var(--border-0)",
            }}>
              <div style={{
                width: 120, padding: "4px 8px",
                color: "var(--fg-3)",
                background: "var(--bg-1)",
                textTransform: "uppercase", letterSpacing: "0.04em", fontWeight: 600, fontSize: 9,
                borderRight: "1px solid var(--border-0)",
              }}>
                {row.label}
              </div>
              <div style={{
                padding: "4px 8px",
                color: impactColor[row.kind] || "var(--fg-1)",
                flex: 1,
              }}>
                {row.value}
              </div>
            </div>
          ))}
        </div>

        <div style={{ display: "flex", gap: 6 }}>
          <button onClick={onApprove} style={{
            flex: 1, height: 28, borderRadius: 3,
            background: "var(--accent)",
            color: "#0a0a0c", fontWeight: 600, fontFamily: "var(--mono)", fontSize: 11,
            display: "flex", alignItems: "center", justifyContent: "center", gap: 6,
          }}>
            <IconCheck size={12} style={{ strokeWidth: 2.5 }} /> Approve & run
            <span className="kbd" style={{ marginLeft: 6, background: "rgba(0,0,0,0.15)", color: "#0a0a0c", borderColor: "rgba(0,0,0,0.2)" }}>⏎</span>
          </button>
          <button onClick={onReject} style={{
            height: 28, padding: "0 14px", borderRadius: 3,
            background: "var(--bg-3)", border: "1px solid var(--border-2)",
            color: "var(--fg-1)", fontFamily: "var(--mono)", fontSize: 11,
          }}>Cancel</button>
          <button style={{
            height: 28, padding: "0 10px", borderRadius: 3,
            background: "transparent", border: "1px solid var(--border-1)",
            color: "var(--fg-2)", fontFamily: "var(--mono)", fontSize: 10,
          }}>Edit plan</button>
        </div>
      </div>
    </div>
  );
}

interface AgentMessageProps {
  msg: TranscriptMessage;
}
function AgentMessage({ msg }: AgentMessageProps) {
  if (msg.role === "user") {
    return (
      <div style={{ display: "flex", gap: 8, padding: "10px 12px", borderBottom: "1px solid var(--border-0)" }}>
        <div style={{
          width: 20, height: 20, borderRadius: 3,
          background: "var(--bg-3)",
          display: "flex", alignItems: "center", justifyContent: "center",
          color: "var(--fg-1)", fontSize: 10, fontWeight: 600, fontFamily: "var(--mono)",
          flexShrink: 0,
        }}>H</div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 2 }}>
            <span style={{ fontSize: 11, fontWeight: 600, color: "var(--fg-0)" }}>hamza</span>
            <span style={{ fontSize: 10, color: "var(--fg-3)", fontFamily: "var(--mono)" }}>{msg.at}</span>
          </div>
          <div style={{ fontSize: 12, color: "var(--fg-1)", lineHeight: 1.5 }}>{msg.text}</div>
        </div>
      </div>
    );
  }

  const a = msg as AssistantMessage;
  return (
    <div style={{ display: "flex", gap: 8, padding: "10px 12px", borderBottom: "1px solid var(--border-0)" }}>
      <div style={{
        width: 20, height: 20, borderRadius: 3,
        background: "var(--accent-ghost)",
        border: "1px solid var(--accent-dim)",
        display: "flex", alignItems: "center", justifyContent: "center",
        color: "var(--accent)",
        flexShrink: 0,
      }}>
        <IconSparkle size={11} />
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 4 }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: "var(--fg-0)" }}>arkived</span>
          <span style={{
            fontSize: 9, fontFamily: "var(--mono)",
            padding: "1px 5px", borderRadius: 2,
            background: "var(--bg-3)", color: "var(--fg-2)",
            border: "1px solid var(--border-1)",
          }}>mcp · claude-sonnet</span>
          <span style={{ fontSize: 10, color: "var(--fg-3)", fontFamily: "var(--mono)" }}>{a.at}</span>
        </div>
        {a.parts.map((part, i) => {
          if (part.kind === "text") {
            return (
              <div
                key={i}
                style={{ fontSize: 12, color: "var(--fg-1)", lineHeight: 1.5, marginBottom: 6 }}
                dangerouslySetInnerHTML={{
                  __html: part.text
                    .replace(/\*\*(.*?)\*\*/g, '<strong style="color:var(--fg-0);font-weight:600">$1</strong>')
                    .replace(/`(.*?)`/g, '<code style="font-family:var(--mono);font-size:11px;background:var(--bg-3);padding:1px 4px;border-radius:2px;color:var(--fg-0)">$1</code>'),
                }}
              />
            );
          }
          if (part.kind === "tool") {
            const t = part as ToolPart;
            return <ToolCard key={i} name={t.name} status={t.status} args={t.args} result={t.result} duration={t.duration} />;
          }
          if (part.kind === "confirm") {
            return <ConfirmCard key={i} confirm={part as ConfirmPart} onApprove={() => {}} onReject={() => {}} />;
          }
          return null;
        })}
      </div>
    </div>
  );
}

interface AgentPanelProps {
  width?: number;
  onClose: () => void;
}
export function AgentPanel({ width = 420, onClose }: AgentPanelProps) {
  const [input, setInput] = useState("");

  return (
    <div style={{
      width,
      flexShrink: 0,
      background: "var(--bg-1)",
      borderLeft: "1px solid var(--border-0)",
      display: "flex", flexDirection: "column",
      overflow: "hidden",
      animation: "arkived-slide-in-right 180ms ease-out",
    }}>
      <div style={{
        display: "flex", alignItems: "center", gap: 8,
        height: 32, padding: "0 10px",
        borderBottom: "1px solid var(--border-0)",
        background: "var(--bg-0)",
      }}>
        <IconSparkle size={12} style={{ color: "var(--accent)" }} />
        <span style={{ fontFamily: "var(--mono)", fontSize: 11, fontWeight: 600, color: "var(--fg-0)", letterSpacing: "0.02em" }}>AGENT</span>
        <span style={{ fontFamily: "var(--mono)", fontSize: 10, color: "var(--fg-3)" }}>· session mc4f2b</span>
        <span style={{ flex: 1 }} />
        <button style={{ color: "var(--fg-2)", width: 22, height: 22, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 3 }} title="New session">
          <IconPlus size={12} />
        </button>
        <button style={{ color: "var(--fg-2)", width: 22, height: 22, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 3 }} title="History">
          <IconTerminal size={12} />
        </button>
        <button onClick={onClose} style={{ color: "var(--fg-2)", width: 22, height: 22, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 3 }} title="Close">
          <IconX size={12} />
        </button>
      </div>

      <div style={{
        display: "flex", alignItems: "center", gap: 8,
        padding: "6px 10px",
        background: "var(--bg-2)",
        borderBottom: "1px solid var(--border-0)",
        fontSize: 10, fontFamily: "var(--mono)",
      }}>
        <IconShieldCheck size={11} style={{ color: "var(--green)" }} />
        <span style={{ color: "var(--fg-2)" }}>scope:</span>
        <span style={{ color: "var(--fg-1)" }}>stdlnphoenixproddlp</span>
        <span style={{ color: "var(--fg-4)" }}>·</span>
        <span style={{ color: "var(--fg-2)" }}>writes require confirm</span>
        <span style={{ flex: 1 }} />
        <button style={{ color: "var(--accent)", fontWeight: 500 }}>edit</button>
      </div>

      <div style={{ flex: 1, overflow: "auto" }}>
        {AGENT_TRANSCRIPT.map((msg, i) => <AgentMessage key={i} msg={msg} />)}

        <div style={{ display: "flex", gap: 8, padding: "10px 12px", opacity: 0.7 }}>
          <div style={{
            width: 20, height: 20, borderRadius: 3,
            background: "var(--accent-ghost)",
            border: "1px solid var(--accent-dim)",
            display: "flex", alignItems: "center", justifyContent: "center",
            color: "var(--accent)",
          }}>
            <IconSparkle size={11} style={{ animation: "arkived-pulse 1.4s ease-in-out infinite" }} />
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--fg-3)", fontSize: 11, fontFamily: "var(--mono)" }}>
            <span>waiting for approval</span>
            <span style={{ display: "inline-flex", gap: 2 }}>
              <span style={{ animation: "arkived-pulse 1.4s infinite", animationDelay: "0s" }}>·</span>
              <span style={{ animation: "arkived-pulse 1.4s infinite", animationDelay: "0.2s" }}>·</span>
              <span style={{ animation: "arkived-pulse 1.4s infinite", animationDelay: "0.4s" }}>·</span>
            </span>
          </div>
        </div>
      </div>

      <div style={{
        borderTop: "1px solid var(--border-0)",
        background: "var(--bg-0)",
        padding: 8,
      }}>
        <div style={{
          background: "var(--bg-2)",
          border: "1px solid var(--border-1)",
          borderRadius: 4,
          padding: 8,
        }}>
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Ask the agent to explore, move, or audit storage…"
            rows={2}
            style={{
              width: "100%", resize: "none",
              fontSize: 12, color: "var(--fg-0)",
              fontFamily: "var(--sans)",
              lineHeight: 1.4,
            }}
          />
          <div style={{
            display: "flex", alignItems: "center", gap: 4,
            marginTop: 6, paddingTop: 6,
            borderTop: "1px solid var(--border-0)",
          }}>
            <button title="Attach selection" style={pillBtn()}>
              <IconPlus size={11} /> selection
            </button>
            <button title="Tool scope" style={pillBtn()}>
              <IconZap size={11} /> 18 tools
            </button>
            <span style={{ flex: 1 }} />
            <span style={{ fontSize: 10, color: "var(--fg-3)", fontFamily: "var(--mono)" }}>Ctrl Enter to send</span>
            <button style={{
              padding: "3px 10px", borderRadius: 3,
              background: input ? "var(--accent)" : "var(--bg-3)",
              color: input ? "#0a0a0c" : "var(--fg-3)",
              fontFamily: "var(--mono)", fontSize: 11, fontWeight: 600,
              border: "1px solid " + (input ? "var(--accent)" : "var(--border-1)"),
            }}>send</button>
          </div>
        </div>
      </div>
    </div>
  );
}

function pillBtn(): CSSProperties {
  return {
    display: "flex", alignItems: "center", gap: 4,
    padding: "2px 6px", borderRadius: 3,
    background: "var(--bg-3)", color: "var(--fg-2)",
    border: "1px solid var(--border-1)",
    fontSize: 10, fontFamily: "var(--mono)",
  };
}

// ─────────────────────────────────────────────────────────────
// COMMAND PALETTE
// ─────────────────────────────────────────────────────────────
interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}
export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open && inputRef.current) {
      setTimeout(() => inputRef.current?.focus(), 10);
      setQuery("");
    }
  }, [open]);

  interface PItem { icon: React.ReactNode; label: string; kbd?: string; }
  interface PSection { section: string; items: PItem[]; }

  const all: PSection[] = [
    { section: "Suggested", items: [
      { icon: <IconSparkle size={12} style={{ color: "var(--accent)" }} />, label: 'Ask agent: "summarize activity in the last hour"', kbd: "agent" },
      { icon: <IconUpload size={12} />, label: "Upload files to device-twins-sync/…", kbd: "Ctrl U" },
      { icon: <IconDownload size={12} />, label: "Download selection", kbd: "Ctrl Shift D" },
    ]},
    { section: "Navigate", items: [
      { icon: <IconContainer size={12} />, label: "Go to container › raw-device-telemetry" },
      { icon: <IconContainer size={12} />, label: "Go to container › pipeline-output" },
      { icon: <IconQueue size={12} />, label: "Go to queue › ingress-q" },
      { icon: <IconTable size={12} />, label: "Go to table › DeviceRegistry" },
    ]},
    { section: "Storage Actions", items: [
      { icon: <IconKey size={12} />, label: "Generate SAS token…", kbd: "sas" },
      { icon: <IconShield size={12} />, label: "Manage ACLs (ADLS Gen2)…", kbd: "acl" },
      { icon: <IconTerminal size={12} />, label: "Copy AzCopy command for selection" },
      { icon: <IconRefresh size={12} />, label: "Sync metadata cache" },
    ]},
    { section: "Sessions", items: [
      { icon: <IconKey size={12} />, label: "Switch subscription › Horizon Tech — Prod" },
      { icon: <IconLock size={12} />, label: "Re-authenticate with Azure AD" },
    ]},
  ];

  const filtered = query
    ? all.map((s) => ({ ...s, items: s.items.filter((i) => i.label.toLowerCase().includes(query.toLowerCase())) })).filter((s) => s.items.length)
    : all;

  if (!open) return null;

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed", inset: 0, zIndex: 100,
        background: "rgba(0,0,0,0.5)",
        backdropFilter: "blur(3px)",
        display: "flex", alignItems: "flex-start", justifyContent: "center",
        paddingTop: 120,
        animation: "arkived-fade-in 120ms ease-out",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 620,
          background: "var(--bg-1)",
          border: "1px solid var(--border-2)",
          borderRadius: 6,
          boxShadow: "0 24px 60px rgba(0,0,0,0.6), 0 0 0 1px var(--border-2)",
          overflow: "hidden",
          animation: "arkived-scale-in 140ms ease-out",
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
            placeholder="Search commands, resources, or ask the agent…"
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
          {filtered.map((s, si) => (
            <div key={si}>
              <div style={{
                padding: "8px 14px 4px",
                fontSize: 9, fontWeight: 700, color: "var(--fg-3)",
                fontFamily: "var(--mono)", textTransform: "uppercase", letterSpacing: "0.08em",
              }}>{s.section}</div>
              {s.items.map((it, ii) => {
                const isFirst = si === 0 && ii === 0;
                return (
                  <div
                    key={ii}
                    style={{
                      display: "flex", alignItems: "center", gap: 10,
                      padding: "6px 14px",
                      cursor: "pointer",
                      background: isFirst ? "var(--accent-ghost)" : "transparent",
                      borderLeft: isFirst ? "2px solid var(--accent)" : "2px solid transparent",
                    }}
                    onMouseEnter={(e) => { if (!isFirst) (e.currentTarget as HTMLDivElement).style.background = "var(--bg-2)"; }}
                    onMouseLeave={(e) => { if (!isFirst) (e.currentTarget as HTMLDivElement).style.background = "transparent"; }}
                  >
                    <span style={{ color: isFirst ? "var(--accent)" : "var(--fg-2)", display: "flex" }}>
                      {it.icon}
                    </span>
                    <span style={{ flex: 1, fontSize: 12, color: "var(--fg-0)" }}>{it.label}</span>
                    {it.kbd && <span className="kbd">{it.kbd}</span>}
                  </div>
                );
              })}
            </div>
          ))}
          {filtered.length === 0 && (
            <div style={{ padding: "30px 14px", textAlign: "center", color: "var(--fg-3)", fontSize: 12 }}>
              No matches. Press <span className="kbd">⏎</span> to ask the agent.
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
          <span><span className="kbd">⏎</span> open</span>
          <span><span className="kbd">Ctrl Enter</span> ask agent</span>
          <span style={{ flex: 1 }} />
          <span>18 MCP tools loaded</span>
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
            background: "linear-gradient(180deg, transparent, rgba(63, 157, 246, 0.14), transparent)",
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
        backdropFilter: "blur(3px)",
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
          boxShadow: "0 24px 60px rgba(0,0,0,0.6)",
          overflow: "hidden",
          animation: "arkived-scale-in 140ms ease-out",
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
