// Arkived — reusable promise-based dialogs.
//
// `useDialogs()` returns imperative helpers (`confirm`, `prompt`, `choose`,
// `editTags`, `generateSas`) that each resolve a Promise when the user acts,
// plus an `element` to render once near the app root. This lets handlers read
// like the old `window.prompt` flow — `const v = await dialogs.prompt(...)` —
// while presenting a real, themed modal instead of a browser dialog.

import React, { CSSProperties, useCallback, useRef, useState } from "react";

export interface ChoiceOption {
  value: string;
  label: string;
  detail?: string;
}

interface ConfirmConfig {
  kind: "confirm";
  title: string;
  subtitle?: string;
  message?: string;
  confirmLabel?: string;
  danger?: boolean;
}

interface PromptConfig {
  kind: "prompt";
  title: string;
  subtitle?: string;
  message?: string;
  label?: string;
  defaultValue?: string;
  placeholder?: string;
  confirmLabel?: string;
  validate?: (value: string) => string | null;
}

interface ChooseConfig {
  kind: "choose";
  title: string;
  subtitle?: string;
  options: ChoiceOption[];
  current?: string;
  confirmLabel?: string;
  toggle?: { label: string; default: boolean };
}

interface TagsConfig {
  kind: "tags";
  title: string;
  subtitle?: string;
  initial: Record<string, string>;
}

interface SasConfig {
  kind: "sas";
  title: string;
  subtitle?: string;
}

type DialogConfig = ConfirmConfig | PromptConfig | ChooseConfig | TagsConfig | SasConfig;

export interface ChooseResult {
  value: string;
  toggle: boolean;
}

export interface SasResult {
  permissions: string;
  hours: number;
}

const SAS_PERMS: { flag: string; label: string }[] = [
  { flag: "r", label: "Read" },
  { flag: "a", label: "Add" },
  { flag: "c", label: "Create" },
  { flag: "w", label: "Write" },
  { flag: "d", label: "Delete" },
  { flag: "l", label: "List" },
];

export interface DialogsApi {
  confirm: (config: Omit<ConfirmConfig, "kind">) => Promise<boolean>;
  prompt: (config: Omit<PromptConfig, "kind">) => Promise<string | null>;
  choose: (config: Omit<ChooseConfig, "kind">) => Promise<ChooseResult | null>;
  editTags: (config: Omit<TagsConfig, "kind">) => Promise<Record<string, string> | null>;
  generateSas: (config: Omit<SasConfig, "kind">) => Promise<SasResult | null>;
  element: React.ReactNode;
}

export function useDialogs(): DialogsApi {
  const [config, setConfig] = useState<DialogConfig | null>(null);
  const resolverRef = useRef<((value: unknown) => void) | null>(null);

  const open = useCallback(<T,>(next: DialogConfig): Promise<T> => {
    // Resolve any in-flight dialog as cancelled before replacing it.
    resolverRef.current?.(null);
    return new Promise<T>((resolve) => {
      resolverRef.current = resolve as (value: unknown) => void;
      setConfig(next);
    });
  }, []);

  const settle = useCallback((value: unknown) => {
    resolverRef.current?.(value);
    resolverRef.current = null;
    setConfig(null);
  }, []);

  const api: Omit<DialogsApi, "element"> = {
    confirm: (c) => open<boolean>({ ...c, kind: "confirm" }).then((v) => v === true),
    prompt: (c) => open<string | null>({ ...c, kind: "prompt" }),
    choose: (c) => open<ChooseResult | null>({ ...c, kind: "choose" }),
    editTags: (c) => open<Record<string, string> | null>({ ...c, kind: "tags" }),
    generateSas: (c) => open<SasResult | null>({ ...c, kind: "sas" }),
  };

  const element = config ? (
    <DialogHost
      // Remount per-dialog so local input state resets cleanly.
      key={dialogKey(config)}
      config={config}
      onCancel={() => settle(config.kind === "confirm" ? false : null)}
      onResolve={settle}
    />
  ) : null;

  return { ...api, element };
}

let dialogSeq = 0;
function dialogKey(config: DialogConfig): string {
  dialogSeq += 1;
  return `${config.kind}-${dialogSeq}`;
}

