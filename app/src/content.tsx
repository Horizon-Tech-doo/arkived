// Arkived — Tabs bar, Action bar, Breadcrumb, Blob table, Inspector
// (ported from design/main.jsx — renamed to avoid clashing with main.tsx entry)
import React, { CSSProperties, ReactNode, useEffect, useRef, useState } from "react";
import {
  IconX, IconPlus, IconUpload, IconDownload, IconEye, IconInfo, IconCopy,
  IconTrash, IconRefresh,
  IconArrowLeft, IconArrowUp, IconChevronRight, IconChevronLeft, IconChevronDown, IconChevronUp,
  IconCircleFilled, IconCaretDown, IconFilter, IconFolder, IconFileCode, IconFileArchive, IconFileImage, IconFile,
  IconUnlock, IconLock, IconCheck,
} from "./icons";
import { BlobRow, BreadcrumbEntry } from "./data";

// ─────────────────────────────────────────────────────────────
// Tabs bar
// ─────────────────────────────────────────────────────────────
export interface TabDef {
  id: string;
  label: string;
  icon: ReactNode;
  dirty?: boolean;
}
interface TabsBarProps {
  tabs: TabDef[];
  active: string;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onNew: () => void;
}
export function TabsBar({ tabs, active, onSelect, onClose, onNew }: TabsBarProps) {
  return (
    <div style={tabsStyles.root}>
      <div style={tabsStyles.scroll}>
        {tabs.map((t) => {
          const isActive = t.id === active;
          return (
            <div
              key={t.id}
              onClick={() => onSelect(t.id)}
              style={{
                ...tabsStyles.tab,
                background: isActive ? "var(--bg-1)" : "transparent",
                color: isActive ? "var(--fg-0)" : "var(--fg-2)",
                borderTop: isActive ? "1px solid var(--accent)" : "1px solid transparent",
              }}
            >
              <span style={{ color: isActive ? "var(--accent)" : "var(--fg-3)", display: "flex" }}>
                {t.icon}
              </span>
              <span style={{ fontSize: 11, fontFamily: "var(--mono)" }}>{t.label}</span>
              {t.dirty && <span style={{ width: 5, height: 5, borderRadius: "50%", background: "var(--yellow)" }} />}
              <button
                onClick={(e) => { e.stopPropagation(); onClose(t.id); }}
                style={{
                  width: 14, height: 14, display: "flex", alignItems: "center",
                  justifyContent: "center", color: "var(--fg-3)", borderRadius: 3,
                  marginLeft: 2,
                }}
                onMouseEnter={(e) => ((e.currentTarget as HTMLButtonElement).style.background = "var(--bg-3)")}
                onMouseLeave={(e) => ((e.currentTarget as HTMLButtonElement).style.background = "transparent")}
              >
                <IconX size={9} />
              </button>
            </div>
          );
        })}
        <button onClick={onNew} style={tabsStyles.newTab}>
          <IconPlus size={11} />
        </button>
      </div>
      <div style={{ flex: 1 }} />
    </div>
  );
}

const tabsStyles: Record<string, CSSProperties> = {
  root: {
    display: "flex", alignItems: "stretch",
    height: 30,
    background: "var(--bg-0)",
    borderBottom: "1px solid var(--border-0)",
    flexShrink: 0,
  },
  scroll: { display: "flex", alignItems: "stretch", overflow: "auto" },
  tab: {
    display: "flex", alignItems: "center", gap: 6,
    padding: "0 10px 0 12px", cursor: "pointer",
    borderRight: "1px solid var(--border-0)",
    minWidth: 120,
  },
  newTab: {
    width: 30, display: "flex", alignItems: "center", justifyContent: "center",
    color: "var(--fg-3)",
  },
};

