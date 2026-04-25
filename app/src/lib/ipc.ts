import type { BlobRow, Activity } from "../data";

export interface BrowserConnection {
  id: string;
  display_name: string;
  account_name: string;
  endpoint: string;
  auth_kind: string;
  fixed_container?: string | null;
  origin_sign_in_id?: string | null;
  origin_subscription_id?: string | null;
}

export interface BrowserSignIn {
  id: string;
  display_name: string;
  tenant: string;
  environment: string;
  subscription_count: number;
  selected_subscription_count: number;
  tenant_count: number;
  selected_tenant_count: number;
}

export interface BrowserTenant {
  id: string;
  sign_in_id: string;
  display_name: string;
  default_domain?: string | null;
  selected: boolean;
  needs_reauth: boolean;
  error?: string | null;
  subscription_count: number;
  selected_subscription_count: number;
  storage_account_count: number;
  subscriptions: BrowserSubscription[];
}

export interface BrowserSubscription {
  id: string;
  sign_in_id: string;
  name: string;
  tenant_id: string;
  tenant_label: string;
  storage_account_count: number;
  selected: boolean;
}

export interface BrowserStorageAccount {
  sign_in_id: string;
  subscription_id: string;
  name: string;
  kind: string;
  region: string;
  replication: string;
  tier: string;
  hns: boolean;
  endpoint: string;
}

export interface BrowserContainer {
  id: string;
  name: string;
  public_access?: string | null;
  lease?: string | null;
  blob_count?: number | null;
}

export interface DeviceCodePrompt {
  login_id: string;
  verification_uri: string;
  user_code: string;
  message: string;
  expires_in_seconds: number;
  interval_seconds: number;
}

export interface BrowserLoginPrompt {
  login_id: string;
  authorize_url: string;
  redirect_uri: string;
}

export interface BlobDownloadResult {
  path: string;
  bytes: number;
  opened: boolean;
}

export interface BlobUploadResult {
  path: string;
  bytes: number;
  etag: string;
}

export interface BlobBulkResult {
  path: string;
  bytes: number;
  item_count: number;
}

export interface BlobPreviewMetadata {
  label: string;
  value: string;
}

export interface BlobPreviewResult {
  kind: "table" | "json" | "text" | "image" | "parquet" | "binary";
  title: string;
  path: string;
  byte_count: number;
  truncated: boolean;
  row_offset: number;
  row_limit: number;
  total_rows?: number | null;
  has_previous_page: boolean;
  has_next_page: boolean;
  columns: string[];
  rows: string[][];
  text?: string | null;
  image_data_url?: string | null;
  metadata: BlobPreviewMetadata[];
  warning?: string | null;
}

export interface DeviceCodeLoginStatus {
  status: "pending" | "complete" | "error";
  connection_id?: string | null;
  error?: string | null;
}

export interface DiscoveryLoginStatus {
  status: "pending" | "complete" | "error";
  sign_in_id?: string | null;
  error?: string | null;
}

async function callTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (typeof window === "undefined") {
    throw new Error("Tauri IPC is unavailable on the server");
  }
  // @ts-expect-error - optional runtime-only global injected by Tauri
  if (!window.__TAURI_INTERNALS__) {
    throw new Error("Tauri IPC is unavailable in browser-only mode");
  }
  const mod = await import("@tauri-apps/api/core");
  return mod.invoke<T>(cmd, args);
}

export async function listConnections(): Promise<BrowserConnection[]> {
  return callTauri<BrowserConnection[]>("list_connections");
}

export async function listSignIns(): Promise<BrowserSignIn[]> {
  return callTauri<BrowserSignIn[]>("list_sign_ins");
}

export async function removeSignIn(signInId: string): Promise<void> {
  return callTauri<void>("remove_sign_in", {
    signInId,
  });
}

export async function listSignInTenants(signInId: string): Promise<BrowserTenant[]> {
  return callTauri<BrowserTenant[]>("list_sign_in_tenants", {
    signInId,
  });
}

export async function updateSignInFilter(
  signInId: string,
  tenantIds: string[],
  subscriptionIds: string[],
): Promise<BrowserSignIn> {
  return callTauri<BrowserSignIn>("update_sign_in_filter", {
    signInId,
    tenantIds,
    subscriptionIds,
  });
}

export async function listSubscriptions(signInId: string): Promise<BrowserSubscription[]> {
  return callTauri<BrowserSubscription[]>("list_subscriptions", {
    signInId,
  });
}

export async function listDiscoveredStorageAccounts(
  signInId: string,
  subscriptionId: string,
): Promise<BrowserStorageAccount[]> {
  return callTauri<BrowserStorageAccount[]>("list_discovered_storage_accounts", {
    signInId,
    subscriptionId,
  });
}

export async function connectWithConnectionString(displayName: string, connectionString: string): Promise<BrowserConnection> {
  return callTauri<BrowserConnection>("connect_connection_string", {
    displayName,
    connectionString,
  });
}

export async function connectWithAccountKey(
  displayName: string,
  accountName: string,
  accountKey: string,
  endpoint?: string,
): Promise<BrowserConnection> {
  return callTauri<BrowserConnection>("connect_account_key", {
    displayName,
    accountName,
    accountKey,
    endpoint,
  });
}

export async function connectWithSas(
  displayName: string,
  endpoint: string,
  sas: string,
  fixedContainer?: string,
): Promise<BrowserConnection> {
  return callTauri<BrowserConnection>("connect_sas", {
    displayName,
    endpoint,
    sas,
    fixedContainer,
  });
}

export async function connectAzurite(): Promise<BrowserConnection> {
  return callTauri<BrowserConnection>("connect_azurite");
}