function DialogHost({
  config,
  onCancel,
  onResolve,
}: {
  config: DialogConfig;
  onCancel: () => void;
  onResolve: (value: unknown) => void;
}) {
  return (
    <div
      style={styles.overlay}
      onClick={onCancel}
      onKeyDown={(event) => {
        if (event.key === "Escape") onCancel();
      }}
    >
      <div style={styles.card} onClick={(event) => event.stopPropagation()}>
        <div style={styles.header}>
          <div>
            {"subtitle" in config && config.subtitle && (
              <div style={styles.eyebrow}>{config.subtitle}</div>
            )}
            <h2 style={styles.title}>{config.title}</h2>
          </div>
          <button type="button" style={styles.closeButton} onClick={onCancel}>
            Close
          </button>
        </div>
        {config.kind === "confirm" && <ConfirmBody config={config} onCancel={onCancel} onResolve={onResolve} />}
        {config.kind === "prompt" && <PromptBody config={config} onCancel={onCancel} onResolve={onResolve} />}
        {config.kind === "choose" && <ChooseBody config={config} onCancel={onCancel} onResolve={onResolve} />}
        {config.kind === "tags" && <TagsBody config={config} onCancel={onCancel} onResolve={onResolve} />}
        {config.kind === "sas" && <SasBody config={config} onCancel={onCancel} onResolve={onResolve} />}
      </div>
    </div>
  );
}

function Footer({
  onCancel,
  onConfirm,
  confirmLabel,
  danger,
  disabled,
}: {
  onCancel: () => void;
  onConfirm: () => void;
  confirmLabel: string;
  danger?: boolean;
  disabled?: boolean;
}) {
  return (
    <div style={styles.footer}>
      <button type="button" style={styles.secondaryButton} onClick={onCancel}>
        Cancel
      </button>
      <button
        type="submit"
        style={{
          ...(danger ? styles.dangerButton : styles.primaryButton),
          opacity: disabled ? 0.6 : 1,
          cursor: disabled ? "default" : "pointer",
        }}
        onClick={onConfirm}
        disabled={disabled}
      >
        {confirmLabel}
      </button>
    </div>
  );
}

function ConfirmBody({
  config,
  onCancel,
  onResolve,
}: {
  config: ConfirmConfig;
  onCancel: () => void;
  onResolve: (value: unknown) => void;
}) {
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        onResolve(true);
      }}
    >
      {config.message && <div style={styles.body}>{config.message}</div>}
      <Footer
        onCancel={onCancel}
        onConfirm={() => onResolve(true)}
        confirmLabel={config.confirmLabel ?? (config.danger ? "Delete" : "Confirm")}
        danger={config.danger}
      />
    </form>
  );
}

function PromptBody({
  config,
  onCancel,
  onResolve,
}: {
  config: PromptConfig;
  onCancel: () => void;
  onResolve: (value: unknown) => void;
}) {
  const [value, setValue] = useState(config.defaultValue ?? "");
  const error = config.validate ? config.validate(value) : null;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!error) onResolve(value);
      }}
    >
      <div style={styles.body}>
        {config.message && <div style={styles.bodyText}>{config.message}</div>}
        {config.label && <label style={styles.fieldLabel}>{config.label}</label>}
        <input
          autoFocus
          value={value}
          placeholder={config.placeholder}
          onChange={(event) => setValue(event.target.value)}
          style={styles.input}
        />
        {error && value.length > 0 && <div style={styles.error}>{error}</div>}
      </div>
      <Footer
        onCancel={onCancel}
        onConfirm={() => !error && onResolve(value)}
        confirmLabel={config.confirmLabel ?? "OK"}
        disabled={!!error}
      />
    </form>
  );
}

function ChooseBody({
  config,
  onCancel,
  onResolve,
}: {
  config: ChooseConfig;
  onCancel: () => void;
  onResolve: (value: unknown) => void;
}) {
  const [selected, setSelected] = useState(config.current ?? config.options[0]?.value ?? "");
  const [toggle, setToggle] = useState(config.toggle?.default ?? false);
  const unchanged = config.current != null && selected === config.current && !config.toggle;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!unchanged) onResolve({ value: selected, toggle });
      }}
    >
      <div style={{ ...styles.body, gap: 10 }}>
        {config.options.map((option) => {
          const active = selected === option.value;
          return (
            <button
              key={option.value}
              type="button"
              onClick={() => setSelected(option.value)}
              style={{
                ...styles.optionCard,
                border: active ? "1px solid var(--accent-dim)" : "1px solid var(--border-1)",
                background: active ? "var(--accent-ghost)" : "var(--bg-2)",
              }}
            >
              <span
                style={{
                  ...styles.radio,
                  border: active ? "5px solid var(--accent)" : "2px solid var(--fg-3)",
                  background: active ? "var(--bg-1)" : "transparent",
                }}
              />
              <span style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <span style={styles.optionLabel}>
                  {option.label}
                  {config.current === option.value && <span style={styles.currentBadge}>Current</span>}
                </span>
                {option.detail && <span style={styles.optionDetail}>{option.detail}</span>}
              </span>
            </button>
          );
        })}
        {config.toggle && (
          <label style={styles.toggleRow}>
            <input
              type="checkbox"
              checked={toggle}
              onChange={(event) => setToggle(event.target.checked)}
            />
            <span>{config.toggle.label}</span>
          </label>
        )}
      </div>
      <Footer
        onCancel={onCancel}
        onConfirm={() => !unchanged && onResolve({ value: selected, toggle })}
        confirmLabel={config.confirmLabel ?? "Apply"}
        disabled={unchanged}
      />
    </form>
  );
}

