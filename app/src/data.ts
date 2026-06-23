// Arkived — shared UI data types.
//
// These describe the shapes the Tauri backend (see `src/lib/ipc.ts`) returns to
// the React UI. No data lives here — every value shown in the app comes from a
// live IPC call into arkived-core.

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

export interface BreadcrumbEntry {
  label: string;
  kind: string;
}