// ─────────────────────────────────────────────────────────────
// Action bar
// ─────────────────────────────────────────────────────────────
interface ActionBarProps {
  selectedCount: number;
  canPreview?: boolean;
  onDelete: () => void;
  onUpload: () => void;
  onDownload: () => void;
  onPreview: () => void;
  onCreateFolder: () => void;
  onPaste: () => void;
  canPaste: boolean;
  onRefresh: () => void;
}
export function ActionBar({
  selectedCount,
  canPreview = selectedCount === 1,
  onDelete,
  onUpload,
  onDownload,
  onPreview,
  onCreateFolder,
  onPaste,
  canPaste,
  onRefresh,
}: ActionBarProps) {
  interface BtnOpts {
    disabled?: boolean;
    onClick?: () => void;
    danger?: boolean;
    kbd?: string;
    title?: string;
  }
  const btn = (icon: ReactNode, label: string, opts: BtnOpts = {}) => (
    <button
      disabled={opts.disabled}
      onClick={opts.onClick}
      style={{
        display: "flex", alignItems: "center", gap: 5,
        padding: "0 8px", height: 24, borderRadius: 3,
        color: opts.disabled ? "var(--fg-3)" : opts.danger ? "var(--red)" : "var(--fg-1)",
        fontSize: 11, fontFamily: "var(--mono)",
        border: "1px solid transparent",
        cursor: opts.disabled ? "not-allowed" : "pointer",
      }}
      onMouseEnter={(e) => { if (!opts.disabled) (e.currentTarget as HTMLButtonElement).style.background = "var(--bg-2)"; }}
      onMouseLeave={(e) => ((e.currentTarget as HTMLButtonElement).style.background = "transparent")}
      title={opts.title}
    >
      {icon}
      <span>{label}</span>
      {opts.kbd && <span className="kbd" style={{ marginLeft: 4 }}>{opts.kbd}</span>}
    </button>
  );

  const sep = <div style={{ width: 1, height: 16, background: "var(--border-0)", margin: "0 4px" }} />;

  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 2,
      padding: "4px 8px",
      background: "var(--bg-0)",
      borderBottom: "1px solid var(--border-0)",
      flexShrink: 0,
    }}>
      {btn(<IconUpload size={12} />, "Upload", { onClick: onUpload, kbd: "Ctrl U" })}
      {btn(<IconDownload size={12} />, "Download", {
        disabled: selectedCount === 0,
        onClick: onDownload,
      })}
      {btn(<IconEye size={12} />, "Preview", {
        disabled: !canPreview,
        onClick: onPreview,
      })}
      {sep}
      {btn(<IconPlus size={12} />, "New folder", { onClick: onCreateFolder })}
      {btn(<IconCopy size={12} />, "Paste", { disabled: !canPaste, onClick: onPaste })}
      {btn(<IconTrash size={12} />, selectedCount > 0 ? `Delete (${selectedCount})` : "Delete", {
        disabled: selectedCount === 0,
        danger: true,
        onClick: onDelete,
        kbd: "Del",
      })}
      <span style={{ flex: 1 }} />
      {btn(<IconRefresh size={12} />, "", { title: "Refresh", kbd: "Ctrl R", onClick: onRefresh })}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Breadcrumb