export async function startEntraDeviceLogin(
  displayName: string,
  accountName: string,
  tenant?: string,
): Promise<DeviceCodePrompt> {
  return callTauri<DeviceCodePrompt>("start_entra_device_login", {
    displayName,
    accountName,
    tenant,
  });
}

export async function pollEntraDeviceLogin(loginId: string): Promise<DeviceCodeLoginStatus> {
  return callTauri<DeviceCodeLoginStatus>("poll_entra_device_login", {
    loginId,
  });
}

export async function startEntraBrowserLogin(
  displayName: string,
  tenant?: string,
): Promise<BrowserLoginPrompt> {
  return callTauri<BrowserLoginPrompt>("start_entra_browser_login", {
    displayName,
    tenant,
  });
}

export async function pollEntraBrowserLogin(loginId: string): Promise<DiscoveryLoginStatus> {
  return callTauri<DiscoveryLoginStatus>("poll_entra_browser_login", {
    loginId,
  });
}

export async function startSignInTenantReauth(
  signInId: string,
  tenantId: string,
): Promise<BrowserLoginPrompt> {
  return callTauri<BrowserLoginPrompt>("start_sign_in_tenant_reauth", {
    signInId,
    tenantId,
  });
}

export async function pollSignInTenantReauth(loginId: string): Promise<DiscoveryLoginStatus> {
  return callTauri<DiscoveryLoginStatus>("poll_sign_in_tenant_reauth", {
    loginId,
  });
}

export async function startEntraDiscoveryLogin(
  displayName: string,
  tenant?: string,
): Promise<DeviceCodePrompt> {
  return callTauri<DeviceCodePrompt>("start_entra_discovery_login", {
    displayName,
    tenant,
  });
}

export async function pollEntraDiscoveryLogin(loginId: string): Promise<DiscoveryLoginStatus> {
  return callTauri<DiscoveryLoginStatus>("poll_entra_discovery_login", {
    loginId,
  });
}

export async function connectDiscoveredStorageAccount(
  signInId: string,
  subscriptionId: string,
  accountName: string,
): Promise<BrowserConnection> {
  return callTauri<BrowserConnection>("connect_discovered_storage_account", {
    signInId,
    subscriptionId,
    accountName,
  });
}

export async function listContainers(connectionId: string): Promise<BrowserContainer[]> {
  return callTauri<BrowserContainer[]>("list_containers", {
    connectionId,
  });
}

export async function fetchBlobs(
  connectionId: string,
  container: string,
  prefix?: string | null,
): Promise<BlobRow[]> {
  return callTauri<BlobRow[]>("list_blobs", {
    connectionId,
    container,
    prefix,
  });
}

export async function uploadBlob(
  connectionId: string,
  container: string,
  sourcePath: string,
  destinationPrefix?: string | null,
  overwrite = false,
): Promise<BlobUploadResult> {
  return callTauri<BlobUploadResult>("upload_blob", {
    connectionId,
    container,
    sourcePath,
    destinationPrefix,
    overwrite,
  });
}

export async function downloadBlob(
  connectionId: string,
  container: string,
  path: string,
  openAfterDownload: boolean,
): Promise<BlobDownloadResult> {
  return callTauri<BlobDownloadResult>("download_blob", {
    connectionId,
    container,
    path,
    openAfterDownload,
  });
}

export async function previewBlob(
  connectionId: string,
  container: string,
  path: string,
  rowOffset = 0,
  rowLimit = 100,
): Promise<BlobPreviewResult> {
  return callTauri<BlobPreviewResult>("preview_blob", {
    connectionId,
    container,
    path,
    rowOffset,
    rowLimit,
  });
}

export async function downloadBlobPrefix(
  connectionId: string,
  container: string,
  prefix: string,
): Promise<BlobBulkResult> {
  return callTauri<BlobBulkResult>("download_blob_prefix", {
    connectionId,
    container,
    prefix,
  });
}

export async function deleteBlob(
  connectionId: string,
  container: string,
  path: string,
  includeSnapshots = false,
): Promise<void> {
  return callTauri<void>("delete_blob", {
    connectionId,
    container,
    path,
    includeSnapshots,
  });
}

export async function deleteBlobPrefix(
  connectionId: string,
  container: string,
  prefix: string,
  includeSnapshots = false,
): Promise<BlobBulkResult> {
  return callTauri<BlobBulkResult>("delete_blob_prefix", {
    connectionId,
    container,
    prefix,
    includeSnapshots,
  });
}

export async function createBlobFolder(
  connectionId: string,
  container: string,
  parentPrefix: string | null,
  folderName: string,
): Promise<BlobUploadResult> {
  return callTauri<BlobUploadResult>("create_blob_folder", {
    connectionId,
    container,
    parentPrefix,
    folderName,
  });
}

export async function renameBlobItem(
  connectionId: string,
  container: string,
  sourcePath: string,
  destinationPath: string,
  isPrefix: boolean,
): Promise<BlobBulkResult> {
  return callTauri<BlobBulkResult>("rename_blob_item", {
    connectionId,
    container,
    sourcePath,
    destinationPath,
    isPrefix,
  });
}

export async function copyBlobItem(
  connectionId: string,
  container: string,
  sourcePath: string,
  destinationPrefix: string | null,
  isPrefix: boolean,
): Promise<BlobBulkResult> {
  return callTauri<BlobBulkResult>("copy_blob_item", {
    connectionId,
    container,
    sourcePath,
    destinationPrefix,
    isPrefix,
  });
}

export async function disconnectConnection(connectionId: string): Promise<void> {
  return callTauri<void>("disconnect_connection", { connectionId });
}

export async function fetchActivities(): Promise<Activity[]> {
  return callTauri<Activity[]>("list_activities");
}
