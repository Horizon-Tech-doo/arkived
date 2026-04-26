// Arkived — Data model + mock fallback
// Backend will eventually supply this via Tauri IPC; we also keep the
// design-prototype mock inline so the UI renders without a running backend.

export interface Container {
  id: string;
  name: string;
  kind: string;
  public: string;
  lease: string;
  blobs: number;
  selected?: boolean;
}

export interface FileShare {
  id: string;
  name: string;
  quota: string;
}

export interface Queue {
  id: string;
  name: string;
  messages: number;
}

export interface TableResource {
  id: string;
  name: string;
  entities: number;
}

export interface StorageAccount {
  id: string;
  name: string;
  kind: string;
  region: string;
  replication: string;
  tier: string;
  hns: boolean;
  expanded?: boolean;
  containers: Container[];
  fileShares?: FileShare[];
  queues?: Queue[];
  tables?: TableResource[];
}

export interface Subscription {
  id: string;
  name: string;
  owner: string;
  accounts: StorageAccount[];
}

export const DATA: { subscriptions: Subscription[] } = {
  subscriptions: [
    {
      id: "sub-dev",
      name: "din — development",
      owner: "hamza.abdagic@pontesolutions",
      accounts: [
        {
          id: "stdlnphoenixdevfunc",
          name: "stdlnphoenixdevfunc",
          kind: "StorageV2",
          region: "West Europe",
          replication: "LRS",
          tier: "Standard",
          hns: false,
          containers: [],
        },
        {
          id: "stdlnphoenixproddlp",
          name: "stdlnphoenixproddlp",
          kind: "StorageV2 (ADLS Gen2)",
          region: "West Europe",
          replication: "LRS",
          tier: "Premium",
          hns: true,
          expanded: true,
          containers: [
            { id: "device-configs", name: "device-configs", kind: "blob", public: "none", lease: "available", blobs: 342 },
            { id: "device-twins", name: "device-twins", kind: "blob", public: "none", lease: "available", blobs: 12843, selected: true },
            { id: "pipeline-output", name: "pipeline-output", kind: "blob", public: "none", lease: "leased", blobs: 5421 },
            { id: "raw-device-telemetry", name: "raw-device-telemetry", kind: "blob", public: "none", lease: "available", blobs: 88210 },
            { id: "support-files", name: "support-files", kind: "blob", public: "blob", lease: "available", blobs: 47 },
          ],
          fileShares: [{ id: "shared-configs", name: "shared-configs", quota: "512 GiB" }],
          queues: [
            { id: "ingress-q", name: "ingress-q", messages: 412 },
            { id: "dlq-telemetry", name: "dlq-telemetry", messages: 3 },
          ],
          tables: [
            { id: "DeviceRegistry", name: "DeviceRegistry", entities: 14820 },
            { id: "AuditLog", name: "AuditLog", entities: 921003 },
          ],
        },
        {
          id: "stdlnphoenixprodfunc",
          name: "stdlnphoenixprodfunc",
          kind: "StorageV2",
          region: "West Europe",
          replication: "GRS",
          tier: "Standard",
          hns: false,
          containers: [],
        },
        {
          id: "stdlnphoenixstate",
          name: "stdlnphoenixstate",
          kind: "StorageV2",
          region: "West Europe",
          replication: "ZRS",
          tier: "Standard",
          hns: false,
          containers: [],
        },
      ],
    },
    {
      id: "sub-hz",
      name: "Horizon Tech — Prod",
      owner: "hamza.abdagic@horizon",
      accounts: [],
    },
    {
      id: "sub-spon",
      name: "Microsoft Azure Sponsorship",
      owner: "hamza.abdagic@pontesolutions",
      accounts: [],
    },
  ],
};

export interface BreadcrumbEntry {
  label: string;
  kind: string;
}

export const BREADCRUMB_BLOB: BreadcrumbEntry[] = [
  { label: "stdlnphoenixproddlp", kind: "account" },
  { label: "device-twins", kind: "container" },
  { label: "device-twins-sync", kind: "dir" },
  { label: "createdAt_Year=2026", kind: "dir" },
  { label: "createdAt_Month=4", kind: "dir" },
  { label: "createdAt_Day=20", kind: "dir" },
  { label: "createdAt_Hour=9", kind: "dir" },
];