interface TagRow {
  key: string;
  value: string;
}

function TagsBody({
  config,
  onCancel,
  onResolve,
}: {
  config: TagsConfig;
  onCancel: () => void;
  onResolve: (value: unknown) => void;
}) {
  const initialRows: TagRow[] = Object.entries(config.initial).map(([key, value]) => ({ key, value }));
  const [rows, setRows] = useState<TagRow[]>(initialRows.length ? initialRows : [{ key: "", value: "" }]);

  function update(index: number, patch: Partial<TagRow>) {
    setRows((current) => current.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }
  function removeRow(index: number) {
    setRows((current) => (current.length === 1 ? [{ key: "", value: "" }] : current.filter((_, i) => i !== index)));
  }

  function submit() {
    const tags: Record<string, string> = {};
    for (const row of rows) {
      const key = row.key.trim();
      if (key) tags[key] = row.value.trim();
    }
    onResolve(tags);
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <div style={styles.body}>
        <div style={styles.bodyText}>Blob index tags are key/value pairs you can filter on. Empty keys are dropped.</div>
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {rows.map((row, index) => (
            <div key={index} style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <input
                value={row.key}
                placeholder="key"
                onChange={(event) => update(index, { key: event.target.value })}
                style={{ ...styles.input, flex: 1 }}
                autoFocus={index === 0}
              />
              <span style={{ color: "var(--fg-3)" }}>=</span>
              <input
                value={row.value}
                placeholder="value"
                onChange={(event) => update(index, { value: event.target.value })}
                style={{ ...styles.input, flex: 1 }}
              />
              <button type="button" style={styles.iconButton} onClick={() => removeRow(index)} title="Remove tag">
                ×
              </button>
            </div>
          ))}
        </div>
        <button
          type="button"
          style={styles.addRowButton}
          onClick={() => setRows((current) => [...current, { key: "", value: "" }])}
        >
          + Add tag
        </button>
      </div>
      <Footer onCancel={onCancel} onConfirm={submit} confirmLabel="Save tags" />
    </form>
  );
}

function SasBody({
  config,
  onCancel,
  onResolve,
}: {
  config: SasConfig;
  onCancel: () => void;
  onResolve: (value: unknown) => void;
}) {
  const [perms, setPerms] = useState<Record<string, boolean>>({ r: true });
  const [hoursText, setHoursText] = useState("1");
  const hours = Number.parseInt(hoursText, 10);
  const flags = SAS_PERMS.filter((p) => perms[p.flag]).map((p) => p.flag);
  const permError = flags.length === 0 ? "Select at least one permission." : null;
  const hoursError = !Number.isFinite(hours) || hours <= 0 ? "Enter a positive number of hours." : null;
  const disabled = !!permError || !!hoursError;

  function submit() {
    if (disabled) return;
    onResolve({ permissions: flags.join(""), hours });
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <div style={styles.body}>
        <label style={styles.fieldLabel}>Permissions</label>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
          {SAS_PERMS.map((perm) => {
            const on = !!perms[perm.flag];
            return (
              <button
                key={perm.flag}
                type="button"
                onClick={() => setPerms((current) => ({ ...current, [perm.flag]: !on }))}
                style={{
                  ...styles.permChip,
                  border: on ? "1px solid var(--accent-dim)" : "1px solid var(--border-1)",
                  background: on ? "var(--accent-ghost)" : "var(--bg-2)",
                  color: on ? "var(--fg-0)" : "var(--fg-2)",
                }}
              >
                {perm.label}
              </button>
            );
          })}
        </div>
        {permError && <div style={styles.error}>{permError}</div>}
        <label style={{ ...styles.fieldLabel, marginTop: 4 }}>Expires in (hours)</label>
        <input
          value={hoursText}
          inputMode="numeric"
          onChange={(event) => setHoursText(event.target.value)}
          style={{ ...styles.input, width: 140 }}
        />
        {hoursError && <div style={styles.error}>{hoursError}</div>}
      </div>
      <Footer
        onCancel={onCancel}
        onConfirm={submit}
        confirmLabel="Generate & copy"
        disabled={disabled}
      />
    </form>
  );
}

const styles: Record<string, CSSProperties> = {
  overlay: {
    position: "fixed",
    inset: 0,
    background: "rgba(6,6,8,0.76)",
    backdropFilter: "blur(10px)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    padding: 24,
    zIndex: 20,
  },
  card: {
    width: "min(560px, 100%)",
    borderRadius: 16,
    overflow: "hidden",
    border: "1px solid var(--border-1)",
    background: "var(--bg-1)",
    boxShadow: "0 28px 90px rgba(0,0,0,0.45)",
    animation: "arkived-scale-in 160ms ease-out",
  },
  header: {
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    gap: 16,
    padding: "20px 22px 16px",
    borderBottom: "1px solid var(--border-0)",
  },
  eyebrow: {
    fontSize: 10,
    fontFamily: "var(--mono)",
    textTransform: "uppercase",
    letterSpacing: "0.08em",
    color: "var(--fg-3)",
    marginBottom: 6,
  },
  title: {
    margin: 0,
    fontSize: 20,
    lineHeight: 1.1,
    color: "var(--fg-0)",
  },
  closeButton: {
    height: 30,
    padding: "0 12px",
    borderRadius: 8,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-1)",
    fontFamily: "var(--mono)",
    fontSize: 11,
    flexShrink: 0,
  },
  body: {
    padding: "18px 22px",
    display: "flex",
    flexDirection: "column",
    gap: 10,
    color: "var(--fg-1)",
    fontSize: 13,
    lineHeight: 1.55,
  },
  bodyText: {
    color: "var(--fg-2)",
    fontSize: 12,
    lineHeight: 1.55,
  },
  fieldLabel: {
    fontSize: 10,
    fontFamily: "var(--mono)",
    textTransform: "uppercase",
    letterSpacing: "0.06em",
    color: "var(--fg-3)",
  },
  input: {
    height: 34,
    padding: "0 10px",
    borderRadius: 8,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-0)",
    fontFamily: "var(--mono)",
    fontSize: 12,
  },
  error: {
    color: "var(--red, #f06363)",
    fontSize: 11,
  },
  optionCard: {
    display: "flex",
    alignItems: "flex-start",
    gap: 12,
    textAlign: "left",
    padding: "12px 14px",
    borderRadius: 12,
    cursor: "pointer",
  },
  radio: {
    marginTop: 2,
    width: 16,
    height: 16,
    flexShrink: 0,
    borderRadius: "50%",
    boxSizing: "border-box",
  },
  optionLabel: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    fontSize: 13,
    fontWeight: 600,
    color: "var(--fg-0)",
  },
  optionDetail: {
    fontSize: 11,
    lineHeight: 1.5,
    color: "var(--fg-2)",
  },
  currentBadge: {
    fontSize: 9,
    fontWeight: 600,
    fontFamily: "var(--mono)",
    textTransform: "uppercase",
    letterSpacing: "0.06em",
    padding: "1px 5px",
    borderRadius: 4,
    background: "var(--bg-3)",
    color: "var(--fg-3)",
    border: "1px solid var(--border-1)",
  },
  toggleRow: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    fontSize: 12,
    color: "var(--fg-1)",
    cursor: "pointer",
    marginTop: 2,
  },
  permChip: {
    height: 30,
    padding: "0 12px",
    borderRadius: 8,
    fontSize: 12,
    fontWeight: 500,
    cursor: "pointer",
  },
  iconButton: {
    width: 28,
    height: 28,
    flexShrink: 0,
    borderRadius: 8,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-2)",
    fontSize: 16,
    lineHeight: 1,
    cursor: "pointer",
  },
  addRowButton: {
    alignSelf: "flex-start",
    height: 30,
    padding: "0 12px",
    borderRadius: 8,
    border: "1px dashed var(--border-1)",
    background: "transparent",
    color: "var(--fg-2)",
    fontSize: 11,
    fontFamily: "var(--mono)",
    cursor: "pointer",
  },
  footer: {
    display: "flex",
    justifyContent: "flex-end",
    gap: 10,
    padding: "0 22px 20px",
  },
  primaryButton: {
    height: 36,
    padding: "0 16px",
    borderRadius: 10,
    border: "1px solid rgba(63, 157, 246, 0.45)",
    background: "linear-gradient(180deg, rgba(85, 170, 247, 1) 0%, rgba(63, 157, 246, 1) 100%)",
    color: "#07111d",
    fontWeight: 700,
    fontSize: 12,
  },
  dangerButton: {
    height: 36,
    padding: "0 16px",
    borderRadius: 10,
    border: "1px solid rgba(240, 99, 99, 0.45)",
    background: "linear-gradient(180deg, rgba(228, 96, 96, 1) 0%, rgba(206, 74, 74, 1) 100%)",
    color: "#1d0707",
    fontWeight: 700,
    fontSize: 12,
  },
  secondaryButton: {
    height: 36,
    padding: "0 14px",
    borderRadius: 9,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-1)",
    fontFamily: "var(--mono)",
    fontSize: 11,
  },
};
