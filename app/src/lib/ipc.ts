// Thin wrapper around Tauri's invoke() with graceful fallback when running
// in a plain browser (e.g. `vite dev`). This lets the design render during
// frontend-only iteration.

import type { BlobRow, Activity } from "../data";

async function callTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (typeof window === "undefined") return null;
  // @ts-expect-error — optional runtime-only global injected by Tauri
  if (!window.__TAURI_INTERNALS__) return null;
  const mod = await import("@tauri-apps/api/core");
  return mod.invoke<T>(cmd, args);
}

export async function fetchBlobs(account: string, container: string, prefix: string): Promise<BlobRow[] | null> {
  return callTauri<BlobRow[]>("list_blobs", { account, container, prefix });
}

export async function fetchActivities(): Promise<Activity[] | null> {
  return callTauri<Activity[]>("list_activities");
}