// ─────────────────────────────────────────────────────────────
interface BreadcrumbProps {
  path: BreadcrumbEntry[];
}
export function Breadcrumb({ path }: BreadcrumbProps) {
  return (
    <div style={{
      display: "flex", alignItems: "stretch", gap: 0,
      height: 28, padding: "0 8px",
      background: "var(--bg-1)",
      borderBottom: "1px solid var(--border-0)",
      fontFamily: "var(--mono)", fontSize: 11,
      flexShrink: 0,
    }}>
      <button style={crumbStyles.navBtn} title="Back"><IconArrowLeft size={12} /></button>
      <button style={crumbStyles.navBtn} title="Forward"><IconChevronRight size={12} /></button>
      <button style={crumbStyles.navBtn} title="Up"><IconArrowUp size={12} /></button>
      <div style={{ width: 1, alignSelf: "center", height: 16, background: "var(--border-0)", margin: "0 6px" }} />

      <button style={{
        display: "flex", alignItems: "center", gap: 5, padding: "0 8px",
        fontSize: 11, fontFamily: "var(--mono)", color: "var(--fg-1)",
        borderRight: "1px solid var(--border-0)",
      }}>
        <IconCircleFilled size={6} color="var(--green)" />
        <span>Active blobs</span>
        <span style={{ color: "var(--fg-3)" }}>(default)</span>
        <IconCaretDown size={10} style={{ color: "var(--fg-3)" }} />
      </button>

      <div style={{ display: "flex", alignItems: "center", padding: "0 4px", overflow: "auto", flex: 1 }}>
        {path.map((chunk, i) => {
          const isLast = i === path.length - 1;
          return (
            <React.Fragment key={i}>
              <button style={{
                padding: "2px 6px",
                color: isLast ? "var(--fg-0)" : "var(--fg-2)",
                fontWeight: isLast ? 600 : 400,
                fontSize: 11, fontFamily: "var(--mono)",
                borderRadius: 3,
              }}>
                {chunk.label}
              </button>
              {!isLast && <span style={{ color: "var(--fg-4)", padding: "0 2px" }}>/</span>}
            </React.Fragment>
          );
        })}
      </div>

      <div style={{
        display: "flex", alignItems: "center", gap: 5,
        padding: "0 8px", marginLeft: 6, alignSelf: "center",
        height: 20, borderRadius: 3,
        background: "var(--bg-2)", border: "1px solid var(--border-1)",
        width: 200,
      }}>
        <IconFilter size={10} style={{ color: "var(--fg-3)" }} />
        <input placeholder="Filter by prefix (case-sensitive)" style={{ flex: 1, fontSize: 11, color: "var(--fg-1)", fontFamily: "var(--mono)" }} />
      </div>
    </div>
  );
}

const crumbStyles: Record<string, CSSProperties> = {
  navBtn: {
    width: 24, display: "flex", alignItems: "center", justifyContent: "center",
    color: "var(--fg-2)",
  },
};

// ─────────────────────────────────────────────────────────────
// BLOB TABLE
// ─────────────────────────────────────────────────────────────
interface TierPillProps {
  tier: string | null;
}
export function TierPill({ tier }: TierPillProps) {
  if (!tier) return null;
  const map: Record<string, { bg: string; fg: string; label: string }> = {
    Hot: { bg: "var(--red-dim)", fg: "var(--red)", label: "Hot" },
    Cool: { bg: "var(--blue-dim)", fg: "var(--blue)", label: "Cool" },
    Cold: { bg: "var(--cyan-dim)", fg: "var(--cyan)", label: "Cold" },
    Archive: { bg: "var(--purple-dim)", fg: "var(--purple)", label: "Archive" },
  };
  const c = map[tier] || { bg: "var(--bg-3)", fg: "var(--fg-2)", label: tier };
  return (
    <span style={{
      display: "inline-flex", alignItems: "center", gap: 4,
      padding: "1px 6px", borderRadius: 2,
      fontSize: 10, fontWeight: 500,
      fontFamily: "var(--mono)",
      background: c.bg, color: c.fg,
    }}>
      <span style={{ width: 5, height: 5, borderRadius: "50%", background: c.fg }} />
      {c.label}
    </span>
  );
}

function blobIcon(iconKey: BlobRow["icon"]): ReactNode {
  const size = 13;
  switch (iconKey) {
    case "folder":  return <IconFolder size={size} style={{ color: "var(--yellow)" }} />;
    case "parquet": return <IconFileCode size={size} style={{ color: "var(--blue)" }} />;
    case "json":    return <IconFileCode size={size} style={{ color: "var(--yellow)" }} />;
    case "archive": return <IconFileArchive size={size} style={{ color: "var(--purple)" }} />;
    case "image":   return <IconFileImage size={size} style={{ color: "var(--green)" }} />;
    default:        return <IconFile size={size} style={{ color: "var(--fg-2)" }} />;
  }
}