export interface BlobRow {
  path?: string;
  name: string;
  kind: "dir" | "blob";
  blob_type?: string | null;
  size: string | null;
  size_bytes?: number | null;
  tier: string | null;
  modified: string;
  etag: string | null;
  lease: string | null;
  icon: "folder" | "parquet" | "json" | "archive" | "image" | "file";
}

export const BLOB_ROWS: BlobRow[] = [
  { name: "deviceSerialNumber_S=DA000405", kind: "dir", size: null, tier: null, modified: "2026-04-20 11:12:04", etag: null, lease: null, icon: "folder" },
  { name: "deviceSerialNumber_S=DA00044C", kind: "dir", size: null, tier: null, modified: "2026-04-20 11:12:06", etag: null, lease: null, icon: "folder" },
  { name: "deviceSerialNumber_S=DA000488", kind: "dir", size: null, tier: null, modified: "2026-04-20 11:12:09", etag: null, lease: null, icon: "folder" },
  { name: "deviceSerialNumber_S=DA0004F8", kind: "dir", size: null, tier: null, modified: "2026-04-20 11:12:11", etag: null, lease: null, icon: "folder" },
  { name: "deviceSerialNumber_S=DA000514", kind: "dir", size: null, tier: null, modified: "2026-04-20 11:12:14", etag: null, lease: null, icon: "folder" },
  { name: "deviceSerialNumber_S=EA0004EA", kind: "dir", size: null, tier: null, modified: "2026-04-20 11:12:18", etag: null, lease: null, icon: "folder" },
  { name: "part-00001-a2f4b8c.c000.snappy.parquet", kind: "blob", size: "14.2 MiB", tier: "Hot", modified: "2026-04-20 11:11:42", etag: "0x8DC7A9F21B4E2C1", lease: "avail", icon: "parquet" },
  { name: "part-00002-a2f4b8c.c000.snappy.parquet", kind: "blob", size: "14.8 MiB", tier: "Hot", modified: "2026-04-20 11:11:58", etag: "0x8DC7A9F21B52088", lease: "avail", icon: "parquet" },
  { name: "part-00003-a2f4b8c.c000.snappy.parquet", kind: "blob", size: "13.9 MiB", tier: "Hot", modified: "2026-04-20 11:12:03", etag: "0x8DC7A9F21B55A12", lease: "avail", icon: "parquet" },
  { name: "_delta_log/00000000000000000412.json", kind: "blob", size: "4.1 KiB", tier: "Hot", modified: "2026-04-20 11:12:21", etag: "0x8DC7A9F21B5F8CB", lease: "avail", icon: "json" },
  { name: "_delta_log/00000000000000000412.checkpoint.parquet", kind: "blob", size: "1.3 MiB", tier: "Hot", modified: "2026-04-20 11:12:22", etag: "0x8DC7A9F21B6108A", lease: "avail", icon: "parquet" },
  { name: "manifest.json", kind: "blob", size: "812 B", tier: "Hot", modified: "2026-04-20 11:12:25", etag: "0x8DC7A9F21B64771", lease: "avail", icon: "json" },
  { name: "_SUCCESS", kind: "blob", size: "0 B", tier: "Hot", modified: "2026-04-20 11:12:26", etag: "0x8DC7A9F21B647A9", lease: "avail", icon: "file" },
  { name: "archive-2026-04-19.tar.zst", kind: "blob", size: "482 MiB", tier: "Cool", modified: "2026-04-19 23:59:12", etag: "0x8DC79FE117220A3", lease: "avail", icon: "archive" },
  { name: "archive-2026-04-18.tar.zst", kind: "blob", size: "511 MiB", tier: "Cold", modified: "2026-04-18 23:59:08", etag: "0x8DC78E2D01DA0FF", lease: "avail", icon: "archive" },
  { name: "legacy/backup-q4-2025.tar.zst", kind: "blob", size: "2.1 GiB", tier: "Archive", modified: "2025-12-31 23:58:41", etag: "0x8DC401119AA0B2E", lease: "avail", icon: "archive" },
];

export interface Activity {
  id: string;
  kind: "delete" | "upload" | "download" | "copy";
  status: "running" | "done" | "error" | "cancelled";
  title: string;
  detail: string;
  started: string;
  duration?: string;
  result?: string;
  progress?: number;
}

