// Arkived — desktop auto-update helpers (Tauri updater plugin).
//
// check() asks the configured endpoint (the GitHub Releases `latest.json`)
// whether a newer signed build exists. downloadAndInstall() fetches it,
// verifies its signature against the bundled public key, installs it, and we
// relaunch into the new version.

import type { Update } from "@tauri-apps/plugin-updater";

export async function checkForUpdate(): Promise<Update | null> {
  const { check } = await import("@tauri-apps/plugin-updater");
  return check();
}

export async function installUpdate(
  update: Update,
  onProgress?: (percent: number | null) => void,
): Promise<void> {
  const { relaunch } = await import("@tauri-apps/plugin-process");
  let downloaded = 0;
  let total = 0;
  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? 0;
        onProgress?.(total ? 0 : null);
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.(total ? Math.min(100, Math.round((downloaded / total) * 100)) : null);
        break;
      case "Finished":
        onProgress?.(100);
        break;
    }
  });
  // The new version is installed; restart into it.
  await relaunch();
}