interface BlobTableProps {
  rows: BlobRow[];
  selected: Set<number>;
  onToggleSelect: (i: number) => void;
  onSelectRow: (i: number, event: React.MouseEvent<HTMLDivElement>) => void;
  onSelectAll: () => void;
  onDelete: () => void;
  onActivateRow?: (i: number) => void;
  onContextMenuRow?: (index: number, row: BlobRow, event: React.MouseEvent<HTMLDivElement>) => void;
}
export function BlobTable({
  rows,
  selected,
  onToggleSelect,
  onSelectRow,
  onSelectAll,
  onActivateRow,
  onContextMenuRow,
}: BlobTableProps) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [tableWidth, setTableWidth] = useState(0);
  const [columnWidths, setColumnWidths] = useState<Record<string, number>>({});

  useEffect(() => {
    const node = rootRef.current;
    if (!node) {
      return;
    }

    const update = () => setTableWidth(node.clientWidth);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const availableWidth = tableWidth || 900;
  const cols = [
    { key: "name", label: "Name", sortable: true, template: "minmax(180px, 2.4fr)", minWidth: 0 },
    { key: "mod", label: "Last Modified", sortable: true, sorted: "desc" as const, template: "minmax(132px, 0.9fr)", minWidth: 420 },
    { key: "size", label: "Size", align: "right" as const, template: "minmax(72px, 0.38fr)", minWidth: 560 },
    { key: "tier", label: "Access Tier", template: "minmax(82px, 0.42fr)", minWidth: 680 },
    { key: "blobType", label: "Blob Type", template: "minmax(78px, 0.42fr)", minWidth: 800 },
    { key: "tierMod", label: "Tier Last Modified", template: "minmax(132px, 0.7fr)", minWidth: 960 },
    { key: "lease", label: "Lease", template: "minmax(72px, 0.34fr)", minWidth: 1120 },
    { key: "etag", label: "ETag", template: "minmax(132px, 0.65fr)", minWidth: 1280 },
  ].filter((column) => availableWidth >= column.minWidth);

  const gridTemplate = `24px ${cols.map((column) => columnWidths[column.key] ? `${columnWidths[column.key]}px` : column.template).join(" ")}`;
  const allSelected = rows.length > 0 && selected.size === rows.length;

  const handleColumnResizeStart = (event: React.MouseEvent<HTMLSpanElement>, key: string) => {
    event.preventDefault();
    event.stopPropagation();

    const startX = event.clientX;
    const startWidth = event.currentTarget.parentElement?.getBoundingClientRect().width ?? columnWidths[key] ?? 120;
    const previousCursor = document.body.style.cursor;
    const previousUserSelect = document.body.style.userSelect;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    const applyWidth = (clientX: number) => {
      const nextWidth = Math.min(720, Math.max(64, Math.round(startWidth + clientX - startX)));
      setColumnWidths((current) => ({ ...current, [key]: nextWidth }));
    };

    const handleMouseMove = (moveEvent: MouseEvent) => {
      applyWidth(moveEvent.clientX);
    };
    const handleMouseUp = () => {
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousUserSelect;
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    applyWidth(event.clientX);
  };

  const renderCell = (r: BlobRow, key: string, isSelected: boolean) => {
    switch (key) {
      case "name":
        return (
          <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "0 10px", overflow: "hidden", minWidth: 0 }}>
            <span style={{ display: "flex", alignItems: "center", justifyContent: "center", color: r.kind === "dir" ? "var(--yellow)" : "var(--fg-2)" }}>
              {blobIcon(r.icon)}
            </span>
            <span style={{
              flex: 1,
              minWidth: 0,
              overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              color: isSelected ? "#ffffff" : r.kind === "dir" ? "#f5f8ff" : "var(--fg-0)",
              fontFamily: "var(--sans)",
              fontSize: 12.5,
              letterSpacing: "0.01em",
              fontWeight: isSelected || r.kind === "dir" ? 650 : 520,
            }} title={r.name}>{r.name}</span>
            {r.kind === "dir" && <IconChevronRight size={9} style={{ color: "var(--fg-3)" }} />}
          </div>
        );
      case "tier":
        return (
          <div style={{ display: "flex", alignItems: "center", padding: "0 8px" }}>
            <TierPill tier={r.tier} />
          </div>
        );
      case "tierMod":
        return <div style={{ display: "flex", alignItems: "center", padding: "0 8px", color: "var(--fg-3)" }}>{r.tier ? r.modified || "—" : "—"}</div>;
      case "mod":
        return <div style={{ display: "flex", alignItems: "center", padding: "0 8px", color: "var(--fg-2)", overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" }}>{r.modified || "—"}</div>;
      case "size":
        return <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", padding: "0 8px", color: r.size ? "var(--fg-1)" : "var(--fg-3)" }}>{r.size || "—"}</div>;
      case "blobType":
        return <div style={{ display: "flex", alignItems: "center", padding: "0 8px", color: "var(--fg-3)" }}>{r.kind === "blob" ? "Block" : "—"}</div>;
      case "etag":
        return (
          <div style={{ display: "flex", alignItems: "center", padding: "0 8px", color: "var(--fg-3)", overflow: "hidden" }}>
            {r.etag ? <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{r.etag}</span> : "—"}
          </div>
        );
      case "lease":
        return (
          <div style={{ display: "flex", alignItems: "center", padding: "0 8px", color: "var(--fg-3)" }}>
            {r.lease === "avail" ? (
              <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                <IconUnlock size={10} style={{ color: "var(--green)" }} /> avail
              </span>
            ) : r.lease === "leased" ? (
              <span style={{ display: "inline-flex", alignItems: "center", gap: 4, color: "var(--yellow)" }}>
                <IconLock size={10} /> leased
              </span>
            ) : "—"}
          </div>
        );
      default:
        return null;
    }
  };

  return (
    <div ref={rootRef} style={{
      flex: 1,
      overflow: "auto",
      background: "var(--bg-1)",
      fontFamily: "var(--mono)", fontSize: 11,
    }}>
      <div style={{
        display: "grid",
        gridTemplateColumns: gridTemplate,
        position: "sticky", top: 0, zIndex: 2,
        background: "var(--bg-2)",
        borderBottom: "1px solid var(--border-1)",
        fontSize: 10, fontWeight: 600,
        color: "var(--fg-2)", textTransform: "uppercase", letterSpacing: "0.04em",
        height: 28,
      }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
          <Checkbox checked={allSelected} onChange={onSelectAll} />
        </div>
        {cols.map((c) => (
          <div key={c.key} style={{
            position: "relative",
            display: "flex", alignItems: "center",
            justifyContent: c.align === "right" ? "flex-end" : "flex-start",
            padding: "0 14px 0 8px", gap: 4,
            borderRight: "1px solid var(--border-0)",
            cursor: c.sortable ? "pointer" : "default",
          }}>
            <span>{c.label}</span>
            {c.sorted === "desc" && <IconChevronDown size={9} style={{ color: "var(--accent)" }} />}
            <span
              aria-hidden="true"
              title="Drag to resize column"
              onMouseDown={(event) => handleColumnResizeStart(event, c.key)}
              style={{
                position: "absolute",
                top: 0,
                right: 0,
                width: 7,
                height: "100%",
                cursor: "col-resize",
                background: "linear-gradient(to right, transparent, transparent 3px, rgba(77, 166, 255, 0.28) 3px, rgba(77, 166, 255, 0.28) 4px, transparent 4px)",
              }}
            />
          </div>
        ))}
      </div>

      {rows.map((r, i) => {
        const isSelected = selected.has(i);
        return (
          <div
            key={i}
            onClick={(event) => onSelectRow(i, event)}
            onDoubleClick={() => onActivateRow?.(i)}
            onContextMenu={(event) => onContextMenuRow?.(i, r, event)}
            style={{
              display: "grid",
              gridTemplateColumns: gridTemplate,
              height: 30,
              borderBottom: "1px solid var(--border-0)",
              background: isSelected ? "var(--accent-ghost-strong)" : "transparent",
              cursor: "pointer",
              color: "var(--fg-1)",
            }}
            onMouseEnter={(e) => { if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = "var(--bg-2)"; }}
            onMouseLeave={(e) => { if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = "transparent"; }}
          >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
              <Checkbox checked={isSelected} onChange={() => onToggleSelect(i)} />
            </div>

            {cols.map((column) => (
              <React.Fragment key={column.key}>{renderCell(r, column.key, isSelected)}</React.Fragment>
            ))}
          </div>
        );
      })}
    </div>
  );
}

interface CheckboxProps {
  checked: boolean;
  onChange?: () => void;
}
export function Checkbox({ checked, onChange }: CheckboxProps) {
  return (
    <button
      onClick={(e) => { e.stopPropagation(); onChange?.(); }}
      style={{
        width: 13, height: 13, borderRadius: 2,
        border: `1px solid ${checked ? "var(--accent)" : "var(--border-2)"}`,
        background: checked ? "var(--accent)" : "transparent",
        display: "flex", alignItems: "center", justifyContent: "center",
        color: "#0a0a0c",
      }}
    >
      {checked && <IconCheck size={9} style={{ strokeWidth: 2.5 }} />}
    </button>
  );
}

// ─────────────────────────────────────────────────────────────
// Inspector
// ─────────────────────────────────────────────────────────────
interface InspectorProps {
  row: BlobRow | null;
  resourceUrl?: string | null;
  containerName?: string | null;
  endpoint?: string | null;
  authKind?: string | null;
}
export function Inspector({ row, resourceUrl, containerName, endpoint, authKind }: InspectorProps) {
  if (!row) return null;
  const kv = (k: string, v: ReactNode, mono = true) => (
    <div style={{ display: "flex", gap: 10, fontSize: 10, minHeight: 16 }}>
      <div style={{ width: 96, color: "var(--fg-3)", textTransform: "uppercase", letterSpacing: "0.04em", fontWeight: 600, fontSize: 9 }}>{k}</div>
      <div style={{ color: "var(--fg-1)", fontFamily: mono ? "var(--mono)" : "var(--sans)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1 }}>{v}</div>
    </div>
  );

  return (
    <div style={{
      padding: "8px 12px",
      fontFamily: "var(--mono)",
      display: "flex", flexDirection: "column", gap: 2,
    }}>
      <div style={{
        fontSize: 10, fontWeight: 600, color: "var(--fg-3)",
        textTransform: "uppercase", letterSpacing: "0.08em",
        paddingBottom: 6, borderBottom: "1px solid var(--border-0)",
        marginBottom: 6,
        display: "flex", alignItems: "center", gap: 6,
      }}>
        <IconInfo size={11} />
        <span>Properties</span>
        <span style={{ flex: 1 }} />
        <span style={{ color: "var(--fg-4)" }}>{row.kind === "dir" ? "VIRTUAL_DIR" : "BLOCK_BLOB"}</span>
      </div>
      {kv("name", row.name)}
      {row.path && kv("path", row.path)}
      {containerName && kv("container", containerName)}
      {resourceUrl && kv("url", resourceUrl)}
      {endpoint && kv("endpoint", endpoint)}
      {authKind && kv("auth", authKind, false)}
      {row.tier && kv("tier", row.tier)}
      {row.etag && kv("etag", row.etag)}
      {row.size && kv("size", row.size)}
      {kv("modified", row.modified || "—")}
      {row.lease && kv("lease state", row.lease === "avail" ? "available" : "leased")}
    </div>
  );
}