export const ACTIVITIES: Activity[] = [
  {
    id: "a1",
    kind: "delete",
    status: "done",
    title: "Deletion of 'createdAt_Year=2026'",
    detail: "from 'device-twins/device-twins-sync'",
    started: "2026-04-20 11:12:55",
    duration: "4s",
    result: "1 deleted (used name + key)",
  },
  {
    id: "a2",
    kind: "delete",
    status: "done",
    title: "Deletion of 'device-twins-sync'",
    detail: "from 'device-twins'",
    started: "2026-04-20 11:09:02",
    duration: "4s",
    result: "1 deleted (used name + key)",
  },
  {
    id: "a3",
    kind: "upload",
    status: "running",
    title: "Upload 14 files → 'device-twins-sync/createdAt_Year=2026/...'",
    detail: "3 of 14 complete · 42 MiB/s",
    started: "2026-04-20 11:13:18",
    progress: 0.34,
  },
];

export interface ImpactRow {
  kind: "info" | "neutral" | "warn" | "danger";
  label: string;
  value: string;
}

export interface ConfirmPart {
  kind: "confirm";
  title: string;
  summary: string;
  cmd: string;
  impact: ImpactRow[];
}

export interface ToolPart {
  kind: "tool";
  name: string;
  status: "ok" | "run" | "err";
  args: Record<string, unknown>;
  result?: string;
  duration: string;
}

export interface TextPart {
  kind: "text";
  text: string;
}

export type MessagePart = TextPart | ToolPart | ConfirmPart;

export interface AssistantMessage {
  role: "assistant";
  at: string;
  parts: MessagePart[];
}

export interface UserMessage {
  role: "user";
  at: string;
  text: string;
}

export type TranscriptMessage = UserMessage | AssistantMessage;

export const AGENT_TRANSCRIPT: TranscriptMessage[] = [
  {
    role: "user",
    text: "find the most recent parquet file in device-twins-sync and show me its size",
    at: "11:09",
  },
  {
    role: "assistant",
    at: "11:09",
    parts: [
      { kind: "text", text: "I'll scan the hive-partitioned path, sorting by modified time. This is a read-only listing — no confirmation needed." },
      {
        kind: "tool",
        name: "azure.blob.list",
        status: "ok",
        args: {
          account: "stdlnphoenixproddlp",
          container: "device-twins",
          prefix: "device-twins-sync/",
          recursive: true,
          orderBy: "lastModified:desc",
          limit: 1,
        },
        result: "part-00003-a2f4b8c.c000.snappy.parquet · 13.9 MiB · 2026-04-20 11:12:03",
        duration: "820ms",
      },
      { kind: "text", text: "Most recent: **part-00003-a2f4b8c.c000.snappy.parquet** — 13.9 MiB, modified 2m ago in `createdAt_Year=2026/Month=4/Day=20/Hour=9/`." },
    ],
  },
  {
    role: "user",
    text: "now archive everything in createdAt_Year=2026/Month=4/Day=18 — move tier to Cool",
    at: "11:12",
  },
  {
    role: "assistant",
    at: "11:12",
    parts: [
      { kind: "text", text: "Changing access tier is a billable, policy-gated operation. I'll preview the plan and ask for confirmation before touching anything." },
      {
        kind: "tool",
        name: "azure.blob.plan-tier-change",
        status: "ok",
        args: {
          account: "stdlnphoenixproddlp",
          container: "device-twins",
          prefix: "device-twins-sync/createdAt_Year=2026/createdAt_Month=4/createdAt_Day=18/",
          target: "Cool",
          dryRun: true,
        },
        result: "284 blobs · 3.9 GiB total · current tier: Hot · estimated rehydration: n/a",
        duration: "2.1s",
      },
      {
        kind: "confirm",
        title: "Confirm tier change",
        summary: "Set 284 blobs to Cool tier",
        cmd: "arkived blob set-tier --account stdlnphoenixproddlp --container device-twins \\\n  --prefix 'device-twins-sync/createdAt_Year=2026/createdAt_Month=4/createdAt_Day=18/' \\\n  --tier Cool --recursive",
        impact: [
          { kind: "info", label: "Scope", value: "284 blobs · 3.9 GiB" },
          { kind: "neutral", label: "Current tier", value: "Hot" },
          { kind: "warn", label: "New tier", value: "Cool (30d min retention)" },
          { kind: "warn", label: "Early-delete fee", value: "~$0.12 if deleted <30d" },
          { kind: "info", label: "Reversible", value: "Yes — can re-tier to Hot" },
        ],
      },
    ],
  },
];
