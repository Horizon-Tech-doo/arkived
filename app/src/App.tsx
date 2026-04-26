import React, { CSSProperties, FormEvent, ReactNode, useEffect, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { GroupHeader, TitleBar, TreeRow } from "./chrome";
import { ActionBar, BlobTable, Inspector, TabsBar } from "./content";
import { ActivityBar } from "./panels";
import type { Activity, BlobRow } from "./data";
import {
  IconAlert,
  IconArrowUp,
  IconAzure,
  IconContainer,
  IconCopy,
  IconDownload,
  IconEye,
  IconExternal,
  IconFolderOpen,
  IconInfo,
  IconKey,
  IconLoader,
  IconLock,
  IconPlug,
  IconPlus,
  IconRefresh,
  IconSettings,
  IconSparkle,
  IconTerminal,
  IconUser,
  IconX,
} from "./icons";
import {
  BrowserConnection,
  BrowserContainer,
  BrowserLoginPrompt,
  BrowserSignIn,
  BrowserStorageAccount,
  BrowserSubscription,
  BrowserTenant,
  BlobPreviewResult,
  cancelActivity,
  clearActivities,
  DeviceCodePrompt,
  connectAzurite,
  connectDiscoveredStorageAccount,
  connectWithAccountKey,
  connectWithConnectionString,
  connectWithSas,
  copyBlobItem,
  createBlobFolder,
  deleteBlob,
  deleteBlobPrefix,
  disconnectConnection,
  downloadBlob,
  downloadBlobPrefix,
  fetchBlobs,
  fetchActivities,
  listConnections,
  listContainers,
  listDiscoveredStorageAccounts,
  listSignIns,
  listSignInTenants,
  pollEntraBrowserLogin,
  pollEntraDiscoveryLogin,
  pollSignInTenantReauth,
  previewBlob,
  removeSignIn,
  renameBlobItem,
  startEntraBrowserLogin,
  startEntraDiscoveryLogin,
  startSignInTenantReauth,
  updateSignInFilter,
  uploadBlob,
  uploadFolder,
} from "./lib/ipc";

type ConnectMethod =
  | "entra-browser"
  | "entra-device-code"
  | "connection-string"
  | "account-key"
  | "sas"
  | "azurite";
type TenantMode = "all" | "organizations" | "specific";

interface ConnectionFormState {
  displayName: string;
  connectionString: string;
  accountName: string;
  accountKey: string;
  endpoint: string;
  sas: string;
  fixedContainer: string;
  tenant: string;
  tenantMode: TenantMode;
}

interface TenantReauthFlowState {
  signInId: string;
  activeTenantId: string;
  queuedTenantIds: string[];
}

interface ContainerListState {
  items: BrowserContainer[];
  busy: boolean;
  error: string | null;
  loaded: boolean;
}

interface BrowserTabState {
  id: string;
  connectionId: string;
  containerName: string;
  prefix: string;
  filter: string;
  rows: BlobRow[];
  busy: boolean;
  error: string | null;
  loaded: boolean;
  continuation: string | null;
  selectedIndices: number[];
}

interface ContextMenuAction {
  kind?: "action";
  label: string;
  action: () => void | Promise<void>;
  danger?: boolean;
  disabled?: boolean;
  hint?: string;
}

interface ContextMenuSeparator {
  kind: "separator";
}

type ContextMenuItem = ContextMenuAction | ContextMenuSeparator;

interface ContextMenuState {
  x: number;
  y: number;
  items: ContextMenuItem[];
}

interface BlobClipboardState {
  connectionId: string;
  containerName: string;
  path: string;
  name: string;
  kind: string;
}

interface PreviewDialogState {
  row: BlobRow;
  result: BlobPreviewResult | null;
  rowOffset: number;
  rowLimit: number;
  busy: boolean;
  error: string | null;
}

interface PersistedBrowserTab {
  id?: string;
  connectionId?: string | null;
  originSignInId?: string | null;
  originSubscriptionId?: string | null;
  accountName?: string | null;
  endpoint?: string | null;
  containerName: string;
  prefix?: string;
  filter?: string;
}

interface PersistedShellSnapshot {
  version: 1;
  activeTabId?: string | null;
  activeConnectionId?: string | null;
  previewPaneRatio?: number;
  sidebarWidth?: number;
  detailPaneWidth?: number;
  sidebarDetailsHeight?: number;
  activityPaneHeight?: number;
  expandedSignIns: Record<string, boolean>;
  expandedSubscriptions: Record<string, boolean>;
  expandedAccounts: Record<string, boolean>;
  tabs: PersistedBrowserTab[];
}

const SHELL_STATE_STORAGE_KEY = "arkived.shell.v1";
const SIDEBAR_DEFAULT_WIDTH = 340;
const SIDEBAR_MIN_WIDTH = 260;
const SIDEBAR_MAX_WIDTH = 640;
const DETAIL_PANE_DEFAULT_WIDTH = 340;
const DETAIL_PANE_MIN_WIDTH = 260;
const DETAIL_PANE_MAX_WIDTH = 820;
const SIDEBAR_DETAILS_DEFAULT_HEIGHT = 200;
const SIDEBAR_DETAILS_MIN_HEIGHT = 118;
const SIDEBAR_DETAILS_MAX_HEIGHT = 420;
const ACTIVITY_PANE_DEFAULT_HEIGHT = 220;
const ACTIVITY_PANE_MIN_HEIGHT = 116;
const ACTIVITY_PANE_MAX_HEIGHT = 520;
const PANE_RESIZE_HANDLE_WIDTH = 7;
const PREVIEW_DEFAULT_ROW_LIMIT = 50;
const PREVIEW_PAGE_SIZE_OPTIONS = [25, 50, 100, 250, 500];
const PREVIEW_COLUMN_MIN_WIDTH = 72;
const PREVIEW_COLUMN_MAX_WIDTH = 420;

const EMPTY_FORM: ConnectionFormState = {
  displayName: "",
  connectionString: "",
  accountName: "",
  accountKey: "",
  endpoint: "",
  sas: "",
  fixedContainer: "",
  tenant: "",
  tenantMode: "organizations",
};

const CONNECT_METHODS: Array<{
  id: ConnectMethod;
  label: string;
  description: string;
}> = [
  {
    id: "entra-browser",
    label: "Sign in with Azure",
    description:
      "Open browser OAuth, pick an Azure account, then discover subscriptions and storage accounts through Azure Resource Manager.",
  },
  {
    id: "entra-device-code",
    label: "Device code",
    description:
      "Fallback sign-in for restricted desktops. Open the verification page, enter a code, then load the ARM discovery tree.",
  },
  {
    id: "connection-string",
    label: "Connection string",
    description: "Attach directly with a storage account connection string.",
  },
  {
    id: "account-key",
    label: "Account key",
    description: "Authenticate with a storage account name and shared key.",
  },
  {
    id: "sas",
    label: "SAS token",
    description: "Attach with a signed blob endpoint and optional fixed container scope.",
  },
  {
    id: "azurite",
    label: "Azurite",
    description: "Connect to the local Azurite emulator on its default blob endpoint.",
  },
];

function App() {
  const [connections, setConnections] = useState<BrowserConnection[]>([]);
  const [signIns, setSignIns] = useState<BrowserSignIn[]>([]);
  const [tenantsBySignIn, setTenantsBySignIn] = useState<Record<string, BrowserTenant[]>>({});
  const [subscriptionsBySignIn, setSubscriptionsBySignIn] = useState<Record<string, BrowserSubscription[]>>({});
  const [accountsBySubscription, setAccountsBySubscription] = useState<Record<string, BrowserStorageAccount[]>>({});
  const [containerStatesByConnection, setContainerStatesByConnection] = useState<Record<string, ContainerListState>>({});
  const [browserTabs, setBrowserTabs] = useState<BrowserTabState[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [activeConnectionId, setActiveConnectionId] = useState<string | null>(null);
  const [expandedSignIns, setExpandedSignIns] = useState<Record<string, boolean>>({});
  const [expandedSubscriptions, setExpandedSubscriptions] = useState<Record<string, boolean>>({});
  const [expandedAccounts, setExpandedAccounts] = useState<Record<string, boolean>>({});
  const [activatingAccounts, setActivatingAccounts] = useState<Record<string, boolean>>({});
  const [connectOpen, setConnectOpen] = useState(false);
  const [manageSignInId, setManageSignInId] = useState<string | null>(null);
  const [connectMethod, setConnectMethod] = useState<ConnectMethod>("entra-browser");
  const [form, setForm] = useState<ConnectionFormState>(EMPTY_FORM);
  const [browserPrompt, setBrowserPrompt] = useState<BrowserLoginPrompt | null>(null);
  const [devicePrompt, setDevicePrompt] = useState<DeviceCodePrompt | null>(null);
  const [tenantBrowserPrompt, setTenantBrowserPrompt] = useState<BrowserLoginPrompt | null>(null);
  const [tenantReauthFlow, setTenantReauthFlow] = useState<TenantReauthFlowState | null>(null);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [shellError, setShellError] = useState<string | null>(null);
  const [discoveryError, setDiscoveryError] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [blobClipboard, setBlobClipboard] = useState<BlobClipboardState | null>(null);
  const [previewDialog, setPreviewDialog] = useState<PreviewDialogState | null>(null);
  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT_WIDTH);
  const [detailPaneWidth, setDetailPaneWidth] = useState(DETAIL_PANE_DEFAULT_WIDTH);
  const [sidebarDetailsHeight, setSidebarDetailsHeight] = useState(SIDEBAR_DETAILS_DEFAULT_HEIGHT);
  const [activityPaneHeight, setActivityPaneHeight] = useState(ACTIVITY_PANE_DEFAULT_HEIGHT);
  const [activities, setActivities] = useState<Activity[]>([]);
  const [activityExpanded, setActivityExpanded] = useState(false);
  const [connectionsBusy, setConnectionsBusy] = useState(false);
  const [signInsBusy, setSignInsBusy] = useState(false);
  const [connectBusy, setConnectBusy] = useState(false);
  const [manageBusy, setManageBusy] = useState(false);
  const [tenantReauthBusy, setTenantReauthBusy] = useState(false);
  const [disconnectBusy, setDisconnectBusy] = useState(false);
  const [copiedCode, setCopiedCode] = useState(false);
  const [shellInitialized, setShellInitialized] = useState(false);
  const [shellPersistenceReady, setShellPersistenceReady] = useState(false);
  const [sidebarPanelTab, setSidebarPanelTab] = useState<"actions" | "properties">("actions");

  const containerRequestIds = useRef<Record<string, number>>({});
  const blobRequestIds = useRef<Record<string, number>>({});
  const openedDevicePromptId = useRef<string | null>(null);
  const tauriAvailable = useRef(isTauriRuntimeAvailable());
  const shellHydrated = useRef(false);
  const browserTabsRef = useRef<BrowserTabState[]>([]);
  const connectionsRef = useRef<BrowserConnection[]>([]);
  const containerStatesRef = useRef<Record<string, ContainerListState>>({});
  const browserPaneRef = useRef<HTMLDivElement | null>(null);
  const previewRequestId = useRef(0);
  const blobSelectionAnchors = useRef<Record<string, number>>({});

  browserTabsRef.current = browserTabs;
  connectionsRef.current = connections;
  containerStatesRef.current = containerStatesByConnection;

  const activeTab = activeTabId ? browserTabs.find((tab) => tab.id === activeTabId) ?? null : null;
  const browsingConnectionId = activeTab?.connectionId ?? activeConnectionId;
  const activeConnection = connections.find((connection) => connection.id === browsingConnectionId) ?? null;
  const activeContainer = activeTab?.containerName ?? null;
  const activeRows = activeTab?.rows ?? [];
  const activeRowsHaveMore = Boolean(activeTab?.loaded && activeTab?.continuation);
  const selectedIndices = activeTab?.selectedIndices ?? [];
  const selectedRows = new Set(selectedIndices);
  const selectedIndex = selectedIndices.length > 0 ? [...selectedIndices].sort((a, b) => a - b)[0] : null;
  const selectedRow = selectedIndex == null ? null : activeRows[selectedIndex] ?? null;
  const selectedResourceRows = selectedIndices
    .map((index) => activeRows[index])
    .filter((row): row is BlobRow => Boolean(row));
  const selectedBlobRows = selectedResourceRows.filter((row) => row.kind !== "dir");
  const canPreviewSelection = selectedBlobRows.length === 1 && selectedResourceRows.length === 1;
  const prefix = activeTab?.prefix ?? "";
  const breadcrumbSegments = splitPrefix(prefix);
  const resourceUrl =
    activeConnection && activeContainer && selectedRow?.path
      ? buildResourceUrl(activeConnection.endpoint, activeContainer, selectedRow.path)
      : null;
  const directConnections = connections.filter((connection) => !isDiscoveredConnection(connection));
  const anyContainersBusy = Object.values(containerStatesByConnection).some((state) => state.busy);
  const anyRowsBusy = browserTabs.some((tab) => tab.busy);

  function tabIdFor(connectionId: string, containerName: string) {
    return `${connectionId}::${containerName}`;
  }

  function discoveredRowKey(account: BrowserStorageAccount) {
    return `${account.sign_in_id}::${account.subscription_id}::${account.name}`.toLowerCase();
  }

  function directRowKey(connectionId: string) {
    return `direct::${connectionId}`;
  }

  function findDiscoveredConnection(account: BrowserStorageAccount): BrowserConnection | null {
    return (
      connectionsRef.current.find(
        (connection) =>
          connection.origin_sign_in_id === account.sign_in_id &&
          connection.origin_subscription_id === account.subscription_id &&
          connection.account_name === account.name &&
          normalizeUrlHost(connection.endpoint) === normalizeUrlHost(account.endpoint),
      ) ?? null
    );
  }

  function isDiscoveredAccountActive(account: BrowserStorageAccount): boolean {
    return (
      activeConnection != null &&
      activeConnection.origin_sign_in_id === account.sign_in_id &&
      activeConnection.origin_subscription_id === account.subscription_id &&
      activeConnection.account_name === account.name &&
      normalizeUrlHost(activeConnection.endpoint) === normalizeUrlHost(account.endpoint)
    );
  }

  function resetAzureSignInFormDefaults() {
    setForm((current) => ({
      ...current,
      tenantMode: "organizations",
      tenant: "",
    }));
  }

  async function refreshConnections(preferredConnectionId?: string | null): Promise<BrowserConnection[]> {
    if (!tauriAvailable.current) {
      setShellError("Live Azure browsing requires the Tauri desktop shell. Start this app with `npm run tauri:dev`.");
      return [];
    }

    setConnectionsBusy(true);
    setShellError(null);

    try {
      const nextConnections = await listConnections();
      const nextConnectionIds = new Set(nextConnections.map((connection) => connection.id));
      setConnections(nextConnections);
      setContainerStatesByConnection((current) =>
        Object.fromEntries(
          Object.entries(current).filter(([connectionId]) => nextConnectionIds.has(connectionId)),
        ),
      );
      setBrowserTabs((current) =>
        current.filter((tab) => nextConnectionIds.has(tab.connectionId)),
      );
      setActiveConnectionId((current) => {
        if (preferredConnectionId && nextConnections.some((connection) => connection.id === preferredConnectionId)) {
          return preferredConnectionId;
        }
        if (current && nextConnections.some((connection) => connection.id === current)) {
          return current;
        }
        return nextConnections[0]?.id ?? null;
      });

      return nextConnections;
    } catch (error) {
      setShellError(getErrorMessage(error));
      setConnections([]);
      setContainerStatesByConnection({});
      setBrowserTabs([]);
      setActiveTabId(null);
      setActiveConnectionId(null);
      return [];
    } finally {
      setConnectionsBusy(false);
    }
  }

  async function refreshDiscoveryTree(preferredSignInId?: string | null): Promise<BrowserSignIn[]> {
    if (!tauriAvailable.current) {
      return [];
    }

    setSignInsBusy(true);
    setDiscoveryError(null);

    try {
      const nextSignIns = await listSignIns();
      const nextTenantsBySignIn: Record<string, BrowserTenant[]> = {};
      const nextSubscriptionsBySignIn: Record<string, BrowserSubscription[]> = {};
      const nextAccountsBySubscription: Record<string, BrowserStorageAccount[]> = {};

      await Promise.all(
        nextSignIns.map(async (signIn) => {
          const tenants = await listSignInTenants(signIn.id);
          nextTenantsBySignIn[signIn.id] = tenants;
          const subscriptions = flattenSelectedSubscriptions(tenants);
          nextSubscriptionsBySignIn[signIn.id] = subscriptions;

          await Promise.all(
            subscriptions.map(async (subscription) => {
              nextAccountsBySubscription[subscription.id] = await listDiscoveredStorageAccounts(
                signIn.id,
                subscription.id,
              );
            }),
          );
        }),
      );

      setSignIns(nextSignIns);
      setTenantsBySignIn(nextTenantsBySignIn);
      setSubscriptionsBySignIn(nextSubscriptionsBySignIn);
      setAccountsBySubscription(nextAccountsBySubscription);

      setExpandedSignIns((current) => {
        const next = { ...current };
        if (preferredSignInId) {
          next[preferredSignInId] = true;
        } else if (nextSignIns.length === 1 && next[nextSignIns[0].id] == null) {
          next[nextSignIns[0].id] = true;
        }
        return next;
      });

      if (preferredSignInId) {
        const firstSubscription = nextSubscriptionsBySignIn[preferredSignInId]?.[0];
        if (firstSubscription) {
          setExpandedSubscriptions((current) => ({
            ...current,
            [firstSubscription.id]: true,
          }));
        }
      }

      return nextSignIns;
    } catch (error) {
      setDiscoveryError(getErrorMessage(error));
      setSignIns([]);
      setTenantsBySignIn({});
      setSubscriptionsBySignIn({});
      setAccountsBySubscription({});
      return [];
    } finally {
      setSignInsBusy(false);
    }
  }

  async function refreshActivities() {
    if (!tauriAvailable.current) {
      return;
    }

    try {
      setActivities(await fetchActivities());
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleCancelActivity(activityId: string) {
    if (!tauriAvailable.current) {
      return;
    }

    try {
      await cancelActivity(activityId);
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleClearActivities(scope: "completed" | "successful") {
    if (!tauriAvailable.current) {
      return;
    }

    try {
      setActivities(await clearActivities(scope));
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function ensureContainersLoaded(connectionId: string, force = false): Promise<BrowserContainer[]> {
    const currentState = containerStatesRef.current[connectionId];
    if (!force && currentState?.loaded && !currentState.busy) {
      return currentState.items;
    }
    if (!force && currentState?.busy) {
      return currentState.items;
    }

    const requestId = (containerRequestIds.current[connectionId] ?? 0) + 1;
    containerRequestIds.current[connectionId] = requestId;
    setContainerStatesByConnection((current) => ({
      ...current,
      [connectionId]: {
        items: current[connectionId]?.items ?? [],
        busy: true,
        error: null,
        loaded: current[connectionId]?.loaded ?? false,
      },
    }));

    try {
      const nextContainers = await listContainers(connectionId);
      if (containerRequestIds.current[connectionId] !== requestId) {
        return currentState?.items ?? [];
      }

      setContainerStatesByConnection((current) => ({
        ...current,
        [connectionId]: {
          items: nextContainers,
          busy: false,
          error: null,
          loaded: true,
        },
      }));
      return nextContainers;
    } catch (error) {
      if (containerRequestIds.current[connectionId] !== requestId) {
        return currentState?.items ?? [];
      }

      setContainerStatesByConnection((current) => ({
        ...current,
        [connectionId]: {
          items: current[connectionId]?.items ?? [],
          busy: false,
          error: getErrorMessage(error),
          loaded: true,
        },
      }));
      return [];
    }
  }

  function updateTab(tabId: string, updater: (tab: BrowserTabState) => BrowserTabState) {
    setBrowserTabs((current) =>
      current.map((tab) => (tab.id === tabId ? updater(tab) : tab)),
    );
  }

  async function loadTabRows(tabId: string, force = false) {
    const tab = browserTabsRef.current.find((currentTab) => currentTab.id === tabId);
    if (!tab) {
      return;
    }
    if (!force && (tab.loaded || tab.busy)) {
      return;
    }

    const requestId = (blobRequestIds.current[tabId] ?? 0) + 1;
    blobRequestIds.current[tabId] = requestId;
    const requestedPrefix = tab.prefix;
    const requestedFilter = tab.filter;
    updateTab(tabId, (currentTab) => ({
      ...currentTab,
      busy: true,
      error: null,
    }));

    try {
      const page = await fetchBlobs(
        tab.connectionId,
        tab.containerName,
        tab.prefix || null,
        tab.filter || null,
        null,
      );
      if (blobRequestIds.current[tabId] !== requestId) {
        return;
      }

      updateTab(tabId, (currentTab) => ({
        ...(currentTab.prefix === requestedPrefix && currentTab.filter === requestedFilter
          ? {
              ...currentTab,
              rows: page.rows,
              busy: false,
              error: null,
              loaded: true,
              continuation: page.continuation ?? null,
              selectedIndices: [],
            }
          : {
              ...currentTab,
              busy: false,
              loaded: false,
              continuation: null,
            }),
      }));
    } catch (error) {
      if (blobRequestIds.current[tabId] !== requestId) {
        return;
      }

      updateTab(tabId, (currentTab) => ({
        ...(currentTab.prefix === requestedPrefix && currentTab.filter === requestedFilter
          ? {
              ...currentTab,
              rows: [],
              busy: false,
              error: getErrorMessage(error),
              loaded: true,
              continuation: null,
              selectedIndices: [],
            }
          : {
              ...currentTab,
              busy: false,
              loaded: false,
              continuation: null,
            }),
      }));
    }
  }

  async function loadMoreTabRows(tabId: string) {
    const tab = browserTabsRef.current.find((currentTab) => currentTab.id === tabId);
    if (!tab || tab.busy || !tab.continuation) {
      return;
    }

    const requestId = (blobRequestIds.current[tabId] ?? 0) + 1;
    blobRequestIds.current[tabId] = requestId;
    const requestedPrefix = tab.prefix;
    const requestedFilter = tab.filter;
    const requestedContinuation = tab.continuation;
    updateTab(tabId, (currentTab) => ({
      ...currentTab,
      busy: true,
      error: null,
    }));

    try {
      const page = await fetchBlobs(
        tab.connectionId,
        tab.containerName,
        tab.prefix || null,
        tab.filter || null,
        tab.continuation,
      );
      if (blobRequestIds.current[tabId] !== requestId) {
        return;
      }

      updateTab(tabId, (currentTab) => ({
        ...(currentTab.prefix === requestedPrefix &&
        currentTab.filter === requestedFilter &&
        currentTab.continuation === requestedContinuation
          ? {
              ...currentTab,
              rows: [...currentTab.rows, ...page.rows],
              busy: false,
              error: null,
              loaded: true,
              continuation: page.continuation ?? null,
            }
          : {
              ...currentTab,
              busy: false,
              loaded: false,
              continuation: null,
            }),
      }));
    } catch (error) {
      if (blobRequestIds.current[tabId] !== requestId) {
        return;
      }

      updateTab(tabId, (currentTab) => ({
        ...(currentTab.prefix === requestedPrefix &&
        currentTab.filter === requestedFilter &&
        currentTab.continuation === requestedContinuation
          ? {
              ...currentTab,
              busy: false,
              error: getErrorMessage(error),
              loaded: true,
            }
          : {
              ...currentTab,
              busy: false,
              loaded: false,
              continuation: null,
            }),
      }));
    }
  }

  async function initializeShell() {
    if (!tauriAvailable.current) {
      setShellError("Live Azure browsing requires the Tauri desktop shell. Start this app with `npm run tauri:dev`.");
      setShellInitialized(true);
      setShellPersistenceReady(true);
      return;
    }

    try {
      const [nextConnections, nextSignIns] = await Promise.all([
        refreshConnections(),
        refreshDiscoveryTree(),
        refreshActivities(),
      ]);
      if (nextConnections.length === 0 && nextSignIns.length === 0) {
        setConnectOpen(true);
      }
    } finally {
      setShellInitialized(true);
    }
  }

  async function handleRefresh() {
    if (!tauriAvailable.current) {
      setShellError("Live Azure browsing requires the Tauri desktop shell. Start this app with `npm run tauri:dev`.");
      return;
    }

    const preferredConnectionId = activeTab?.connectionId ?? activeConnectionId;
    await Promise.all([
      refreshConnections(preferredConnectionId),
      refreshDiscoveryTree(),
      refreshActivities(),
    ]);

    await Promise.all(
      Object.keys(containerStatesRef.current).map((connectionId) =>
        ensureContainersLoaded(connectionId, true),
      ),
    );
    if (activeTabId) {
      updateTab(activeTabId, (tab) => ({ ...tab, loaded: false }));
    }
  }

  async function handleDisconnect(connectionId = activeTab?.connectionId ?? activeConnectionId) {
    if (!connectionId) {
      return;
    }

    setDisconnectBusy(true);
    setShellError(null);

    try {
      await disconnectConnection(connectionId);
      setBrowserTabs((current) => current.filter((tab) => tab.connectionId !== connectionId));
      setContainerStatesByConnection((current) =>
        Object.fromEntries(
          Object.entries(current).filter(([currentConnectionId]) => currentConnectionId !== connectionId),
        ),
      );
      await refreshConnections();
    } catch (error) {
      setShellError(getErrorMessage(error));
    } finally {
      setDisconnectBusy(false);
    }
  }

  async function handleRemoveSignIn(signIn: BrowserSignIn) {
    const confirmed = window.confirm(`Remove Azure account "${signIn.display_name}" from Arkived?`);
    if (!confirmed) {
      return;
    }

    setSignInsBusy(true);
    setShellError(null);
    setDiscoveryError(null);

    try {
      await removeSignIn(signIn.id);
      setManageSignInId((current) => (current === signIn.id ? null : current));
      setExpandedSignIns((current) => {
        const next = { ...current };
        delete next[signIn.id];
        return next;
      });
      setBrowserTabs((current) =>
        current.filter((tab) => {
          const connection = connectionsRef.current.find((candidate) => candidate.id === tab.connectionId);
          return connection?.origin_sign_in_id !== signIn.id;
        }),
      );
      await Promise.all([refreshConnections(), refreshDiscoveryTree()]);
    } catch (error) {
      setDiscoveryError(getErrorMessage(error));
    } finally {
      setSignInsBusy(false);
    }
  }

  async function handleConnectSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!tauriAvailable.current) {
      setConnectError("Live Azure browsing requires the Tauri desktop shell. Start this app with `npm run tauri:dev`.");
      return;
    }

    setConnectBusy(true);
    setConnectError(null);
    setShellError(null);

    try {
      const azureTenant = resolveAzureTenant(form);
      if ((connectMethod === "entra-browser" || connectMethod === "entra-device-code") && form.tenantMode === "specific" && !azureTenant) {
        setConnectError("Enter a tenant ID or verified domain to use a specific tenant filter.");
        return;
      }

      let connection: BrowserConnection | null = null;

      switch (connectMethod) {
        case "connection-string":
          connection = await connectWithConnectionString(form.displayName, form.connectionString);
          break;
        case "account-key":
          connection = await connectWithAccountKey(
            form.displayName,
            form.accountName,
            form.accountKey,
            form.endpoint || undefined,
          );
          break;
        case "sas":
          connection = await connectWithSas(
            form.displayName,
            form.endpoint,
            form.sas,
            form.fixedContainer || undefined,
          );
          break;
        case "azurite":
          connection = await connectAzurite();
          break;
        case "entra-browser": {
          const prompt = await startEntraBrowserLogin(form.displayName, azureTenant);
          setBrowserPrompt(prompt);
          setDevicePrompt(null);
          setConnectMethod("entra-browser");
          setCopiedCode(false);
          break;
        }
        case "entra-device-code": {
          const prompt = await startEntraDiscoveryLogin(form.displayName, azureTenant);
          setDevicePrompt(prompt);
          setBrowserPrompt(null);
          setConnectMethod("entra-device-code");
          setCopiedCode(false);
          break;
        }
      }

      if (connection) {
        await refreshConnections(connection.id);
        setExpandedAccounts((current) => ({
          ...current,
          [directRowKey(connection.id)]: true,
        }));
        await ensureContainersLoaded(connection.id);
        setConnectOpen(false);
        setBrowserPrompt(null);
        setDevicePrompt(null);
      }
    } catch (error) {
      setConnectError(getErrorMessage(error));
      setBrowserPrompt(null);
      setDevicePrompt(null);
    } finally {
      setConnectBusy(false);
    }
  }

  function openContainerTab(connectionId: string, containerName: string) {
    const id = tabIdFor(connectionId, containerName);
    setBrowserTabs((current) => {
      if (current.some((tab) => tab.id === id)) {
        return current;
      }

      return [
        ...current,
        {
          id,
          connectionId,
          containerName,
          prefix: "",
          filter: "",
          rows: [],
          busy: false,
          error: null,
          loaded: false,
          continuation: null,
          selectedIndices: [],
        },
      ];
    });
    setActiveTabId(id);
    setActiveConnectionId(connectionId);
  }

  async function ensureDiscoveredAccountConnection(account: BrowserStorageAccount): Promise<BrowserConnection> {
    const existing = findDiscoveredConnection(account);

    if (existing?.auth_kind === "entra-managed-key") {
      setActiveConnectionId(existing.id);
      return existing;
    }

    setConnectBusy(true);
    setShellError(null);

    try {
      const connection = await connectDiscoveredStorageAccount(
        account.sign_in_id,
        account.subscription_id,
        account.name,
      );
      const nextConnections = await refreshConnections(connection.id);
      const resolved =
        nextConnections.find((candidate) => candidate.id === connection.id) ?? connection;
      setActiveConnectionId(resolved.id);
      return resolved;
    } finally {
      setConnectBusy(false);
    }
  }

  async function handleSelectDiscoveredAccount(account: BrowserStorageAccount) {
    try {
      const connection = await ensureDiscoveredAccountConnection(account);
      setActiveConnectionId(connection.id);
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleToggleDiscoveredAccount(account: BrowserStorageAccount) {
    const key = discoveredRowKey(account);
    const nextExpanded = !(expandedAccounts[key] ?? false);
    setExpandedAccounts((current) => ({ ...current, [key]: nextExpanded }));

    if (!nextExpanded) {
      return;
    }

    setActivatingAccounts((current) => ({ ...current, [key]: true }));
    try {
      const connection = await ensureDiscoveredAccountConnection(account);
      await ensureContainersLoaded(connection.id);
    } catch (error) {
      setShellError(getErrorMessage(error));
    } finally {
      setActivatingAccounts((current) => ({ ...current, [key]: false }));
    }
  }

  async function handleToggleDirectAccount(connection: BrowserConnection) {
    const key = directRowKey(connection.id);
    const nextExpanded = !(expandedAccounts[key] ?? false);
    setExpandedAccounts((current) => ({ ...current, [key]: nextExpanded }));

    if (!nextExpanded) {
      return;
    }

    setActiveConnectionId(connection.id);
    await ensureContainersLoaded(connection.id);
  }

  function openManageDialog(signInId: string) {
    setManageSignInId(signInId);
  }

  function tenantLabelFor(signInId: string, tenantId: string): string {
    return (
      tenantsBySignIn[signInId]?.find((tenant) => tenant.id === tenantId)?.display_name ??
      tenantId
    );
  }

  function resetTenantReauthFlow() {
    setTenantBrowserPrompt(null);
    setTenantReauthFlow(null);
    setTenantReauthBusy(false);
  }

  async function launchTenantReauth(signInId: string, tenantId: string, queuedTenantIds: string[] = []) {
    if (!tauriAvailable.current) {
      setDiscoveryError("Live Azure browsing requires the Tauri desktop shell. Start this app with `npm run tauri:dev`.");
      return;
    }

    setTenantReauthBusy(true);
    setDiscoveryError(null);

    try {
      const prompt = await startSignInTenantReauth(signInId, tenantId);
      setTenantReauthFlow({ signInId, activeTenantId: tenantId, queuedTenantIds });
      setTenantBrowserPrompt(prompt);
    } catch (error) {
      const label = tenantLabelFor(signInId, tenantId);
      const message = `Tenant authentication failed for ${label}: ${getErrorMessage(error)}`;
      if (queuedTenantIds.length > 0) {
        setDiscoveryError(message);
        await launchTenantReauth(signInId, queuedTenantIds[0], queuedTenantIds.slice(1));
        return;
      }
      resetTenantReauthFlow();
      setDiscoveryError(message);
    }
  }

  async function handleReauthenticateTenant(tenantId: string) {
    if (!manageSignInId || tenantReauthBusy || manageBusy) {
      return;
    }

    await launchTenantReauth(manageSignInId, tenantId);
  }

  async function handleReauthenticateBlockedTenants() {
    if (!manageSignInId || tenantReauthBusy || manageBusy) {
      return;
    }

    await reauthenticateBlockedTenantsFor(manageSignInId);
  }

  async function reauthenticateBlockedTenantsFor(signInId: string) {
    if (tenantReauthBusy || manageBusy) {
      return;
    }

    const blockedTenantIds = (tenantsBySignIn[signInId] ?? [])
      .filter((tenant) => tenant.needs_reauth)
      .map((tenant) => tenant.id);
    if (blockedTenantIds.length === 0) {
      return;
    }

    await launchTenantReauth(signInId, blockedTenantIds[0], blockedTenantIds.slice(1));
  }

  async function handleApplyTenantFilter(tenants: BrowserTenant[]) {
    if (!manageSignInId) {
      return;
    }

    setManageBusy(true);
    setDiscoveryError(null);

    try {
      const tenantIds = tenants.filter((tenant) => tenant.selected).map((tenant) => tenant.id);
      const subscriptionIds = tenants.flatMap((tenant) =>
        tenant.subscriptions.filter((subscription) => subscription.selected).map((subscription) => subscription.id),
      );
      await updateSignInFilter(manageSignInId, tenantIds, subscriptionIds);
      await refreshDiscoveryTree(manageSignInId);
      setManageSignInId(null);
    } catch (error) {
      setDiscoveryError(getErrorMessage(error));
    } finally {
      setManageBusy(false);
    }
  }

  function updateForm<K extends keyof ConnectionFormState>(field: K, value: ConnectionFormState[K]) {
    setForm((current) => ({ ...current, [field]: value }));
  }

  function openConnectDialog(method?: ConnectMethod) {
    setConnectError(null);
    setConnectOpen(true);
    if (browserPrompt) {
      setConnectMethod("entra-browser");
      return;
    }
    if (devicePrompt) {
      setConnectMethod("entra-device-code");
      return;
    }
    if (method) {
      if (method === "entra-browser" || method === "entra-device-code") {
        resetAzureSignInFormDefaults();
      }
      setConnectMethod(method);
    } else if (connectMethod === "entra-browser" || connectMethod === "entra-device-code") {
      resetAzureSignInFormDefaults();
    }
  }

  function closeConnectDialog() {
    if (connectBusy) {
      return;
    }
    setConnectError(null);
    setConnectOpen(false);
  }

  function handleSelectConnection(connectionId: string) {
    setActiveConnectionId(connectionId);
  }

  function handleSelectContainer(connectionId: string, containerName: string) {
    openContainerTab(connectionId, containerName);
  }

  async function hydratePersistedShell(snapshot: PersistedShellSnapshot) {
    setExpandedSignIns(snapshot.expandedSignIns);
    setExpandedSubscriptions(snapshot.expandedSubscriptions);
    setExpandedAccounts(snapshot.expandedAccounts);

    const restoredTabs: BrowserTabState[] = [];
    let restoredActiveTabId: string | null = null;
    let restoredActiveConnectionId =
      snapshot.activeConnectionId &&
      connectionsRef.current.some((connection) => connection.id === snapshot.activeConnectionId)
        ? snapshot.activeConnectionId
        : null;

    const discoveredAccounts = Object.values(accountsBySubscription).flat();

    for (const connection of connectionsRef.current) {
      if (snapshot.expandedAccounts[directRowKey(connection.id)]) {
        await ensureContainersLoaded(connection.id);
      }
    }

    for (const account of discoveredAccounts) {
      if (!snapshot.expandedAccounts[discoveredRowKey(account)]) {
        continue;
      }

      try {
        const connection = await ensureDiscoveredAccountConnection(account);
        await ensureContainersLoaded(connection.id);
        restoredActiveConnectionId = restoredActiveConnectionId ?? connection.id;
      } catch (error) {
        setShellError(getErrorMessage(error));
      }
    }

    for (const persistedTab of snapshot.tabs) {
      if (!persistedTab.containerName) {
        continue;
      }

      let connection =
        (persistedTab.connectionId
          ? connectionsRef.current.find((candidate) => candidate.id === persistedTab.connectionId)
          : null) ?? null;

      if (!connection && persistedTab.originSignInId && persistedTab.originSubscriptionId && persistedTab.accountName) {
        const account = discoveredAccounts.find(
          (candidate) =>
            candidate.sign_in_id === persistedTab.originSignInId &&
            candidate.subscription_id === persistedTab.originSubscriptionId &&
            candidate.name === persistedTab.accountName &&
            (!persistedTab.endpoint || normalizeUrlHost(candidate.endpoint) === normalizeUrlHost(persistedTab.endpoint)),
        );

        if (account) {
          try {
            connection = await ensureDiscoveredAccountConnection(account);
          } catch (error) {
            setShellError(getErrorMessage(error));
          }
        }
      }

      if (!connection) {
        continue;
      }

      await ensureContainersLoaded(connection.id);
      const id = tabIdFor(connection.id, persistedTab.containerName);
      if (restoredTabs.some((tab) => tab.id === id)) {
        continue;
      }

      restoredTabs.push({
        id,
        connectionId: connection.id,
        containerName: persistedTab.containerName,
        prefix: persistedTab.prefix ?? "",
        filter: persistedTab.filter ?? "",
        rows: [],
        busy: false,
        error: null,
        loaded: false,
        continuation: null,
        selectedIndices: [],
      });

      if (persistedTab.id === snapshot.activeTabId || !restoredActiveTabId) {
        restoredActiveTabId = id;
      }
      restoredActiveConnectionId = restoredActiveConnectionId ?? connection.id;
    }

    if (restoredTabs.length > 0) {
      setBrowserTabs(restoredTabs);
      setActiveTabId(restoredActiveTabId ?? restoredTabs[0].id);
    }

    if (restoredActiveConnectionId) {
      setActiveConnectionId(restoredActiveConnectionId);
    }
  }

  function toggleSignIn(signInId: string) {
    setExpandedSignIns((current) => ({ ...current, [signInId]: !current[signInId] }));
  }

  function toggleSubscription(subscriptionId: string) {
    setExpandedSubscriptions((current) => ({ ...current, [subscriptionId]: !current[subscriptionId] }));
  }

  function handleActivateRow(index: number) {
    if (!activeTab) {
      return;
    }

    const row = activeRows[index];
    if (!row || row.kind !== "dir" || !row.path) {
      return;
    }
    const nextPrefix = ensureTrailingSlash(row.path);

    updateTab(activeTab.id, (tab) => ({
      ...tab,
      prefix: nextPrefix,
      filter: "",
      loaded: false,
      error: null,
      continuation: null,
      selectedIndices: [],
    }));
  }

  function handleGoUp() {
    if (!activeTab || !prefix) {
      return;
    }

    updateTab(activeTab.id, (tab) => ({
      ...tab,
      prefix: parentPrefix(tab.prefix),
      filter: "",
      loaded: false,
      error: null,
      continuation: null,
      selectedIndices: [],
    }));
  }

  function handleToggleSelection(index: number) {
    if (!activeTab) {
      return;
    }

    blobSelectionAnchors.current[activeTab.id] = index;
    updateTab(activeTab.id, (tab) => {
      const next = new Set(tab.selectedIndices);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return { ...tab, selectedIndices: Array.from(next) };
    });
  }

  function handleSelectRow(index: number, event: React.MouseEvent<HTMLDivElement>) {
    if (!activeTab) {
      return;
    }

    const tabId = activeTab.id;
    updateTab(tabId, (tab) => {
      let selectedIndices: number[];

      if (event.shiftKey) {
        const anchor = blobSelectionAnchors.current[tabId] ?? tab.selectedIndices[tab.selectedIndices.length - 1] ?? index;
        const start = Math.min(anchor, index);
        const end = Math.max(anchor, index);
        selectedIndices = Array.from({ length: end - start + 1 }, (_, offset) => start + offset);
      } else if (event.ctrlKey || event.metaKey) {
        const next = new Set(tab.selectedIndices);
        if (next.has(index)) {
          next.delete(index);
        } else {
          next.add(index);
        }
        selectedIndices = Array.from(next).sort((a, b) => a - b);
        blobSelectionAnchors.current[tabId] = index;
      } else {
        selectedIndices = [index];
        blobSelectionAnchors.current[tabId] = index;
      }

      return { ...tab, selectedIndices };
    });
  }

  function handleToggleSelectAll() {
    if (!activeTab) {
      return;
    }

    updateTab(activeTab.id, (tab) => ({
      ...tab,
      selectedIndices:
        tab.selectedIndices.length === tab.rows.length
          ? []
          : tab.rows.map((_, index) => index),
    }));
  }

  async function handleCopyUserCode() {
    if (!devicePrompt || !navigator.clipboard) {
      return;
    }

    await navigator.clipboard.writeText(devicePrompt.user_code);
    setCopiedCode(true);
  }

  async function copyText(value: string) {
    if (!navigator.clipboard) {
      return;
    }
    await navigator.clipboard.writeText(value);
  }

  async function handleDownloadBlob(row: BlobRow, openAfterDownload = false) {
    if (!activeConnection || !activeContainer || !row.path || row.kind === "dir") {
      return;
    }

    setShellError(null);
    try {
      const result = await downloadBlob(
        activeConnection.id,
        activeContainer,
        row.path,
        openAfterDownload,
      );
      setShellError(
        openAfterDownload && result.opened
          ? `Opened ${row.name} from ${result.path}`
          : `Downloaded ${row.name} to ${result.path}`,
      );
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handlePreviewBlob(row: BlobRow, rowOffset = 0, rowLimit = PREVIEW_DEFAULT_ROW_LIMIT) {
    if (!activeConnection || !activeContainer || !row.path || row.kind === "dir") {
      return;
    }

    const requestId = ++previewRequestId.current;
    setShellError(null);
    setPreviewDialog((current) => ({
      row,
      result: current && current.row.path === row.path ? current.result : null,
      rowOffset,
      rowLimit,
      busy: true,
      error: null,
    }));

    try {
      const result = await previewBlob(activeConnection.id, activeContainer, row.path, rowOffset, rowLimit);
      if (requestId !== previewRequestId.current) {
        return;
      }
      setPreviewDialog({
        row,
        result,
        rowOffset: result.row_offset,
        rowLimit: result.row_limit || rowLimit,
        busy: false,
        error: null,
      });
    } catch (error) {
      if (requestId !== previewRequestId.current) {
        return;
      }
      const message = getErrorMessage(error);
      setShellError(message);
      setPreviewDialog({
        row,
        result: null,
        rowOffset,
        rowLimit,
        busy: false,
        error: message,
      });
    }
  }

  async function handlePreviewSelection() {
    if (!canPreviewSelection) {
      return;
    }

    await handlePreviewBlob(selectedBlobRows[0]);
  }

  async function handleDownloadPrefix(row: BlobRow) {
    if (!activeConnection || !activeContainer || !row.path || row.kind !== "dir") {
      return;
    }

    setShellError(null);
    try {
      const result = await downloadBlobPrefix(activeConnection.id, activeContainer, row.path);
      setShellError(
        `Downloaded ${result.item_count} blob${result.item_count === 1 ? "" : "s"} from ${row.name} to ${result.path}`,
      );
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleDeleteBlob(row: BlobRow) {
    if (!activeConnection || !activeContainer || !activeTab || !row.path || row.kind === "dir") {
      return;
    }

    const confirmed = window.confirm(`Delete blob "${row.path}" from "${activeContainer}"?`);
    if (!confirmed) {
      return;
    }

    setShellError(null);
    try {
      await deleteBlob(activeConnection.id, activeContainer, row.path, false);
      updateTab(activeTab.id, (tab) => ({
        ...tab,
        loaded: false,
        selectedIndices: [],
      }));
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleDeletePrefix(row: BlobRow) {
    if (!activeConnection || !activeContainer || !activeTab || !row.path || row.kind !== "dir") {
      return;
    }

    const confirmed = window.confirm(`Delete all blobs under "${row.path}" from "${activeContainer}"?`);
    if (!confirmed) {
      return;
    }

    setShellError(null);
    try {
      const result = await deleteBlobPrefix(activeConnection.id, activeContainer, row.path, false);
      setShellError(`Deleted ${result.item_count} blob${result.item_count === 1 ? "" : "s"} under ${row.name}`);
      updateTab(activeTab.id, (tab) => ({
        ...tab,
        loaded: false,
        selectedIndices: [],
      }));
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleDownloadSelection(openAfterDownload = false) {
    if (selectedResourceRows.length === 0) {
      return;
    }

    for (const row of selectedResourceRows) {
      if (row.kind === "dir") {
        await handleDownloadPrefix(row);
      } else {
        await handleDownloadBlob(row, openAfterDownload && selectedResourceRows.length === 1);
      }
    }
  }

  async function handleDeleteSelection() {
    if (!activeConnection || !activeContainer || !activeTab || selectedResourceRows.length === 0) {
      return;
    }

    const label =
      selectedResourceRows.length === 1
        ? `"${selectedResourceRows[0].path ?? selectedResourceRows[0].name}"`
        : `${selectedResourceRows.length} items`;
    const confirmed = window.confirm(`Delete ${label} from "${activeContainer}"?`);
    if (!confirmed) {
      return;
    }

    setShellError(null);
    try {
      for (const row of selectedResourceRows) {
        if (!row.path) {
          continue;
        }
        if (row.kind === "dir") {
          await deleteBlobPrefix(activeConnection.id, activeContainer, row.path, false);
        } else {
          await deleteBlob(activeConnection.id, activeContainer, row.path, false);
        }
      }
      updateTab(activeTab.id, (tab) => ({
        ...tab,
        loaded: false,
        selectedIndices: [],
      }));
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleUploadFiles() {
    if (!activeConnection || !activeContainer || !activeTab) {
      return;
    }

    setShellError(null);
    try {
      const selection = await openFileDialog({
        multiple: true,
        directory: false,
        title: `Upload to ${activeContainer}${prefix ? `/${prefix}` : ""}`,
      });
      if (!selection) {
        return;
      }

      const sourcePaths = Array.isArray(selection) ? selection : [selection];
      let uploadedCount = 0;
      for (const sourcePath of sourcePaths) {
        if (typeof sourcePath !== "string") {
          continue;
        }
        await uploadBlob(
          activeConnection.id,
          activeContainer,
          sourcePath,
          prefix || null,
          false,
        );
        uploadedCount += 1;
      }

      if (uploadedCount > 0) {
        setShellError(
          `Uploaded ${uploadedCount} file${uploadedCount === 1 ? "" : "s"} to ${activeContainer}${prefix ? `/${prefix}` : ""}`,
        );
        updateTab(activeTab.id, (tab) => ({
          ...tab,
          loaded: false,
          selectedIndices: [],
        }));
        await refreshActivities();
      }
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleUploadFolders() {
    if (!activeConnection || !activeContainer || !activeTab) {
      return;
    }

    setShellError(null);
    try {
      const selection = await openFileDialog({
        multiple: true,
        directory: true,
        title: `Upload folder to ${activeContainer}${prefix ? `/${prefix}` : ""}`,
      });
      if (!selection) {
        return;
      }

      const sourcePaths = Array.isArray(selection) ? selection : [selection];
      let folderCount = 0;
      let fileCount = 0;
      let byteCount = 0;
      for (const sourcePath of sourcePaths) {
        if (typeof sourcePath !== "string") {
          continue;
        }
        const result = await uploadFolder(
          activeConnection.id,
          activeContainer,
          sourcePath,
          prefix || null,
          false,
        );
        folderCount += 1;
        fileCount += result.item_count;
        byteCount += result.bytes;
      }

      if (folderCount > 0) {
        setShellError(
          `Uploaded ${folderCount} folder${folderCount === 1 ? "" : "s"} (${fileCount} file${fileCount === 1 ? "" : "s"}, ${formatBytesLabel(byteCount)}) to ${activeContainer}${prefix ? `/${prefix}` : ""}`,
        );
        updateTab(activeTab.id, (tab) => ({
          ...tab,
          loaded: false,
          selectedIndices: [],
        }));
        await refreshActivities();
      }
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleCreateFolder(
    connectionId = activeConnection?.id,
    containerName = activeContainer,
    parent = prefix,
  ) {
    if (!connectionId || !containerName) {
      return;
    }

    const folderName = window.prompt(`New folder name in ${containerName}${parent ? `/${parent}` : ""}`);
    if (!folderName) {
      return;
    }
    const trimmed = folderName.trim();
    if (!trimmed) {
      return;
    }
    if (/[\\/]/.test(trimmed)) {
      setShellError("Folder name cannot contain slashes. Create nested folders one level at a time.");
      return;
    }

    setShellError(null);
    try {
      const result = await createBlobFolder(connectionId, containerName, parent || null, trimmed);
      setShellError(`Created folder ${result.path}`);
      if (activeTab && activeTab.connectionId === connectionId && activeTab.containerName === containerName) {
        updateTab(activeTab.id, (tab) => ({
          ...tab,
          loaded: false,
          selectedIndices: [],
        }));
      }
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  async function handleRenameRow(row: BlobRow) {
    if (!activeConnection || !activeContainer || !activeTab || !row.path) {
      return;
    }

    const nextName = window.prompt(`Rename ${row.kind === "dir" ? "folder" : "blob"}`, row.name);
    if (!nextName) {
      return;
    }
    const trimmed = nextName.trim();
    if (!trimmed || trimmed === row.name) {
      return;
    }
    if (/[\\/]/.test(trimmed)) {
      setShellError("Rename only changes the item name. Slashes are not allowed here.");
      return;
    }

    const destination = `${parentPathPrefix(row.path)}${trimmed}${row.kind === "dir" ? "/" : ""}`;
    setShellError(null);
    try {
      const result = await renameBlobItem(
        activeConnection.id,
        activeContainer,
        row.path,
        destination,
        row.kind === "dir",
      );
      setShellError(
        row.kind === "dir"
          ? `Renamed folder to ${result.path} (${result.item_count} blob${result.item_count === 1 ? "" : "s"})`
          : `Renamed blob to ${result.path}`,
      );
      updateTab(activeTab.id, (tab) => ({
        ...tab,
        loaded: false,
        selectedIndices: [],
      }));
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  function handleCopyRow(row: BlobRow) {
    if (!activeConnection || !activeContainer || !row.path) {
      return;
    }

    setBlobClipboard({
      connectionId: activeConnection.id,
      containerName: activeContainer,
      path: row.path,
      name: row.name,
      kind: row.kind,
    });
    setShellError(`Copied ${row.name}. Paste is available in the same container.`);
  }

  async function handleCopyPreviewRows(columns: string[], rows: string[][]) {
    if (rows.length === 0) {
      return;
    }

    const cleanCell = (value: string) => value.replace(/\r?\n/g, " ").replace(/\t/g, " ").trim();
    const table = [
      columns.map(cleanCell),
      ...rows.map((row) => columns.map((_, index) => cleanCell(row[index] ?? ""))),
    ];
    await copyText(table.map((row) => row.join("\t")).join("\r\n"));
    setShellError(`Copied ${rows.length} preview row${rows.length === 1 ? "" : "s"} to clipboard.`);
  }

  async function handlePasteClipboard() {
    if (!activeConnection || !activeContainer || !activeTab || !blobClipboard) {
      return;
    }
    if (
      blobClipboard.connectionId !== activeConnection.id ||
      blobClipboard.containerName !== activeContainer
    ) {
      setShellError("Paste currently supports items copied within the same storage account container.");
      return;
    }

    setShellError(null);
    try {
      const result = await copyBlobItem(
        activeConnection.id,
        activeContainer,
        blobClipboard.path,
        activeTab.prefix || null,
        blobClipboard.kind === "dir",
      );
      setShellError(
        `Pasted ${blobClipboard.name} to ${result.path} (${result.item_count} item${result.item_count === 1 ? "" : "s"})`,
      );
      updateTab(activeTab.id, (tab) => ({
        ...tab,
        loaded: false,
        selectedIndices: [],
      }));
      await refreshActivities();
    } catch (error) {
      setShellError(getErrorMessage(error));
    }
  }

  function beginWindowResize(
    event: React.MouseEvent<HTMLElement>,
    cursor: "col-resize" | "row-resize",
    onMove: (clientX: number, clientY: number) => void,
  ) {
    event.preventDefault();
    event.stopPropagation();
    const previousCursor = document.body.style.cursor;
    const previousUserSelect = document.body.style.userSelect;
    document.body.style.cursor = cursor;
    document.body.style.userSelect = "none";

    const handleMouseMove = (moveEvent: MouseEvent) => {
      onMove(moveEvent.clientX, moveEvent.clientY);
    };
    const handleMouseUp = () => {
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousUserSelect;
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    onMove(event.clientX, event.clientY);
  }

  function handleSidebarResizeStart(event: React.MouseEvent<HTMLDivElement>) {
    const startX = event.clientX;
    const startWidth = sidebarWidth;
    beginWindowResize(event, "col-resize", (clientX) => {
      setSidebarWidth(clampNumber(startWidth + clientX - startX, SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH));
    });
  }

  function handleSidebarDetailsResizeStart(event: React.MouseEvent<HTMLDivElement>) {
    const startY = event.clientY;
    const startHeight = sidebarDetailsHeight;
    beginWindowResize(event, "row-resize", (_clientX, clientY) => {
      setSidebarDetailsHeight(
        clampNumber(startHeight + startY - clientY, SIDEBAR_DETAILS_MIN_HEIGHT, SIDEBAR_DETAILS_MAX_HEIGHT),
      );
    });
  }

  function handleDetailPaneResizeStart(event: React.MouseEvent<HTMLDivElement>) {
    if (!browserPaneRef.current) {
      return;
    }

    const paneRect = browserPaneRef.current.getBoundingClientRect();
    const maxWidth = Math.min(
      DETAIL_PANE_MAX_WIDTH,
      Math.max(DETAIL_PANE_MIN_WIDTH, paneRect.width - PANE_RESIZE_HANDLE_WIDTH - 280),
    );
    beginWindowResize(event, "col-resize", (clientX) => {
      setDetailPaneWidth(clampNumber(paneRect.right - clientX, DETAIL_PANE_MIN_WIDTH, maxWidth));
    });
  }

  function handleActivityResizeStart(event: React.MouseEvent<HTMLDivElement>) {
    const startY = event.clientY;
    const startHeight = activityPaneHeight;
    beginWindowResize(event, "row-resize", (_clientX, clientY) => {
      setActivityPaneHeight(
        clampNumber(startHeight + startY - clientY, ACTIVITY_PANE_MIN_HEIGHT, ACTIVITY_PANE_MAX_HEIGHT),
      );
    });
  }

  function menuSeparator(): ContextMenuSeparator {
    return { kind: "separator" };
  }

  function normalizeContextMenuItems(items: ContextMenuItem[]) {
    return items.filter((item, index, source) => {
      if (item.kind !== "separator") {
        return true;
      }
      const previous = source[index - 1];
      const next = source[index + 1];
      return (
        index > 0 &&
        index < source.length - 1 &&
        previous?.kind !== "separator" &&
        next?.kind !== "separator"
      );
    });
  }

  function closeContextMenu() {
    setContextMenu(null);
  }

  function openContextMenu(
    event: React.MouseEvent<HTMLDivElement>,
    items: ContextMenuItem[],
  ) {
    event.preventDefault();
    event.stopPropagation();
    const normalizedItems = normalizeContextMenuItems(items);
    if (normalizedItems.length === 0) {
      return;
    }

    setContextMenu({
      x: event.clientX,
      y: event.clientY,
      items: normalizedItems,
    });
  }

  useEffect(() => {
    void initializeShell();
  }, []);

  useEffect(() => {
    if (!shellInitialized || shellHydrated.current) {
      return;
    }

    shellHydrated.current = true;
    const snapshot = loadPersistedShellSnapshot();
    if (!snapshot) {
      setShellPersistenceReady(true);
      return;
    }
    if (typeof snapshot.sidebarWidth === "number") {
      setSidebarWidth(snapshot.sidebarWidth);
    }
    if (typeof snapshot.detailPaneWidth === "number") {
      setDetailPaneWidth(snapshot.detailPaneWidth);
    }
    if (typeof snapshot.sidebarDetailsHeight === "number") {
      setSidebarDetailsHeight(snapshot.sidebarDetailsHeight);
    }
    if (typeof snapshot.activityPaneHeight === "number") {
      setActivityPaneHeight(snapshot.activityPaneHeight);
    }

    void hydratePersistedShell(snapshot).finally(() => {
      setShellPersistenceReady(true);
    });
  }, [shellInitialized, accountsBySubscription, connections]);

  useEffect(() => {
    if (!shellInitialized || !shellPersistenceReady) {
      return;
    }

    writePersistedShellSnapshot({
      version: 1,
      activeTabId,
      activeConnectionId,
      sidebarWidth,
      detailPaneWidth,
      sidebarDetailsHeight,
      activityPaneHeight,
      expandedSignIns,
      expandedSubscriptions,
      expandedAccounts,
      tabs: browserTabs
        .map<PersistedBrowserTab | null>((tab) => {
          const connection = connections.find((candidate) => candidate.id === tab.connectionId);
          if (!connection) {
            return null;
          }

          return {
            id: tab.id,
            connectionId: connection.origin_sign_in_id ? null : connection.id,
            originSignInId: connection.origin_sign_in_id ?? null,
            originSubscriptionId: connection.origin_subscription_id ?? null,
            accountName: connection.account_name,
            endpoint: connection.endpoint,
            containerName: tab.containerName,
            prefix: tab.prefix,
            filter: tab.filter,
          };
        })
        .filter((tab): tab is PersistedBrowserTab => tab != null),
    });
  }, [
    shellInitialized,
    shellPersistenceReady,
    activeTabId,
    activeConnectionId,
    sidebarWidth,
    detailPaneWidth,
    sidebarDetailsHeight,
    activityPaneHeight,
    expandedSignIns,
    expandedSubscriptions,
    expandedAccounts,
    browserTabs,
    connections,
  ]);

  useEffect(() => {
    if (activeTabId && !browserTabs.some((tab) => tab.id === activeTabId)) {
      setActiveTabId(browserTabs[0]?.id ?? null);
      return;
    }
    if (!activeTabId && browserTabs.length > 0) {
      setActiveTabId(browserTabs[0].id);
    }
  }, [activeTabId, browserTabs]);

  useEffect(() => {
    if (!activeTabId) {
      return;
    }

    const tab = browserTabs.find((currentTab) => currentTab.id === activeTabId);
    if (tab) {
      setActiveConnectionId(tab.connectionId);
    }
  }, [activeTabId, browserTabs]);

  useEffect(() => {
    if (!activeTab || activeTab.loaded || activeTab.busy) {
      return;
    }

    void loadTabRows(activeTab.id);
  }, [
    activeTab?.id,
    activeTab?.connectionId,
    activeTab?.containerName,
    activeTab?.prefix,
    activeTab?.loaded,
    activeTab?.busy,
  ]);

  useEffect(() => {
    if (!shellInitialized || !tauriAvailable.current) {
      return;
    }

    let cancelled = false;
    let timerId: number | null = null;

    const poll = async () => {
      await refreshActivities();
      if (!cancelled) {
        timerId = window.setTimeout(poll, 1000);
      }
    };

    timerId = window.setTimeout(poll, 1000);

    return () => {
      cancelled = true;
      if (timerId != null) {
        window.clearTimeout(timerId);
      }
    };
  }, [shellInitialized]);

  useEffect(() => {
    if (!browserPrompt) {
      return;
    }

    let cancelled = false;
    let timerId: number | null = null;

    const poll = async () => {
      try {
        const status = await pollEntraBrowserLogin(browserPrompt.login_id);
        if (cancelled) {
          return;
        }

        if (status.status === "pending") {
          timerId = window.setTimeout(poll, 1500);
          return;
        }

        if (status.status === "error") {
          setBrowserPrompt(null);
          setConnectError(status.error ?? "Azure browser sign-in failed.");
          setConnectOpen(true);
          return;
        }

        setBrowserPrompt(null);
        setConnectError(null);
        setShellError(null);
        await refreshDiscoveryTree(status.sign_in_id ?? null);
        setConnectOpen(false);
        if (status.sign_in_id) {
          openManageDialog(status.sign_in_id);
        }
      } catch (error) {
        if (cancelled) {
          return;
        }

        setBrowserPrompt(null);
        setConnectError(getErrorMessage(error));
        setConnectOpen(true);
      }
    };

    timerId = window.setTimeout(poll, 750);

    return () => {
      cancelled = true;
      if (timerId != null) {
        window.clearTimeout(timerId);
      }
    };
  }, [browserPrompt]);

  useEffect(() => {
    if (!tenantBrowserPrompt || !tenantReauthFlow) {
      return;
    }

    let cancelled = false;
    let timerId: number | null = null;

    const poll = async () => {
      try {
        const status = await pollSignInTenantReauth(tenantBrowserPrompt.login_id);
        if (cancelled) {
          return;
        }

        if (status.status === "pending") {
          timerId = window.setTimeout(poll, 1500);
          return;
        }

        const { signInId, activeTenantId, queuedTenantIds } = tenantReauthFlow;
        const activeTenantLabel = tenantLabelFor(signInId, activeTenantId);
        setTenantBrowserPrompt(null);
        await refreshDiscoveryTree(status.sign_in_id ?? signInId);

        if (status.status === "error") {
          setDiscoveryError(
            `Tenant authentication failed for ${activeTenantLabel}: ${status.error ?? "Azure tenant sign-in failed."}`,
          );
        } else if (queuedTenantIds.length === 0) {
          setDiscoveryError(null);
        }

        if (queuedTenantIds.length > 0) {
          await launchTenantReauth(signInId, queuedTenantIds[0], queuedTenantIds.slice(1));
          return;
        }

        resetTenantReauthFlow();
      } catch (error) {
        if (cancelled) {
          return;
        }

        const { signInId, activeTenantId, queuedTenantIds } = tenantReauthFlow;
        const activeTenantLabel = tenantLabelFor(signInId, activeTenantId);
        setTenantBrowserPrompt(null);
        await refreshDiscoveryTree(signInId);
        setDiscoveryError(
          `Tenant authentication failed for ${activeTenantLabel}: ${getErrorMessage(error)}`,
        );

        if (queuedTenantIds.length > 0) {
          await launchTenantReauth(signInId, queuedTenantIds[0], queuedTenantIds.slice(1));
          return;
        }

        resetTenantReauthFlow();
      }
    };

    timerId = window.setTimeout(poll, 750);

    return () => {
      cancelled = true;
      if (timerId != null) {
        window.clearTimeout(timerId);
      }
    };
  }, [tenantBrowserPrompt, tenantReauthFlow, tenantsBySignIn]);

  useEffect(() => {
    if (!devicePrompt) {
      openedDevicePromptId.current = null;
      return;
    }

    if (openedDevicePromptId.current !== devicePrompt.login_id) {
      openedDevicePromptId.current = devicePrompt.login_id;
      window.open(devicePrompt.verification_uri, "_blank", "noopener,noreferrer");
    }

    let cancelled = false;
    let timerId: number | null = null;

    const poll = async () => {
      try {
        const status = await pollEntraDiscoveryLogin(devicePrompt.login_id);
        if (cancelled) {
          return;
        }

        if (status.status === "pending") {
          timerId = window.setTimeout(poll, Math.max(devicePrompt.interval_seconds, 2) * 1000);
          return;
        }

        if (status.status === "error") {
          setDevicePrompt(null);
          setConnectError(status.error ?? "Azure sign-in failed.");
          setConnectOpen(true);
          return;
        }

        setDevicePrompt(null);
        setConnectError(null);
        setShellError(null);
        await refreshDiscoveryTree(status.sign_in_id ?? null);
        setConnectOpen(false);
        if (status.sign_in_id) {
          openManageDialog(status.sign_in_id);
        }
      } catch (error) {
        if (cancelled) {
          return;
        }

        setDevicePrompt(null);
        setConnectError(getErrorMessage(error));
        setConnectOpen(true);
      }
    };

    timerId = window.setTimeout(poll, Math.max(devicePrompt.interval_seconds, 2) * 1000);

    return () => {
      cancelled = true;
      if (timerId != null) {
        window.clearTimeout(timerId);
      }
    };
  }, [devicePrompt]);

  useEffect(() => {
    if (!copiedCode) {
      return;
    }

    const timerId = window.setTimeout(() => setCopiedCode(false), 1500);
    return () => window.clearTimeout(timerId);
  }, [copiedCode]);

  useEffect(() => {
    if (!contextMenu) {
      return;
    }

    const close = () => setContextMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("blur", close);
    };
  }, [contextMenu]);

  const runtimeUnavailable = !tauriAvailable.current;
  const statusText = runtimeUnavailable
    ? "browser-only mode"
    : shellError
      ? "error"
      : browserPrompt || devicePrompt || tenantBrowserPrompt || tenantReauthBusy
      ? "auth pending"
      : connectionsBusy || signInsBusy || anyContainersBusy || anyRowsBusy
        ? "refreshing"
        : activeConnection
          ? "live"
          : signIns.length > 0
            ? "discovered"
            : "idle";

  const titleConnection = activeConnection
    ? activeConnection.display_name
    : signIns.length > 0
      ? `${signIns.length} Azure sign-in${signIns.length === 1 ? "" : "s"}`
      : "No connection";
  const visibleDiscoveredSubscriptionCount = Object.values(subscriptionsBySignIn).reduce(
    (total, subscriptions) => total + subscriptions.length,
    0,
  );
  const managedSignIn = manageSignInId ? signIns.find((signIn) => signIn.id === manageSignInId) ?? null : null;
  const managedTenants = manageSignInId ? tenantsBySignIn[manageSignInId] ?? [] : [];
  const connectionDetail = activeConnection
    ? `${activeConnection.account_name} · ${authLabel(activeConnection.auth_kind)}`
    : signIns.length > 0
      ? "Select a discovered storage account"
      : runtimeUnavailable
        ? "Desktop shell required"
        : shellError
          ? shellError
          : "Attach a storage account";

  return (
    <div style={styles.appRoot}>
      <TitleBar
        onOpenPalette={() => openConnectDialog()}
        activeConnection={titleConnection}
        connectionDetail={connectionDetail}
        connected={Boolean(activeConnection || signIns.length > 0) && !runtimeUnavailable}
        statusText={statusText}
        onRefresh={() => {
          void handleRefresh();
        }}
      />

      <div style={styles.shell}>
        <aside style={{ ...styles.sidebar, width: sidebarWidth }}>
          <div style={styles.sidebarHeader}>
            <div>
              <div style={styles.sidebarEyebrow}>Explorer</div>
              <div style={styles.sidebarTitle}>Azure tree</div>
            </div>
            <div style={styles.sidebarHeaderActions}>
              <IconButton
                title="Sign in or attach"
                onClick={() => openConnectDialog()}
                disabled={connectBusy}
                icon={<IconPlus size={12} />}
              />
              <IconButton
                title="Refresh discovery"
                onClick={() => {
                  void handleRefresh();
                }}
                disabled={connectionsBusy || signInsBusy || anyContainersBusy || anyRowsBusy}
                icon={
                  connectionsBusy || signInsBusy || anyContainersBusy || anyRowsBusy ? (
                    <IconLoader size={12} />
                  ) : (
                    <IconRefresh size={12} />
                  )
                }
              />
            </div>
          </div>

          {shellError && <Banner tone="warn" icon={<IconAlert size={12} />} text={shellError} />}
          {discoveryError && <Banner tone="error" icon={<IconAlert size={12} />} text={discoveryError} />}

          <div style={styles.sidebarBody}>
            <GroupHeader label="Azure Accounts" count={signInsBusy ? "…" : signIns.length} />

            {signIns.length === 0 && !signInsBusy && (
              <EmptySidebarState
                title="No Azure account signed in"
                detail="Use Azure sign-in to discover subscriptions and storage accounts the way Storage Explorer does."
                actionLabel="Sign in with Azure"
                onAction={() => openConnectDialog("entra-browser")}
              />
            )}

            {signIns.map((signIn) => {
              const subscriptions = subscriptionsBySignIn[signIn.id] ?? [];
              const blockedTenantCount = (tenantsBySignIn[signIn.id] ?? []).filter(
                (tenant) => tenant.needs_reauth,
              ).length;
              const signInExpanded = expandedSignIns[signIn.id] ?? true;

              return (
                <div key={signIn.id} style={styles.discoveryGroup}>
                  <TreeRow
                    depth={0}
                    expanded={subscriptions.length > 0 ? signInExpanded : undefined}
                    onToggle={subscriptions.length > 0 ? () => toggleSignIn(signIn.id) : undefined}
                    icon={<IconUser size={11} />}
                    label={signIn.display_name}
                    meta={`${signIn.selected_tenant_count}/${signIn.tenant_count} tenants`}
                    action={<IconSettings size={10} />}
                    onAction={() => openManageDialog(signIn.id)}
                    onContextMenu={(event) =>
                      openContextMenu(event, [
                        {
                          label: "Manage account",
                          action: () => {
                            openManageDialog(signIn.id);
                          },
                        },
                        {
                          label: "Refresh discovery",
                          action: () => {
                            void refreshDiscoveryTree(signIn.id);
                          },
                        },
                        {
                          label: "Authenticate blocked tenants",
                          disabled: blockedTenantCount === 0,
                          hint: blockedTenantCount > 0 ? `${blockedTenantCount}` : undefined,
                          action: () => {
                            void reauthenticateBlockedTenantsFor(signIn.id);
                          },
                        },
                        menuSeparator(),
                        {
                          label: signInExpanded ? "Collapse account" : "Expand account",
                          action: () => {
                            toggleSignIn(signIn.id);
                          },
                        },
                        {
                          label: "Copy account name",
                          action: () => {
                            void copyText(signIn.display_name);
                          },
                        },
                        menuSeparator(),
                        {
                          label: "Remove account",
                          danger: true,
                          action: () => {
                            void handleRemoveSignIn(signIn);
                          },
                        },
                      ])
                    }
                  />

                  {signInExpanded && subscriptions.map((subscription) => {
                    const accounts = accountsBySubscription[subscription.id] ?? [];
                    const subscriptionExpanded = expandedSubscriptions[subscription.id] ?? false;

                    return (
                      <React.Fragment key={subscription.id}>
                        <TreeRow
                          depth={1}
                          expanded={accounts.length > 0 ? subscriptionExpanded : undefined}
                          onToggle={accounts.length > 0 ? () => toggleSubscription(subscription.id) : undefined}
                          icon={<IconAzure size={11} />}
                          label={subscription.name}
                          meta={subscription.storage_account_count}
                          onContextMenu={(event) =>
                            openContextMenu(event, [
                              {
                                label: subscriptionExpanded ? "Collapse subscription" : "Expand subscription",
                                action: () => {
                                  toggleSubscription(subscription.id);
                                },
                              },
                              {
                                label: "Refresh subscription tree",
                                action: () => {
                                  void refreshDiscoveryTree(signIn.id);
                                },
                              },
                              menuSeparator(),
                              {
                                label: "Copy subscription name",
                                action: () => {
                                  void copyText(subscription.name);
                                },
                              },
                              {
                                label: "Copy subscription ID",
                                action: () => {
                                  void copyText(subscription.id);
                                },
                              },
                            ])
                          }
                        />

                        {subscriptionExpanded && accounts.map((account) => {
                          const isActive = isDiscoveredAccountActive(account);
                          const connection = findDiscoveredConnection(account);
                          const accountKey = discoveredRowKey(account);
                          const isExpanded = expandedAccounts[accountKey] ?? false;
                          const isBusy = activatingAccounts[accountKey] ?? false;

                          return (
                            <React.Fragment key={account.name}>
                              <TreeRow
                                depth={2}
                                expanded={isExpanded}
                                onToggle={() => {
                                  void handleToggleDiscoveredAccount(account);
                                }}
                                icon={<IconAzure size={11} />}
                                label={account.name}
                                meta={account.region}
                                badge={account.hns ? "ADLS" : account.tier === "Premium" ? "P" : null}
                                selected={isActive}
                                onClick={() => {
                                  void handleSelectDiscoveredAccount(account);
                                }}
                                onContextMenu={(event) =>
                                  openContextMenu(event, [
                                    {
                                      label: connection ? "Open account" : "Connect account",
                                      action: () => {
                                        void handleSelectDiscoveredAccount(account);
                                      },
                                    },
                                    {
                                      label: isExpanded ? "Collapse account" : "Expand account",
                                      action: () => {
                                        void handleToggleDiscoveredAccount(account);
                                      },
                                    },
                                    {
                                      label: "Refresh containers",
                                      disabled: !connection,
                                      action: () => {
                                        if (!connection) {
                                          return;
                                        }
                                        void ensureContainersLoaded(connection.id, true);
                                      },
                                    },
                                    menuSeparator(),
                                    {
                                      label: "Copy account name",
                                      action: () => {
                                        void copyText(account.name);
                                      },
                                    },
                                    {
                                      label: "Copy endpoint",
                                      action: () => {
                                        void copyText(account.endpoint);
                                      },
                                    },
                                    menuSeparator(),
                                    {
                                      label: "Properties",
                                      disabled: true,
                                      hint: "soon",
                                      action: () => undefined,
                                    },
                                  ])
                                }
                              />

                              {isExpanded && renderContainerBranch(connection?.id ?? null, 3, isBusy)}
                            </React.Fragment>
                          );
                        })}
                      </React.Fragment>
                    );
                  })}
                </div>
              );
            })}

            <GroupHeader label="Direct Attachments" count={connectionsBusy ? "…" : directConnections.length} />

            {directConnections.length === 0 && !connectionsBusy && (
              <div style={styles.inlineEmptyBlock}>
                Connection string, SAS, shared-key, and Azurite attachments appear here.
              </div>
            )}

            {directConnections.map((connection) => {
              const isActive = connection.id === browsingConnectionId;
              const accountKey = directRowKey(connection.id);
              const isExpanded = expandedAccounts[accountKey] ?? false;
              return (
                <div key={connection.id} style={styles.discoveryGroup}>
                  <TreeRow
                    depth={0}
                    expanded={isExpanded}
                    onToggle={() => {
                      void handleToggleDirectAccount(connection);
                    }}
                    icon={connectionIcon(connection.auth_kind)}
                    label={connection.display_name}
                    meta={compactAuthLabel(connection.auth_kind)}
                    selected={isActive}
                    onClick={() => handleSelectConnection(connection.id)}
                    onContextMenu={(event) =>
                      openContextMenu(event, [
                        {
                          label: "Open account",
                          action: () => {
                            handleSelectConnection(connection.id);
                          },
                        },
                        {
                          label: isExpanded ? "Collapse account" : "Expand account",
                          action: () => {
                            void handleToggleDirectAccount(connection);
                          },
                        },
                        {
                          label: "Refresh containers",
                          action: () => {
                            void ensureContainersLoaded(connection.id, true);
                          },
                        },
                        menuSeparator(),
                        {
                          label: "Copy account name",
                          action: () => {
                            void copyText(connection.display_name);
                          },
                        },
                        {
                          label: "Copy endpoint",
                          action: () => {
                            void copyText(connection.endpoint);
                          },
                        },
                        menuSeparator(),
                        {
                          label: "Detach",
                          danger: true,
                          action: () => {
                            void handleDisconnect(connection.id);
                          },
                        },
                      ])
                    }
                  />
                  <div style={styles.connectionMeta}>{compactHost(connection.endpoint)}</div>
                  {isExpanded && renderContainerBranch(connection.id, 1)}
                </div>
              );
            })}
          </div>

          <div
            role="separator"
            aria-orientation="horizontal"
            aria-label="Resize explorer details pane"
            title="Drag to resize Actions / Properties"
            style={styles.horizontalPaneResizeHandle}
            onMouseDown={handleSidebarDetailsResizeStart}
          />
          {renderSidebarDetailsPanel()}
        </aside>

        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize explorer pane"
          title="Drag to resize explorer"
          style={styles.shellVerticalResizeHandle}
          onMouseDown={handleSidebarResizeStart}
        />

        <main style={styles.main}>
          <div style={styles.toolbar}>
            <ToolbarButton
              label="Sign in"
              icon={<IconUser size={12} />}
              onClick={() => openConnectDialog("entra-browser")}
            />
            <ToolbarButton
              label="Attach"
              icon={<IconPlus size={12} />}
              onClick={() => openConnectDialog("connection-string")}
            />
            <ToolbarButton
              label="Refresh"
              icon={connectionsBusy || signInsBusy || anyContainersBusy || anyRowsBusy ? <IconLoader size={12} /> : <IconRefresh size={12} />}
              onClick={() => {
                void handleRefresh();
              }}
              disabled={!tauriAvailable.current || connectionsBusy || signInsBusy || anyContainersBusy || anyRowsBusy}
            />
            <ToolbarButton
              label="Up"
              icon={<IconArrowUp size={12} />}
              onClick={handleGoUp}
              disabled={!activeTab || !prefix}
            />
            <ToolbarButton
              label={disconnectBusy ? "Detaching…" : "Detach"}
              icon={disconnectBusy ? <IconLoader size={12} /> : <IconPlug size={12} />}
              onClick={() => {
                void handleDisconnect();
              }}
              disabled={!activeConnectionId || disconnectBusy}
              tone="danger"
            />
            <div style={{ flex: 1 }} />
            {activeConnection && (
              <div style={styles.toolbarPill}>
                <IconAzure size={11} />
                <span>{authLabel(activeConnection.auth_kind)}</span>
                <span style={styles.toolbarPillDivider}>•</span>
                <span>{compactHost(activeConnection.endpoint)}</span>
              </div>
            )}
          </div>

          {!activeConnection && signIns.length === 0 && (
            <MainEmptyState
              title="Sign in to Azure"
              body={
                tauriAvailable.current
                  ? "Use browser-based Azure sign-in to discover subscriptions and storage accounts, or attach directly with a connection string, shared key, SAS, or Azurite."
                  : "The frontend is running outside Tauri, so the live Azure IPC layer is unavailable in this window."
              }
              primaryLabel="Open sign-in"
              onPrimary={() => openConnectDialog("entra-browser")}
              secondaryLabel={tauriAvailable.current ? undefined : "Use `npm run tauri:dev`"}
            />
          )}

          {!activeConnection && signIns.length > 0 && (
            <MainEmptyState
              title="Choose a discovered storage account"
              body={
                browserPrompt || devicePrompt || tenantBrowserPrompt || tenantReauthBusy
                  ? "Arkived is waiting for Azure authentication to finish. When the sign-in completes, the ARM discovery tree will refresh automatically."
                  : visibleDiscoveredSubscriptionCount === 0
                    ? "This Azure account is connected, but no tenants or subscriptions are currently selected for loading. Open the account filter to choose what should appear in the explorer."
                    : "Your Azure account is signed in and the ARM tree has been discovered. Pick a storage account from the left explorer to activate it and browse live containers and blobs."
              }
            />
          )}

          {browserTabs.length > 0 && (
            <TabsBar
              tabs={browserTabs.map((tab) => ({
                id: tab.id,
                label: tab.containerName,
                icon: <IconContainer size={11} />,
                dirty: tab.busy,
              }))}
              active={activeTabId ?? browserTabs[0].id}
              onSelect={(tabId) => {
                setActiveTabId(tabId);
              }}
              onClose={(tabId) => {
                setBrowserTabs((current) => {
                  const index = current.findIndex((tab) => tab.id === tabId);
                  const next = current.filter((tab) => tab.id !== tabId);
                  if (activeTabId === tabId) {
                    const fallback = next[index] ?? next[index - 1] ?? null;
                    setActiveTabId(fallback?.id ?? null);
                    if (fallback) {
                      setActiveConnectionId(fallback.connectionId);
                    }
                  }
                  return next;
                });
              }}
              onNew={() => openConnectDialog()}
            />
          )}

          {activeConnection && !activeContainer && anyContainersBusy && (
            <MainEmptyState
              title="Loading containers"
              body="The selected storage account is live. Container metadata is being fetched now."
            />
          )}

          {activeConnection && !activeContainer && activeConnectionId && containerStatesByConnection[activeConnectionId]?.error && (
            <MainEmptyState
              title="Container lookup failed"
              body={containerStatesByConnection[activeConnectionId]?.error ?? "Container lookup failed."}
              primaryLabel="Try again"
              onPrimary={() => {
                if (activeConnectionId) {
                  void ensureContainersLoaded(activeConnectionId, true);
                }
              }}
            />
          )}

          {activeConnection && !activeContainer && !anyContainersBusy && !containerStatesByConnection[activeConnectionId ?? ""]?.error && (
            <MainEmptyState
              title="No container selected"
              body="Choose a container from the left explorer tree. Each container opens as its own tab, so you can keep multiple storage accounts expanded and switch between live listings without losing your place."
            />
          )}

          {activeConnection && activeContainer && activeTab && (
            <>
              <div style={styles.pathBar}>
                <div style={styles.pathTrail}>
                  <button
                    type="button"
                    style={styles.pathButtonActive}
                    onClick={() => {
                      updateTab(activeTab.id, (tab) => ({
                        ...tab,
                        prefix: "",
                        filter: "",
                        loaded: false,
                        error: null,
                        continuation: null,
                        selectedIndices: [],
                      }));
                    }}
                  >
                    {activeContainer}
                  </button>

                  {breadcrumbSegments.map((segment) => (
                    <React.Fragment key={segment.value}>
                      <span style={styles.pathDivider}>/</span>
                      <button
                        type="button"
                        style={segment.value === prefix ? styles.pathButtonActive : styles.pathButton}
                        onClick={() => {
                          updateTab(activeTab.id, (tab) => ({
                            ...tab,
                            prefix: segment.value,
                            filter: "",
                            loaded: false,
                            error: null,
                            continuation: null,
                            selectedIndices: [],
                          }));
                        }}
                      >
                        {segment.label}
                      </button>
                    </React.Fragment>
                  ))}
                </div>

                <div style={styles.pathMeta}>
                  <input
                    type="search"
                    aria-label="Filter blobs by prefix"
                    placeholder="Filter by prefix"
                    value={activeTab.filter}
                    style={styles.pathFilterInput}
                    onChange={(event) => {
                      const nextFilter = event.currentTarget.value;
                      updateTab(activeTab.id, (tab) => ({
                        ...tab,
                        filter: nextFilter,
                        loaded: false,
                        error: null,
                        continuation: null,
                        selectedIndices: [],
                      }));
                    }}
                  />
                  {activeTab.busy && (
                    <span style={styles.pathStatus}>
                      <IconLoader size={11} />
                      Loading…
                    </span>
                  )}
                  <span style={styles.pathCount}>
                    {activeRows.length} {activeRows.length === 1 ? "item" : "items"}
                    {activeTab.filter ? " matched" : ""}
                  </span>
                </div>
              </div>

              <ActionBar
                selectedCount={selectedResourceRows.length}
                canPreview={canPreviewSelection}
                onUpload={() => {
                  void handleUploadFiles();
                }}
                onUploadFolder={() => {
                  void handleUploadFolders();
                }}
                onDownload={() => {
                  void handleDownloadSelection(false);
                }}
                onPreview={() => {
                  void handlePreviewSelection();
                }}
                onCreateFolder={() => {
                  void handleCreateFolder();
                }}
                canPaste={Boolean(
                  blobClipboard &&
                    activeConnection &&
                    activeContainer &&
                    blobClipboard.connectionId === activeConnection.id &&
                    blobClipboard.containerName === activeContainer,
                )}
                onPaste={() => {
                  void handlePasteClipboard();
                }}
                onDelete={() => {
                  void handleDeleteSelection();
                }}
                onRefresh={() => {
                  updateTab(activeTab.id, (tab) => ({ ...tab, loaded: false }));
                }}
              />

              {activeTab.error ? (
                <MainEmptyState
                  title="Blob listing failed"
                  body={activeTab.error}
                  primaryLabel="Retry"
                  onPrimary={() => {
                    updateTab(activeTab.id, (tab) => ({ ...tab, loaded: false }));
                  }}
                />
              ) : (
                <div
                  ref={browserPaneRef}
                  style={{
                    ...styles.browserPane,
                    gridTemplateColumns: `minmax(0, 1fr) ${PANE_RESIZE_HANDLE_WIDTH}px ${detailPaneWidth}px`,
                  }}
                >
                  <div style={styles.tablePane}>
                    {activeRows.length === 0 && !activeTab.busy ? (
                      <MainEmptyState
                        title="This prefix is empty"
                        body="The live container responded successfully, but there are no blobs or virtual directories at the current prefix."
                      />
                    ) : (
                      <>
                        <BlobTable
                          rows={activeRows}
                          selected={selectedRows}
                          onToggleSelect={handleToggleSelection}
                          onSelectRow={handleSelectRow}
                          onSelectAll={handleToggleSelectAll}
                          onDelete={() => undefined}
                          onActivateRow={handleActivateRow}
                          onContextMenuRow={(index, row, event) => {
                          if (!selectedRows.has(index)) {
                            updateTab(activeTab.id, (tab) => ({
                              ...tab,
                              selectedIndices: [index],
                            }));
                          }
                          const rowUrl =
                            activeConnection && activeContainer && row.path
                              ? buildResourceUrl(activeConnection.endpoint, activeContainer, row.path)
                              : null;
                          const contextRows = selectedRows.has(index) ? selectedResourceRows : [row];
                          const contextBlobRows = contextRows.filter((item) => item.kind !== "dir");
                          const contextHasFolders = contextRows.some((item) => item.kind === "dir");
                          openContextMenu(event, [
                            {
                              label: row.kind === "dir" ? "Open" : "Open",
                              action: () => {
                                if (row.kind === "dir") {
                                  handleActivateRow(index);
                                } else {
                                  void handleDownloadBlob(row, true);
                                }
                              },
                            },
                            {
                              label: contextRows.length > 1 ? `Download (${contextRows.length})` : "Download",
                              action: () => {
                                if (selectedRows.has(index) && contextRows.length > 1) {
                                  void handleDownloadSelection(false);
                                } else if (row.kind === "dir") {
                                  void handleDownloadPrefix(row);
                                } else {
                                  void handleDownloadBlob(row, false);
                                }
                              },
                            },
                            {
                              label: "Preview",
                              disabled: contextRows.length !== 1 || row.kind === "dir",
                              action: () => {
                                void handlePreviewBlob(row);
                              },
                            },
                            {
                              label: "Rename…",
                              action: () => {
                                void handleRenameRow(row);
                              },
                            },
                            menuSeparator(),
                            {
                              label: "Copy",
                              action: () => {
                                handleCopyRow(row);
                              },
                            },
                            {
                              label: "Paste",
                              disabled:
                                !blobClipboard ||
                                blobClipboard.connectionId !== activeConnection?.id ||
                                blobClipboard.containerName !== activeContainer,
                              action: () => {
                                void handlePasteClipboard();
                              },
                            },
                            {
                              label: "Clone…",
                              disabled: true,
                              action: () => undefined,
                            },
                            menuSeparator(),
                            {
                              label: contextRows.length > 1 ? `Delete (${contextRows.length})` : "Delete",
                              danger: true,
                              action: () => {
                                if (selectedRows.has(index) && contextRows.length > 1) {
                                  void handleDeleteSelection();
                                } else if (row.kind === "dir") {
                                  void handleDeletePrefix(row);
                                } else {
                                  void handleDeleteBlob(row);
                                }
                              },
                            },
                            {
                              label: "Undelete",
                              disabled: true,
                              hint: "›",
                              action: () => undefined,
                            },
                            menuSeparator(),
                            {
                              label: "Copy path",
                              action: () => {
                                void copyText(row.path ?? row.name);
                              },
                            },
                            {
                              label: "Copy URL",
                              disabled: !rowUrl,
                              action: () => {
                                if (rowUrl) {
                                  void copyText(rowUrl);
                                }
                              },
                            },
                            {
                              label: "Copy direct link",
                              disabled: !rowUrl,
                              action: () => {
                                if (rowUrl) {
                                  void copyText(rowUrl);
                                }
                              },
                            },
                            menuSeparator(),
                            {
                              label: "Clone and Rehydrate…",
                              disabled: true,
                              action: () => undefined,
                            },
                            {
                              label: "Change Access Tier…",
                              disabled: true,
                              action: () => undefined,
                            },
                            menuSeparator(),
                            {
                              label: "Get Shared Access Signature…",
                              disabled: true,
                              action: () => undefined,
                            },
                            {
                              label: "Acquire Lease",
                              disabled: true,
                              action: () => undefined,
                            },
                            {
                              label: "Break Lease",
                              disabled: true,
                              action: () => undefined,
                            },
                            menuSeparator(),
                            {
                              label: "Create Snapshot",
                              disabled: true,
                              action: () => undefined,
                            },
                            {
                              label: "Manage History",
                              disabled: true,
                              hint: "›",
                              action: () => undefined,
                            },
                            {
                              label: "Selection Statistics",
                              action: () => {
                                void copyText(
                                  `${contextRows.length} selected • ${contextBlobRows.length} blob${contextBlobRows.length === 1 ? "" : "s"} • ${contextHasFolders ? "folders included" : "no folders"}`,
                                );
                              },
                            },
                            menuSeparator(),
                            {
                              label: "Edit Tags…",
                              disabled: true,
                              action: () => undefined,
                            },
                            {
                              label: "Properties…",
                              action: () => {
                                updateTab(activeTab.id, (tab) => ({
                                  ...tab,
                                  selectedIndices: [index],
                                }));
                              },
                            },
                            {
                              label: "Pin to Quick Access",
                              disabled: true,
                              action: () => undefined,
                            },
                            menuSeparator(),
                            {
                              label: "Refresh listing",
                              action: () => {
                                updateTab(activeTab.id, (tab) => ({ ...tab, loaded: false }));
                              },
                            },
                          ]);
                          }}
                        />
                        <div style={styles.blobListFooter}>
                          <span>
                            Showing {activeRows.length.toLocaleString()} cached item{activeRows.length === 1 ? "" : "s"}
                            {activeTab.filter ? ` for prefix "${activeTab.filter}"` : ""}
                          </span>
                          {activeRowsHaveMore ? (
                            <span style={styles.blobListFooterHint}>More results are available from Azure.</span>
                          ) : (
                            <span style={styles.blobListFooterHint}>End of current listing.</span>
                          )}
                          <span style={{ flex: 1 }} />
                          <button
                            type="button"
                            style={{
                              ...styles.blobListFooterButton,
                              ...(!activeRowsHaveMore || activeTab.busy ? styles.blobListFooterButtonDisabled : {}),
                            }}
                            disabled={!activeRowsHaveMore || activeTab.busy}
                            onClick={() => {
                              void loadMoreTabRows(activeTab.id);
                            }}
                          >
                            {activeTab.busy ? "Loading…" : "Load more"}
                          </button>
                        </div>
                      </>
                    )}
                  </div>

                  <div
                    role="separator"
                    aria-orientation="vertical"
                    aria-label="Resize detail pane"
                    title={previewDialog ? "Drag to resize preview" : "Drag to resize inspector"}
                    style={styles.previewResizeHandle}
                    onMouseDown={handleDetailPaneResizeStart}
                  />

                  {previewDialog ? (
                      <BlobPreviewPane
                        state={previewDialog}
                        onClose={() => setPreviewDialog(null)}
                        onPage={(rowOffset, rowLimit) => {
                          void handlePreviewBlob(previewDialog.row, rowOffset, rowLimit);
                        }}
                        onOpenContextMenu={openContextMenu}
                        onCopyRows={(columns, rows) => {
                          void handleCopyPreviewRows(columns, rows);
                        }}
                        onDownload={() => {
                          void handleDownloadBlob(previewDialog.row, false);
                        }}
                        onOpenExternal={() => {
                          void handleDownloadBlob(previewDialog.row, true);
                        }}
                      />
                  ) : (
                    <aside style={styles.inspectorPane}>
                      <div style={styles.inspectorHeader}>
                        <IconInfo size={12} />
                        <span>Selection</span>
                      </div>
                      {selectedRow ? (
                        <Inspector
                          row={selectedRow}
                          resourceUrl={resourceUrl}
                          containerName={activeContainer}
                          endpoint={activeConnection.endpoint}
                          authKind={authLabel(activeConnection.auth_kind)}
                        />
                      ) : (
                        <div style={styles.inspectorEmpty}>
                          Select a blob or virtual folder to inspect its live metadata.
                        </div>
                      )}
                    </aside>
                  )}
                </div>
              )}
            </>
          )}
          <ActivityBar
            expanded={activityExpanded}
            onToggle={() => setActivityExpanded((current) => !current)}
            activities={activities}
            expandedHeight={activityPaneHeight}
            onResizeStart={handleActivityResizeStart}
            onCancelActivity={(activityId) => {
              void handleCancelActivity(activityId);
            }}
            onClearCompleted={() => {
              void handleClearActivities("completed");
            }}
            onClearSuccessful={() => {
              void handleClearActivities("successful");
            }}
          />
        </main>
      </div>

      {connectOpen && (
        <ConnectDialog
          method={connectMethod}
          busy={connectBusy}
          form={form}
          error={connectError}
          runtimeAvailable={tauriAvailable.current}
          browserPrompt={browserPrompt}
          devicePrompt={devicePrompt}
          copiedCode={copiedCode}
          onClose={closeConnectDialog}
          onSubmit={handleConnectSubmit}
          onMethodChange={(method) => {
            if (browserPrompt || devicePrompt) {
              return;
            }
            const nextIsAzure = method === "entra-browser" || method === "entra-device-code";
            const currentIsAzure = connectMethod === "entra-browser" || connectMethod === "entra-device-code";
            if (nextIsAzure && !currentIsAzure) {
              resetAzureSignInFormDefaults();
            }
            setConnectMethod(method);
            setConnectError(null);
          }}
          onFormChange={updateForm}
          onCopyUserCode={() => {
            void handleCopyUserCode();
          }}
          onOpenBrowserAgain={() => {
            if (browserPrompt) {
              window.open(browserPrompt.authorize_url, "_blank", "noopener,noreferrer");
            }
          }}
        />
      )}

      {manageSignInId && managedSignIn && (
        <AccountFilterDialog
          signIn={managedSignIn}
          tenants={managedTenants}
          busy={manageBusy}
          reauthBusy={tenantReauthBusy}
          activeTenantId={tenantReauthFlow?.signInId === manageSignInId ? tenantReauthFlow.activeTenantId : null}
          queuedTenantCount={
            tenantReauthFlow?.signInId === manageSignInId ? tenantReauthFlow.queuedTenantIds.length : 0
          }
          onClose={() => {
            if (!manageBusy) {
              setManageSignInId(null);
            }
          }}
          onApply={handleApplyTenantFilter}
          onReauthenticateTenant={(tenantId) => {
            void handleReauthenticateTenant(tenantId);
          }}
          onReauthenticateAll={() => {
            void handleReauthenticateBlockedTenants();
          }}
          onOpenBrowserAgain={() => {
            if (tenantBrowserPrompt) {
              window.open(tenantBrowserPrompt.authorize_url, "_blank", "noopener,noreferrer");
            }
          }}
        />
      )}

      {contextMenu && (
        <div
          style={{
            position: "fixed",
            left: Math.min(contextMenu.x, window.innerWidth - 224),
            top: Math.min(contextMenu.y, window.innerHeight - 240),
            minWidth: 220,
            padding: 6,
            borderRadius: 10,
            background: "rgba(10, 12, 18, 0.98)",
            border: "1px solid var(--border-1)",
            boxShadow: "0 18px 40px rgba(0, 0, 0, 0.45)",
            zIndex: 60,
          }}
          onClick={(event) => event.stopPropagation()}
        >
          {contextMenu.items.map((item, index) =>
            item.kind === "separator" ? (
              <div
                key={`separator-${index}`}
                style={{
                  height: 1,
                  margin: "6px 6px",
                  background: "var(--border-0)",
                }}
              />
            ) : (
              <button
                key={`${item.label}-${index}`}
                type="button"
                disabled={item.disabled}
                style={{
                  width: "100%",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 12,
                  padding: "8px 10px",
                  borderRadius: 8,
                  color: item.disabled ? "var(--fg-4)" : item.danger ? "var(--red)" : "var(--fg-1)",
                  fontFamily: "var(--sans)",
                  fontSize: 12,
                  textAlign: "left",
                  cursor: item.disabled ? "not-allowed" : "pointer",
                }}
                onClick={() => {
                  closeContextMenu();
                  if (!item.disabled) {
                    void item.action();
                  }
                }}
              >
                <span>{item.label}</span>
                {item.hint && (
                  <span style={{ color: "var(--fg-3)", fontSize: 11 }}>{item.hint}</span>
                )}
              </button>
            ),
          )}
        </div>
      )}
    </div>
  );

  function renderSidebarDetailsPanel() {
    const containerUrl =
      activeConnection && activeContainer
        ? new URL(`${activeContainer}/`, activeConnection.endpoint).toString()
        : null;
    const selectedUrl =
      activeConnection && activeContainer && selectedRow?.path
        ? buildResourceUrl(activeConnection.endpoint, activeContainer, selectedRow.path)
        : null;
    const resourceName =
      selectedRow?.name ?? activeContainer ?? activeConnection?.display_name ?? "No resource selected";
    const resourceType = selectedRow
      ? selectedRow.kind === "dir"
        ? "Virtual directory"
        : "Blob"
      : activeContainer
        ? "Blob container"
        : activeConnection
          ? "Storage account"
          : "Explorer";

    const action = (
      label: string,
      onClick: () => void,
      disabled = false,
      danger = false,
    ) => (
      <button
        key={label}
        type="button"
        style={{
          ...styles.sidebarActionButton,
          ...(disabled ? styles.sidebarActionButtonDisabled : {}),
          ...(danger && !disabled ? styles.sidebarActionButtonDanger : {}),
        }}
        disabled={disabled}
        onClick={onClick}
      >
        {label}
      </button>
    );
    const property = (label: string, value?: ReactNode | null) => (
      <div key={label} style={styles.sidebarPropertyRow}>
        <span style={styles.sidebarPropertyLabel}>{label}</span>
        <span style={styles.sidebarPropertyValue}>{value || "—"}</span>
      </div>
    );

    return (
      <div style={{ ...styles.sidebarDetailsPanel, height: sidebarDetailsHeight }}>
        <div style={styles.sidebarPanelTabs}>
          <button
            type="button"
            style={{
              ...styles.sidebarPanelTab,
              ...(sidebarPanelTab === "actions" ? styles.sidebarPanelTabActive : {}),
            }}
            onClick={() => setSidebarPanelTab("actions")}
          >
            Actions
          </button>
          <button
            type="button"
            style={{
              ...styles.sidebarPanelTab,
              ...(sidebarPanelTab === "properties" ? styles.sidebarPanelTabActive : {}),
            }}
            onClick={() => setSidebarPanelTab("properties")}
          >
            Properties
          </button>
        </div>

        {sidebarPanelTab === "actions" ? (
          <div style={styles.sidebarPanelBody}>
            {action("Open", () => {
              if (selectedRow?.kind === "dir") {
                const index = activeRows.findIndex((row) => row.path === selectedRow.path);
                if (index >= 0) {
                  handleActivateRow(index);
                }
              }
            }, !selectedRow || selectedRow.kind !== "dir")}
            {action("Upload files…", () => {
              void handleUploadFiles();
            }, !activeConnection || !activeContainer)}
            {action("Upload folder…", () => {
              void handleUploadFolders();
            }, !activeConnection || !activeContainer)}
            {action("Download", () => {
              if (!selectedRow) {
                return;
              }
              if (selectedRow.kind === "dir") {
                void handleDownloadPrefix(selectedRow);
              } else {
                void handleDownloadBlob(selectedRow, false);
              }
            }, !selectedRow)}
            {action("Preview", () => {
              if (selectedRow && selectedRow.kind === "blob") {
                void handlePreviewBlob(selectedRow);
              }
            }, !selectedRow || selectedRow.kind !== "blob")}
            {action("Delete", () => {
              if (!selectedRow) {
                return;
              }
              if (selectedRow.kind === "dir") {
                void handleDeletePrefix(selectedRow);
              } else {
                void handleDeleteBlob(selectedRow);
              }
            }, !selectedRow, true)}
            {action("Copy URL", () => {
              void copyText(selectedUrl ?? containerUrl ?? activeConnection?.endpoint ?? "");
            }, !selectedUrl && !containerUrl && !activeConnection)}
            {action("Refresh", () => {
              if (activeTab) {
                updateTab(activeTab.id, (tab) => ({ ...tab, loaded: false }));
              } else if (activeConnectionId) {
                void ensureContainersLoaded(activeConnectionId, true);
              } else {
                void handleRefresh();
              }
            })}
          </div>
        ) : (
          <div style={styles.sidebarPanelBody}>
            {property("Name", resourceName)}
            {property("Type", resourceType)}
            {property("Account", activeConnection?.account_name)}
            {property("Container", activeContainer)}
            {property("Path", selectedRow?.path)}
            {property("URL", selectedUrl ?? containerUrl ?? activeConnection?.endpoint)}
            {property("Auth", activeConnection ? authLabel(activeConnection.auth_kind) : null)}
            {property("Modified", selectedRow?.modified)}
            {property("Size", selectedRow?.size)}
            {property("ETag", selectedRow?.etag)}
          </div>
        )}
      </div>
    );
  }

  function renderContainerBranch(connectionId: string | null, depth: number, connecting = false) {
    if (!connectionId) {
      return connecting ? (
        <div style={treeHintStyle(depth)}>
          <IconLoader size={11} />
          <span>Connecting account…</span>
        </div>
      ) : (
        <div style={treeEmptyStyle(depth)}>Select this account to load containers.</div>
      );
    }

    const containerState = containerStatesByConnection[connectionId];
    if (connecting || containerState?.busy) {
      return (
        <div style={treeHintStyle(depth)}>
          <IconLoader size={11} />
          <span>Loading containers…</span>
        </div>
      );
    }

    if (!containerState) {
      return <div style={treeEmptyStyle(depth)}>Expand this account to load containers.</div>;
    }

    if (containerState.error) {
      return <div style={treeErrorStyle(depth)}>{containerState.error}</div>;
    }

    if (containerState.items.length === 0) {
      return <div style={treeEmptyStyle(depth)}>No containers available.</div>;
    }

    return containerState.items.map((container) => (
      <TreeRow
        key={`${connectionId}:${container.id}`}
        depth={depth}
        icon={<IconContainer size={11} />}
        label={container.name}
        meta={container.public_access ?? undefined}
        selected={
          activeTab?.connectionId === connectionId &&
          activeTab.containerName === container.name
        }
        onClick={() => handleSelectContainer(connectionId, container.name)}
        onContextMenu={(event) =>
          openContextMenu(event, [
            {
              label: "Open",
              action: () => {
                handleSelectContainer(connectionId, container.name);
              },
            },
            {
              label: "Open in new tab",
              hint: "tab",
              action: () => {
                handleSelectContainer(connectionId, container.name);
              },
            },
            {
              label: "New folder…",
              action: () => {
                handleSelectContainer(connectionId, container.name);
                void handleCreateFolder(connectionId, container.name, "");
              },
            },
            menuSeparator(),
            {
              label: "Refresh account containers",
              action: () => {
                void ensureContainersLoaded(connectionId, true);
              },
            },
            menuSeparator(),
            {
              label: "Copy container name",
              action: () => {
                void copyText(container.name);
              },
            },
            {
              label: "Copy container URL",
              action: () => {
                const connection = connectionsRef.current.find(
                  (candidate) => candidate.id === connectionId,
                );
                if (connection) {
                  void copyText(new URL(`${container.name}/`, connection.endpoint).toString());
                }
              },
            },
            menuSeparator(),
            {
              label: "Properties",
              disabled: true,
              hint: "soon",
              action: () => undefined,
            },
            {
              label: "Manage Stored Access Policies…",
              disabled: true,
              action: () => undefined,
            },
            {
              label: "Get Shared Access Signature…",
              disabled: true,
              action: () => undefined,
            },
            {
              label: "Set Public Access Level…",
              disabled: true,
              action: () => undefined,
            },
            {
              label: "Pin to Quick Access",
              disabled: true,
              action: () => undefined,
            },
          ])
        }
      />
    ));
  }
}

interface ConnectDialogProps {
  method: ConnectMethod;
  busy: boolean;
  form: ConnectionFormState;
  error: string | null;
  runtimeAvailable: boolean;
  browserPrompt: BrowserLoginPrompt | null;
  devicePrompt: DeviceCodePrompt | null;
  copiedCode: boolean;
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onMethodChange: (method: ConnectMethod) => void;
  onFormChange: <K extends keyof ConnectionFormState>(field: K, value: ConnectionFormState[K]) => void;
  onCopyUserCode: () => void;
  onOpenBrowserAgain: () => void;
}

function ConnectDialog({
  method,
  busy,
  form,
  error,
  runtimeAvailable,
  browserPrompt,
  devicePrompt,
  copiedCode,
  onClose,
  onSubmit,
  onMethodChange,
  onFormChange,
  onCopyUserCode,
  onOpenBrowserAgain,
}: ConnectDialogProps) {
  const descriptor = CONNECT_METHODS.find((entry) => entry.id === method) ?? CONNECT_METHODS[0];
  const showBrowserPrompt = method === "entra-browser" && Boolean(browserPrompt);
  const showDevicePrompt = method === "entra-device-code" && Boolean(devicePrompt);
  const showPrompt = showBrowserPrompt || showDevicePrompt;
  const showTenantAdvanced = method === "entra-browser" || method === "entra-device-code";
  const [advancedTenantOpen, setAdvancedTenantOpen] = useState(false);

  useEffect(() => {
    setAdvancedTenantOpen(false);
  }, [method]);

  return (
    <div style={styles.overlay}>
      <div style={styles.dialog}>
        <div style={styles.dialogSidebar}>
          <div style={styles.dialogSidebarTitle}>Connect to storage</div>
          <div style={styles.dialogSidebarBody}>
            {CONNECT_METHODS.map((entry) => (
              <button
                key={entry.id}
                type="button"
                style={{
                  ...styles.methodButton,
                  ...(entry.id === method ? styles.methodButtonActive : {}),
                  ...(showPrompt ? styles.methodButtonLocked : {}),
                }}
                onClick={() => onMethodChange(entry.id)}
                disabled={showPrompt}
              >
                <span style={styles.methodIcon}>{connectionIcon(entry.id)}</span>
                <span style={styles.methodTextBlock}>
                  <span style={styles.methodLabel}>{entry.label}</span>
                  <span style={styles.methodDescription}>{entry.description}</span>
                </span>
              </button>
            ))}
          </div>
        </div>

        <form style={styles.dialogMain} onSubmit={onSubmit}>
          <div style={styles.dialogHeader}>
            <div>
              <div style={styles.dialogEyebrow}>Authentication</div>
              <h2 style={styles.dialogTitle}>{descriptor.label}</h2>
              <p style={styles.dialogDescription}>{descriptor.description}</p>
            </div>
            <button type="button" style={styles.closeButton} onClick={onClose}>
              Close
            </button>
          </div>

          {!runtimeAvailable && (
            <Banner
              tone="warn"
              icon={<IconAlert size={12} />}
              text="Live auth is only available inside the Tauri desktop shell. Plain Vite dev mode cannot reach the Azure backend bridge."
            />
          )}

          {error && <Banner tone="error" icon={<IconAlert size={12} />} text={error} />}

          <div style={styles.dialogContent}>
            {showPrompt ? (
              <div style={styles.promptCard}>
                <div style={styles.promptStatus}>
                  <IconLoader size={13} />
                  <span>{showBrowserPrompt ? "Waiting for browser OAuth callback…" : "Waiting for device code approval…"}</span>
                </div>

                {showBrowserPrompt ? (
                  <>
                    <div style={styles.promptMessage}>
                      Your system browser should already be open. Pick an Azure account there, finish OAuth, and Arkived will refresh the ARM tree automatically.
                    </div>

                    <div style={styles.promptGrid}>
                      <PromptField label="Redirect URI" value={browserPrompt?.redirect_uri ?? ""} />
                      <PromptField label="Flow" value="Interactive browser OAuth" />
                    </div>

                    <div style={styles.promptActions}>
                      <button type="button" style={styles.secondaryButton} onClick={onOpenBrowserAgain}>
                        <IconExternal size={12} />
                        <span>Open browser again</span>
                      </button>
                    </div>
                  </>
                ) : (
                  <>
                    <div style={styles.promptCodeRow}>
                      <span style={styles.promptCode}>{devicePrompt?.user_code}</span>
                      <button type="button" style={styles.secondaryButton} onClick={onCopyUserCode}>
                        <IconCopy size={12} />
                        <span>{copiedCode ? "Copied" : "Copy code"}</span>
                      </button>
                    </div>

                    <div style={styles.promptGrid}>
                      <PromptField label="Verification URL" value={devicePrompt?.verification_uri ?? ""} />
                      <PromptField label="Expires in" value={`${devicePrompt?.expires_in_seconds ?? 0}s`} />
                    </div>

                    <div style={styles.promptMessage}>
                      Your browser should already be open. Finish Microsoft OAuth device sign-in there, then Arkived will load your Azure subscriptions and storage accounts.
                    </div>

                    <div style={styles.promptActions}>
                      <button
                        type="button"
                        style={styles.secondaryButton}
                        onClick={() => window.open(devicePrompt?.verification_uri, "_blank", "noopener,noreferrer")}
                      >
                        <IconExternal size={12} />
                        <span>Open verification page</span>
                      </button>
                    </div>
                  </>
                )}
              </div>
            ) : (
              <>
                {(method === "connection-string" || method === "account-key" || method === "sas") && (
                  <FormField
                    label="Display name"
                    placeholder="Friendly label in the explorer"
                    value={form.displayName}
                    onChange={(value) => onFormChange("displayName", value)}
                  />
                )}

                {(method === "entra-browser" || method === "entra-device-code") && (
                  <>
                    <div style={styles.infoCard}>
                      <div style={styles.infoTitle}>
                        <IconAzure size={14} />
                        <span>{method === "entra-browser" ? "Storage Explorer-style sign-in" : "Device-code fallback"}</span>
                      </div>
                      <div style={styles.infoText}>
                        {method === "entra-browser"
                          ? "Arkived opens Microsoft OAuth in your browser, you pick an account there, and the ARM discovery tree refreshes after sign-in. Tenant discovery happens after login, not before."
                          : "Use this when interactive browser OAuth is blocked. Arkived opens the Microsoft verification page, you enter the code, and the ARM discovery tree loads after approval."}
                      </div>
                    </div>
                    {showTenantAdvanced && (
                      <div style={styles.promptActions}>
                        <button
                          type="button"
                          style={styles.secondaryButton}
                          onClick={() => setAdvancedTenantOpen((current) => !current)}
                        >
                          <IconSettings size={12} />
                          <span>{advancedTenantOpen ? "Hide advanced tenant targeting" : "Advanced tenant targeting"}</span>
                        </button>
                      </div>
                    )}
                    {advancedTenantOpen && (
                      <>
                        <div style={styles.field}>
                          <span style={styles.fieldLabel}>Tenant target</span>
                          <div style={styles.tenantModeGroup}>
                            <button
                              type="button"
                              style={{
                                ...styles.tenantModeButton,
                                ...(form.tenantMode === "all" ? styles.tenantModeButtonActive : {}),
                              }}
                              onClick={() => onFormChange("tenantMode", "all")}
                            >
                              <span style={styles.tenantModeLabel}>All tenants</span>
                              <span style={styles.tenantModeText}>Use `common` and discover whatever this account can see.</span>
                            </button>
                            <button
                              type="button"
                              style={{
                                ...styles.tenantModeButton,
                                ...(form.tenantMode === "organizations" ? styles.tenantModeButtonActive : {}),
                              }}
                              onClick={() => onFormChange("tenantMode", "organizations")}
                            >
                              <span style={styles.tenantModeLabel}>Organizations</span>
                              <span style={styles.tenantModeText}>Match Azure Storage Explorer’s work and school tenant sign-in path.</span>
                            </button>
                            <button
                              type="button"
                              style={{
                                ...styles.tenantModeButton,
                                ...(form.tenantMode === "specific" ? styles.tenantModeButtonActive : {}),
                              }}
                              onClick={() => onFormChange("tenantMode", "specific")}
                            >
                              <span style={styles.tenantModeLabel}>Specific tenant</span>
                              <span style={styles.tenantModeText}>Only use this when you need to force a known tenant ID or verified domain.</span>
                            </button>
                          </div>
                        </div>
                        {form.tenantMode === "specific" && (
                          <FormField
                            label="Tenant ID or domain"
                            placeholder="contoso.onmicrosoft.com"
                            value={form.tenant}
                            onChange={(value) => onFormChange("tenant", value)}
                            mono
                          />
                        )}
                      </>
                    )}
                  </>
                )}

                {method === "connection-string" && (
                  <FormField
                    label="Connection string"
                    placeholder="DefaultEndpointsProtocol=https;AccountName=..."
                    value={form.connectionString}
                    onChange={(value) => onFormChange("connectionString", value)}
                    multiline
                    mono
                  />
                )}

                {method === "account-key" && (
                  <>
                    <FormField
                      label="Storage account name"
                      placeholder="mystorageaccount"
                      value={form.accountName}
                      onChange={(value) => onFormChange("accountName", value)}
                      mono
                    />
                    <FormField
                      label="Account key"
                      placeholder="Base64-encoded shared key"
                      value={form.accountKey}
                      onChange={(value) => onFormChange("accountKey", value)}
                      multiline
                      mono
                    />
                    <FormField
                      label="Blob endpoint (optional)"
                      placeholder="https://mystorageaccount.blob.core.windows.net"
                      value={form.endpoint}
                      onChange={(value) => onFormChange("endpoint", value)}
                      mono
                    />
                  </>
                )}

                {method === "sas" && (
                  <>
                    <FormField
                      label="Blob endpoint"
                      placeholder="https://mystorageaccount.blob.core.windows.net"
                      value={form.endpoint}
                      onChange={(value) => onFormChange("endpoint", value)}
                      mono
                    />
                    <FormField
                      label="SAS token"
                      placeholder="?sv=..."
                      value={form.sas}
                      onChange={(value) => onFormChange("sas", value)}
                      multiline
                      mono
                    />
                    <FormField
                      label="Fixed container (optional)"
                      placeholder="Leave blank for account-level browsing"
                      value={form.fixedContainer}
                      onChange={(value) => onFormChange("fixedContainer", value)}
                      mono
                    />
                  </>
                )}

                {method === "azurite" && (
                  <div style={styles.azuriteCard}>
                    <div style={styles.azuriteTitle}>
                      <IconTerminal size={14} />
                      <span>Default Azurite endpoint</span>
                    </div>
                    <div style={styles.azuriteValue}>http://127.0.0.1:10000/devstoreaccount1</div>
                    <div style={styles.azuriteText}>
                      The backend will validate the emulator by listing containers from the local Azurite blob service.
                    </div>
                  </div>
                )}
              </>
            )}
          </div>

          <div style={styles.dialogFooter}>
            <button type="button" style={styles.secondaryButton} onClick={onClose}>
              Cancel
            </button>
            {!showPrompt && (
              <button type="submit" style={styles.primaryButton} disabled={busy || !runtimeAvailable}>
                {busy ? <IconLoader size={12} /> : <IconSparkle size={12} />}
                <span>{submitLabel(method)}</span>
              </button>
            )}
          </div>
        </form>
      </div>
    </div>
  );
}

interface AccountFilterDialogProps {
  signIn: BrowserSignIn;
  tenants: BrowserTenant[];
  busy: boolean;
  reauthBusy: boolean;
  activeTenantId: string | null;
  queuedTenantCount: number;
  onClose: () => void;
  onApply: (tenants: BrowserTenant[]) => void | Promise<void>;
  onReauthenticateTenant: (tenantId: string) => void | Promise<void>;
  onReauthenticateAll: () => void | Promise<void>;
  onOpenBrowserAgain: () => void;
}

function AccountFilterDialog({
  signIn,
  tenants,
  busy,
  reauthBusy,
  activeTenantId,
  queuedTenantCount,
  onClose,
  onApply,
  onReauthenticateTenant,
  onReauthenticateAll,
  onOpenBrowserAgain,
}: AccountFilterDialogProps) {
  const [draft, setDraft] = useState<BrowserTenant[]>(() => cloneTenantSnapshot(tenants));
  const blockedTenantCount = draft.filter((tenant) => tenant.needs_reauth).length;
  const activeTenant = activeTenantId
    ? draft.find((tenant) => tenant.id === activeTenantId) ?? null
    : null;
  const dialogBusy = busy || reauthBusy;

  useEffect(() => {
    setDraft(cloneTenantSnapshot(tenants));
  }, [signIn.id, tenants]);

  function updateTenant(tenantId: string, nextSelected: boolean) {
    setDraft((current) =>
      current.map((tenant) =>
        tenant.id === tenantId
          ? {
              ...tenant,
              selected: nextSelected && canSelectTenant(tenant),
              subscriptions: tenant.subscriptions.map((subscription) => ({
                ...subscription,
                selected: nextSelected && canSelectTenant(tenant),
              })),
            }
          : tenant,
      ),
    );
  }

  function updateSubscription(tenantId: string, subscriptionId: string, nextSelected: boolean) {
    setDraft((current) =>
      current.map((tenant) => {
        if (tenant.id !== tenantId) {
          return tenant;
        }

        const subscriptions = tenant.subscriptions.map((subscription) =>
          subscription.id === subscriptionId ? { ...subscription, selected: nextSelected } : subscription,
        );
        return {
          ...tenant,
          selected: subscriptions.some((subscription) => subscription.selected),
          subscriptions,
        };
      }),
    );
  }

  function setAll(selected: boolean) {
    setDraft((current) =>
      current.map((tenant) => ({
        ...tenant,
        selected: selected && canSelectTenant(tenant),
        subscriptions: tenant.subscriptions.map((subscription) => ({
          ...subscription,
          selected: selected && canSelectTenant(tenant),
        })),
      })),
    );
  }

  return (
    <div style={styles.overlay}>
      <div style={{ ...styles.dialog, width: "min(980px, 100%)", minHeight: 580 }}>
        <div style={styles.dialogSidebar}>
          <div style={styles.dialogSidebarTitle}>Account management</div>
          <div style={styles.dialogSidebarBody}>
            <div style={styles.manageAccountCard}>
              <div style={styles.manageAccountTitle}>{signIn.display_name}</div>
              <div style={styles.manageAccountText}>
                {signIn.selected_tenant_count}/{signIn.tenant_count} tenants selected
              </div>
              <div style={styles.manageAccountText}>
                {signIn.selected_subscription_count}/{signIn.subscription_count} subscriptions visible
              </div>
            </div>
            {blockedTenantCount > 0 && (
              <div style={styles.infoCard}>
                <div style={styles.infoTitle}>
                  {reauthBusy ? <IconLoader size={12} /> : <IconUser size={12} />}
                  <span>
                    {reauthBusy
                      ? `Authenticating ${activeTenant?.display_name ?? "tenant"}`
                      : `${blockedTenantCount} tenant${blockedTenantCount === 1 ? "" : "s"} need reauthentication`}
                  </span>
                </div>
                <div style={styles.infoText}>
                  {reauthBusy
                    ? queuedTenantCount > 0
                      ? `${queuedTenantCount} blocked tenant${queuedTenantCount === 1 ? "" : "s"} remain in the queue. Finish the browser sign-in for the current tenant, then Arkived will open the next one.`
                      : "Finish the current browser sign-in and Arkived will refresh this account automatically."
                    : "Microsoft may require a separate browser confirmation for each blocked tenant. Arkived can walk them for you without creating separate account entries."}
                </div>
                <div style={styles.promptActions}>
                  <button
                    type="button"
                    style={styles.secondaryButton}
                    onClick={() => onReauthenticateAll()}
                    disabled={dialogBusy}
                  >
                    {reauthBusy ? <IconLoader size={12} /> : <IconUser size={12} />}
                    <span>
                      {reauthBusy
                        ? "Authenticating blocked tenants…"
                        : `Authenticate blocked tenant${blockedTenantCount === 1 ? "" : "s"}`}
                    </span>
                  </button>
                  {reauthBusy && (
                    <button type="button" style={styles.secondaryButton} onClick={onOpenBrowserAgain}>
                      <IconExternal size={12} />
                      <span>Open browser again</span>
                    </button>
                  )}
                </div>
              </div>
            )}
            <button type="button" style={styles.secondaryButton} onClick={() => setAll(true)} disabled={dialogBusy}>
              Select all
            </button>
            <button type="button" style={styles.secondaryButton} onClick={() => setAll(false)} disabled={dialogBusy}>
              Clear all
            </button>
          </div>
        </div>

        <div style={styles.dialogMain}>
          <div style={styles.dialogHeader}>
            <div>
              <div style={styles.dialogEyebrow}>Tenant filters</div>
              <h2 style={styles.dialogTitle}>Choose what this account loads</h2>
              <p style={styles.dialogDescription}>
                Arkived auto-discovers tenants and subscriptions after sign-in. Use this screen to keep only the tenants and subscriptions you want visible in the explorer.
              </p>
            </div>
            <button type="button" style={styles.closeButton} onClick={onClose}>
              Close
            </button>
          </div>

          <div style={styles.dialogContent}>
            {draft.map((tenant) => (
              <div key={tenant.id} style={styles.tenantCard}>
                <label style={styles.tenantCardHeader}>
                  <input
                    type="checkbox"
                    checked={tenant.selected}
                    onChange={(event) => updateTenant(tenant.id, event.target.checked)}
                    disabled={dialogBusy || !canSelectTenant(tenant)}
                  />
                  <div style={styles.tenantCardCopy}>
                    <div style={styles.tenantCardTitle}>{tenant.display_name}</div>
                    <div style={styles.tenantCardMeta}>
                      {tenant.default_domain ?? tenant.id}
                    </div>
                  </div>
                  <div style={styles.tenantCardCounts}>
                    {tenant.subscriptions.filter((subscription) => subscription.selected).length}/{tenant.subscriptions.length} subscriptions
                  </div>
                </label>

                {tenant.error && <div style={styles.tenantCardError}>{tenant.error}</div>}
                {tenant.needs_reauth && (
                  <div style={styles.promptActions}>
                    <button
                      type="button"
                      style={styles.secondaryButton}
                      onClick={() => onReauthenticateTenant(tenant.id)}
                      disabled={dialogBusy}
                    >
                      {reauthBusy && activeTenantId === tenant.id ? <IconLoader size={12} /> : <IconUser size={12} />}
                      <span>
                        {reauthBusy && activeTenantId === tenant.id
                          ? "Waiting for browser sign-in…"
                          : "Authenticate tenant"}
                      </span>
                    </button>
                    {reauthBusy && activeTenantId === tenant.id && (
                      <button type="button" style={styles.secondaryButton} onClick={onOpenBrowserAgain}>
                        <IconExternal size={12} />
                        <span>Open browser again</span>
                      </button>
                    )}
                  </div>
                )}

                <div style={styles.subscriptionList}>
                  {tenant.subscriptions.map((subscription) => (
                    <label key={subscription.id} style={styles.subscriptionRow}>
                      <input
                        type="checkbox"
                        checked={subscription.selected}
                        onChange={(event) =>
                          updateSubscription(tenant.id, subscription.id, event.target.checked)
                        }
                        disabled={dialogBusy || !canSelectTenant(tenant)}
                      />
                      <span style={styles.subscriptionName}>{subscription.name}</span>
                      <span style={styles.subscriptionMeta}>
                        {subscription.storage_account_count} storage accounts
                      </span>
                    </label>
                  ))}
                  {tenant.subscriptions.length === 0 && !tenant.error && (
                    <div style={styles.subscriptionEmpty}>No subscriptions were discovered for this tenant.</div>
                  )}
                </div>
              </div>
            ))}
          </div>

          <div style={styles.dialogFooter}>
            <button type="button" style={styles.secondaryButton} onClick={onClose}>
              Cancel
            </button>
            <button type="button" style={styles.primaryButton} onClick={() => onApply(draft)} disabled={dialogBusy}>
              {dialogBusy ? <IconLoader size={12} /> : <IconSparkle size={12} />}
              <span>{dialogBusy ? "Working…" : "Apply filters"}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

interface FormFieldProps {
  label: string;
  placeholder?: string;
  value: string;
  onChange: (value: string) => void;
  multiline?: boolean;
  mono?: boolean;
}

function FormField({ label, placeholder, value, onChange, multiline = false, mono = false }: FormFieldProps) {
  return (
    <label style={styles.field}>
      <span style={styles.fieldLabel}>{label}</span>
      {multiline ? (
        <textarea
          style={{ ...styles.fieldInput, ...styles.fieldTextarea, fontFamily: mono ? "var(--mono)" : "var(--sans)" }}
          placeholder={placeholder}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          rows={4}
        />
      ) : (
        <input
          style={{ ...styles.fieldInput, fontFamily: mono ? "var(--mono)" : "var(--sans)" }}
          placeholder={placeholder}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
    </label>
  );
}

interface PromptFieldProps {
  label: string;
  value: string;
}

function PromptField({ label, value }: PromptFieldProps) {
  return (
    <div style={styles.promptField}>
      <div style={styles.promptFieldLabel}>{label}</div>
      <div style={styles.promptFieldValue}>{value}</div>
    </div>
  );
}

interface BannerProps {
  tone: "warn" | "error";
  icon: ReactNode;
  text: string;
}

function Banner({ tone, icon, text }: BannerProps) {
  return (
    <div
      style={{
        ...styles.banner,
        ...(tone === "error" ? styles.bannerError : styles.bannerWarn),
      }}
    >
      <span style={styles.bannerIcon}>{icon}</span>
      <span>{text}</span>
    </div>
  );
}

interface ToolbarButtonProps {
  label: string;
  icon: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  tone?: "default" | "danger";
}

function ToolbarButton({ label, icon, onClick, disabled = false, tone = "default" }: ToolbarButtonProps) {
  return (
    <button
      type="button"
      style={{
        ...styles.toolbarButton,
        ...(tone === "danger" ? styles.toolbarButtonDanger : {}),
        ...(disabled ? styles.toolbarButtonDisabled : {}),
      }}
      onClick={onClick}
      disabled={disabled}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

interface IconButtonProps {
  title: string;
  onClick: () => void;
  icon: ReactNode;
  disabled?: boolean;
}

function IconButton({ title, onClick, icon, disabled = false }: IconButtonProps) {
  return (
    <button
      type="button"
      title={title}
      style={{
        ...styles.iconButton,
        ...(disabled ? styles.iconButtonDisabled : {}),
      }}
      onClick={onClick}
      disabled={disabled}
    >
      {icon}
    </button>
  );
}

interface MainEmptyStateProps {
  title: string;
  body: string;
  primaryLabel?: string;
  onPrimary?: () => void;
  secondaryLabel?: string;
}

function MainEmptyState({ title, body, primaryLabel, onPrimary, secondaryLabel }: MainEmptyStateProps) {
  return (
    <div style={styles.mainEmptyWrap}>
      <div style={styles.mainEmptyCard}>
        <div style={styles.mainEmptyIcon}>
          <IconFolderOpen size={18} />
        </div>
        <h2 style={styles.mainEmptyTitle}>{title}</h2>
        <p style={styles.mainEmptyBody}>{body}</p>
        {primaryLabel && onPrimary && (
          <button type="button" style={styles.primaryButton} onClick={onPrimary}>
            <IconPlus size={12} />
            <span>{primaryLabel}</span>
          </button>
        )}
        {secondaryLabel && <div style={styles.mainEmptySecondary}>{secondaryLabel}</div>}
      </div>
    </div>
  );
}

interface BlobPreviewPaneProps {
  state: PreviewDialogState;
  onClose: () => void;
  onPage: (rowOffset: number, rowLimit: number) => void;
  onOpenContextMenu: (event: React.MouseEvent<HTMLDivElement>, items: ContextMenuItem[]) => void;
  onCopyRows: (columns: string[], rows: string[][]) => void;
  onDownload: () => void;
  onOpenExternal: () => void;
}

function BlobPreviewPane({
  state,
  onClose,
  onPage,
  onOpenContextMenu,
  onCopyRows,
  onDownload,
  onOpenExternal,
}: BlobPreviewPaneProps) {
  const result = state.result;
  const columns = result ? previewColumns(result) : [];
  const [columnWidthsByKey, setColumnWidthsByKey] = useState<Record<string, number[]>>({});
  const [selectedRowsByKey, setSelectedRowsByKey] = useState<Record<string, number[]>>({});
  const [selectionAnchorByKey, setSelectionAnchorByKey] = useState<Record<string, number>>({});
  const tableKey = result ? `${result.kind}\u001f${result.path}\u001f${columns.join("\u001f")}` : "";
  const rowLimit = result?.row_limit || state.rowLimit || 100;
  const tablePageKey = result ? `${tableKey}\u001f${result.row_offset}\u001f${rowLimit}` : "";
  const columnWidths =
    result && columns.length > 0
      ? previewTableColumnWidths(tableKey, columns, result.rows, columnWidthsByKey)
      : [];
  const canPage = Boolean(result && columns.length > 0);
  const selectedRowIndices = new Set(tablePageKey ? selectedRowsByKey[tablePageKey] ?? [] : []);
  const currentStart = result && result.rows.length > 0 ? result.row_offset + 1 : 0;
  const currentEnd = result ? result.row_offset + result.rows.length : 0;
  const totalRowsLabel = result?.total_rows != null ? formatNumber(result.total_rows) : "sample";
  const selectionLabel = selectedRowIndices.size === 1 ? "1 item selected" : `${formatNumber(selectedRowIndices.size)} items selected`;
  const lastOffset =
    result?.total_rows != null
      ? Math.max(0, Math.floor(Math.max(0, result.total_rows - 1) / rowLimit) * rowLimit)
      : null;
  const currentPage = result ? Math.floor(result.row_offset / rowLimit) + 1 : 1;
  const pageCount = lastOffset != null ? Math.floor(lastOffset / rowLimit) + 1 : null;
  const tableGridTemplate = previewTableGrid(columnWidths);

  const handleColumnResizeStart = (event: React.MouseEvent<HTMLSpanElement>, columnIndex: number) => {
    if (!result || !tableKey) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startWidth = columnWidths[columnIndex] ?? PREVIEW_COLUMN_MIN_WIDTH;
    const previousCursor = document.body.style.cursor;
    const previousUserSelect = document.body.style.userSelect;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    const applyWidth = (clientX: number) => {
      const nextWidth = clampPreviewColumnWidth(startWidth + clientX - startX);
      setColumnWidthsByKey((current) => {
        const base = current[tableKey] ?? columnWidths;
        const next = [...base];
        next[columnIndex] = nextWidth;
        return { ...current, [tableKey]: next };
      });
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

  const setSelectedPreviewRows = (indices: number[]) => {
    if (!tablePageKey) {
      return;
    }

    const normalized = Array.from(new Set(indices))
      .filter((index) => result && index >= 0 && index < result.rows.length)
      .sort((a, b) => a - b);
    setSelectedRowsByKey((current) => ({ ...current, [tablePageKey]: normalized }));
  };

  const handlePreviewRowClick = (event: React.MouseEvent<HTMLDivElement>, rowIndex: number) => {
    if (!result || !tablePageKey) {
      return;
    }

    if (event.shiftKey) {
      const anchor = selectionAnchorByKey[tablePageKey] ?? rowIndex;
      const start = Math.min(anchor, rowIndex);
      const end = Math.max(anchor, rowIndex);
      setSelectedPreviewRows(Array.from({ length: end - start + 1 }, (_, offset) => start + offset));
      return;
    }

    if (event.ctrlKey || event.metaKey) {
      const next = new Set(selectedRowIndices);
      if (next.has(rowIndex)) {
        next.delete(rowIndex);
      } else {
        next.add(rowIndex);
      }
      setSelectedPreviewRows(Array.from(next));
    } else {
      setSelectedPreviewRows([rowIndex]);
    }

    setSelectionAnchorByKey((current) => ({ ...current, [tablePageKey]: rowIndex }));
  };

  const handlePreviewRowContextMenu = (event: React.MouseEvent<HTMLDivElement>, rowIndex: number) => {
    if (!result || !tablePageKey) {
      return;
    }

    const copyIndices =
      selectedRowIndices.has(rowIndex) && selectedRowIndices.size > 0
        ? Array.from(selectedRowIndices).sort((a, b) => a - b)
        : [rowIndex];
    if (!selectedRowIndices.has(rowIndex)) {
      setSelectedPreviewRows([rowIndex]);
      setSelectionAnchorByKey((current) => ({ ...current, [tablePageKey]: rowIndex }));
    }

    const rowsToCopy = copyIndices
      .map((index) => result.rows[index])
      .filter((row): row is string[] => Boolean(row));
    onOpenContextMenu(event, [
      {
        label: "Copy Rows",
        hint: `${rowsToCopy.length}`,
        disabled: rowsToCopy.length === 0,
        action: () => onCopyRows(columns, rowsToCopy),
      },
    ]);
  };

  return (
    <aside style={styles.previewPane}>
      <div style={styles.previewHeader}>
        <div style={styles.previewIcon}>
          {state.busy ? <IconLoader size={14} /> : <IconEye size={14} />}
        </div>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={styles.previewTitle}>Preview of '{result?.title ?? state.row.name}'</div>
          <div style={styles.previewPath}>{result?.path ?? state.row.path}</div>
        </div>
        <button type="button" style={styles.previewHeaderButton} onClick={onDownload} disabled={state.busy}>
          <IconDownload size={11} />
          <span>Download</span>
        </button>
        <button type="button" style={styles.previewHeaderButton} onClick={onOpenExternal} disabled={state.busy}>
          <IconExternal size={11} />
          <span>Open</span>
        </button>
        <button type="button" style={styles.previewCloseButton} onClick={onClose}>
          <IconX size={12} />
        </button>
      </div>

      <div style={styles.previewBody}>
        {state.busy ? (
          <div style={styles.previewCentered}>
            <IconLoader size={18} />
            <span>Reading live blob data…</span>
          </div>
        ) : state.error ? (
          <div style={styles.previewError}>
            <IconAlert size={16} />
            <div>
              <div style={styles.previewErrorTitle}>Preview failed</div>
              <div style={styles.previewErrorBody}>{state.error}</div>
            </div>
          </div>
        ) : result ? (
          <>
            <div style={styles.previewMetaStrip}>
              <span>{previewKindLabel(result.kind)}</span>
              <span>
                {currentStart > 0
                  ? `${formatNumber(currentStart)}-${formatNumber(currentEnd)} of ${totalRowsLabel}`
                  : "0 rows"}
              </span>
              <span>{formatNumber(result.columns.length)} columns</span>
              <span>{formatBytesLabel(result.byte_count)}</span>
              {result.truncated && <span>partial file</span>}
            </div>

            {result.warning && (
              <div style={styles.previewWarning}>
                <IconAlert size={12} />
                <span>{result.warning}</span>
              </div>
            )}

            {result.image_data_url ? (
              <div style={styles.previewImageWrap}>
                <img src={result.image_data_url} alt={result.title} style={styles.previewImage} />
              </div>
            ) : columns.length > 0 ? (
              <div style={styles.previewTableWrap}>
                {result.rows.length === 0 ? (
                  <div style={styles.previewEmpty}>No rows were available in the preview sample.</div>
                ) : (
                  <>
                    <div style={styles.previewTableScroll}>
                      <div style={{ ...styles.previewTableGrid, gridTemplateColumns: tableGridTemplate }}>
                        {columns.map((column, index) => (
                          <div key={`${column}-${index}`} style={styles.previewTableCellHeader}>
                            <span style={styles.previewTableCellText}>{column || `column ${index + 1}`}</span>
                            <span
                              aria-hidden="true"
                              title="Drag to resize column"
                              style={styles.previewColumnResizeHandle}
                              onMouseDown={(event) => handleColumnResizeStart(event, index)}
                            />
                          </div>
                        ))}
                        {result.rows.map((row, rowIndex) =>
                          columns.map((_, columnIndex) => {
                            const isSelected = selectedRowIndices.has(rowIndex);
                            return (
                              <div
                                key={`${rowIndex}-${columnIndex}`}
                                style={{
                                  ...styles.previewTableCell,
                                  ...(isSelected ? styles.previewTableCellSelected : {}),
                                }}
                                title={row[columnIndex] ?? ""}
                                onClick={(event) => handlePreviewRowClick(event, rowIndex)}
                                onContextMenu={(event) => handlePreviewRowContextMenu(event, rowIndex)}
                              >
                                {row[columnIndex] ?? ""}
                              </div>
                            );
                          }),
                        )}
                      </div>
                    </div>
                    <div style={styles.previewTableFooter}>
                      <span style={styles.previewTableFooterStatus}>
                        {currentStart > 0
                          ? `Showing ${formatNumber(currentStart)} to ${formatNumber(currentEnd)} of ${totalRowsLabel} items (${selectionLabel})`
                          : `Showing 0 items (${selectionLabel})`}
                      </span>
                      {canPage && (
                        <span style={styles.previewPager}>
                          <button
                            type="button"
                            style={styles.previewPagerButton}
                            disabled={state.busy || !result.has_previous_page}
                            onClick={() => onPage(0, rowLimit)}
                          >
                            First
                          </button>
                          <button
                            type="button"
                            style={styles.previewPagerButton}
                            disabled={state.busy || !result.has_previous_page}
                            onClick={() => onPage(Math.max(0, result.row_offset - rowLimit), rowLimit)}
                          >
                            Prev
                          </button>
                          {pageCount != null && pageCount <= 500 ? (
                            <select
                              style={styles.previewPageSizeSelect}
                              value={result.row_offset}
                              disabled={state.busy}
                              title="Page"
                              onChange={(event) => onPage(Number(event.currentTarget.value) || 0, rowLimit)}
                            >
                              {Array.from({ length: pageCount }, (_, index) => {
                                const offset = index * rowLimit;
                                return (
                                  <option key={offset} value={offset}>
                                    {index + 1}
                                  </option>
                                );
                              })}
                            </select>
                          ) : (
                            <span style={styles.previewPageLabel}>
                              Page {formatNumber(currentPage)}
                              {pageCount != null ? ` / ${formatNumber(pageCount)}` : ""}
                            </span>
                          )}
                          <button
                            type="button"
                            style={styles.previewPagerButton}
                            disabled={state.busy || !result.has_next_page}
                            onClick={() => onPage(result.row_offset + rowLimit, rowLimit)}
                          >
                            Next
                          </button>
                          <button
                            type="button"
                            style={styles.previewPagerButton}
                            disabled={state.busy || lastOffset == null || !result.has_next_page}
                            onClick={() => {
                              if (lastOffset != null) {
                                onPage(lastOffset, rowLimit);
                              }
                            }}
                          >
                            Last
                          </button>
                          <select
                            style={styles.previewPageSizeSelect}
                            value={rowLimit}
                            disabled={state.busy}
                            title="Rows per page"
                            onChange={(event) => {
                              const nextLimit = Number(event.currentTarget.value) || PREVIEW_DEFAULT_ROW_LIMIT;
                              onPage(0, nextLimit);
                            }}
                          >
                            {PREVIEW_PAGE_SIZE_OPTIONS.map((option) => (
                              <option key={option} value={option}>
                                {option}/page
                              </option>
                            ))}
                          </select>
                          <button
                            type="button"
                            style={styles.previewPagerButton}
                            disabled={state.busy || !result.has_next_page}
                            onClick={() => onPage(result.row_offset + rowLimit, rowLimit)}
                          >
                            Load more
                          </button>
                        </span>
                      )}
                    </div>
                  </>
                )}
              </div>
            ) : result.text != null ? (
              <pre style={styles.previewText}>{result.text}</pre>
            ) : (
              <div style={styles.previewEmpty}>
                This blob format does not have an inline preview yet. Download or open it externally.
              </div>
            )}
          </>
        ) : null}
      </div>
    </aside>
  );
}

function previewColumns(result: BlobPreviewResult): string[] {
  if (result.columns.length > 0) {
    return result.columns;
  }

  const width = result.rows.reduce((max, row) => Math.max(max, row.length), 0);
  return Array.from({ length: width }, (_, index) => `column ${index + 1}`);
}

function previewKindLabel(kind: BlobPreviewResult["kind"]): string {
  switch (kind) {
    case "parquet":
      return "Parquet table";
    case "table":
      return "Delimited table";
    case "json":
      return "JSON";
    case "image":
      return "Image";
    case "binary":
      return "Binary";
    default:
      return "Text";
  }
}

function previewTableColumnWidths(
  tableKey: string,
  columns: string[],
  rows: string[][],
  overrides: Record<string, number[]>,
): number[] {
  const saved = tableKey ? overrides[tableKey] : undefined;
  return previewTableDefaultColumnWidths(columns, rows).map((width, index) =>
    saved?.[index] != null ? clampPreviewColumnWidth(saved[index]) : width,
  );
}

function previewTableDefaultColumnWidths(columns: string[], rows: string[][]): number[] {
  return columns.map((column, columnIndex) => {
    const longest = rows.reduce(
      (max, row) => Math.max(max, (row[columnIndex] ?? "").length),
      column.length,
    );
    const ch = Math.min(48, Math.max(12, longest + 2));
    return clampPreviewColumnWidth(Math.round(ch * 7.2 + 24));
  });
}

function previewTableGrid(widths: number[]): string {
  return widths.length > 0 ? widths.map((width) => `${clampPreviewColumnWidth(width)}px`).join(" ") : "96px";
}

function clampPreviewColumnWidth(width: number): number {
  if (!Number.isFinite(width)) {
    return PREVIEW_COLUMN_MIN_WIDTH;
  }
  return Math.min(PREVIEW_COLUMN_MAX_WIDTH, Math.max(PREVIEW_COLUMN_MIN_WIDTH, Math.round(width)));
}

function clampNumber(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }
  return Math.min(max, Math.max(min, Math.round(value)));
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(value);
}

function formatBytesLabel(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

interface EmptySidebarStateProps {
  title: string;
  detail: string;
  actionLabel: string;
  onAction: () => void;
}

function EmptySidebarState({ title, detail, actionLabel, onAction }: EmptySidebarStateProps) {
  return (
    <div style={styles.sidebarEmptyCard}>
      <div style={styles.sidebarEmptyTitle}>{title}</div>
      <div style={styles.sidebarEmptyBody}>{detail}</div>
      <button type="button" style={styles.sidebarEmptyAction} onClick={onAction}>
        {actionLabel}
      </button>
    </div>
  );
}

function cloneTenantSnapshot(tenants: BrowserTenant[]): BrowserTenant[] {
  return tenants.map((tenant) => ({
    ...tenant,
    subscriptions: tenant.subscriptions.map((subscription) => ({ ...subscription })),
  }));
}

function flattenSelectedSubscriptions(tenants: BrowserTenant[]): BrowserSubscription[] {
  return tenants.flatMap((tenant) =>
    tenant.selected
      ? tenant.subscriptions.filter((subscription) => subscription.selected)
      : [],
  );
}

function canSelectTenant(tenant: BrowserTenant): boolean {
  return !tenant.needs_reauth && !tenant.error && tenant.subscriptions.length > 0;
}

function connectionIcon(kind: string): ReactNode {
  switch (kind) {
    case "account-key":
      return <IconKey size={11} />;
    case "sas":
      return <IconLock size={11} />;
    case "azurite":
      return <IconTerminal size={11} />;
    case "entra-managed-key":
      return <IconKey size={11} />;
    case "entra":
    case "entra-browser":
    case "entra-interactive":
    case "entra-device-code":
      return <IconUser size={11} />;
    default:
      return <IconAzure size={11} />;
  }
}

function authLabel(kind: string): string {
  switch (kind) {
    case "connection-string":
      return "Connection string";
    case "account-key":
      return "Account key";
    case "sas":
      return "SAS";
    case "azurite":
      return "Azurite";
    case "entra":
    case "entra-browser":
    case "entra-interactive":
      return "Azure AD";
    case "entra-managed-key":
      return "Managed key";
    case "entra-device-code":
      return "Device code";
    default:
      return kind;
  }
}

function compactAuthLabel(kind: string): string {
  switch (kind) {
    case "connection-string":
      return "conn";
    case "account-key":
      return "key";
    case "sas":
      return "sas";
    case "azurite":
      return "emu";
    case "entra":
    case "entra-browser":
    case "entra-interactive":
      return "aad";
    case "entra-managed-key":
      return "key";
    case "entra-device-code":
      return "code";
    default:
      return kind;
  }
}

function isDiscoveredAzureAuth(kind: string): boolean {
  return (
    kind === "entra" ||
    kind === "entra-interactive" ||
    kind === "entra-managed-key" ||
    kind === "entra-device-code"
  );
}

function isDiscoveredConnection(connection: BrowserConnection): boolean {
  return Boolean(connection.origin_sign_in_id) || isDiscoveredAzureAuth(connection.auth_kind);
}

function resolveAzureTenant(form: ConnectionFormState): string | undefined {
  switch (form.tenantMode) {
    case "all":
      return "common";
    case "organizations":
      return "organizations";
    case "specific": {
      const trimmed = form.tenant.trim();
      return trimmed.length > 0 ? trimmed : undefined;
    }
    default:
      return undefined;
  }
}

function compactHost(endpoint: string): string {
  try {
    return new URL(endpoint).host;
  } catch {
    return endpoint;
  }
}

function normalizeUrlHost(endpoint: string): string {
  try {
    return new URL(endpoint).toString().replace(/\/+$/, "");
  } catch {
    return endpoint.replace(/\/+$/, "");
  }
}

function ensureTrailingSlash(value: string): string {
  return value.endsWith("/") ? value : `${value}/`;
}

function parentPrefix(currentPrefix: string): string {
  const trimmed = currentPrefix.replace(/\/+$/, "");
  if (!trimmed) {
    return "";
  }

  const parts = trimmed.split("/");
  parts.pop();
  return parts.length === 0 ? "" : `${parts.join("/")}/`;
}

function parentPathPrefix(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  if (!trimmed || !trimmed.includes("/")) {
    return "";
  }

  const parts = trimmed.split("/");
  parts.pop();
  return parts.length === 0 ? "" : `${parts.join("/")}/`;
}

function splitPrefix(currentPrefix: string): Array<{ label: string; value: string }> {
  const trimmed = currentPrefix.replace(/\/+$/, "");
  if (!trimmed) {
    return [];
  }

  const parts = trimmed.split("/");
  return parts.map((part, index) => ({
    label: part,
    value: `${parts.slice(0, index + 1).join("/")}/`,
  }));
}

function buildResourceUrl(endpoint: string, container: string, path: string): string {
  const base = endpoint.replace(/\/+$/, "");
  const encodedContainer = encodeURIComponent(container);
  const encodedPath = path
    .split("/")
    .filter((segment) => segment.length > 0)
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  return encodedPath ? `${base}/${encodedContainer}/${encodedPath}` : `${base}/${encodedContainer}`;
}

function submitLabel(method: ConnectMethod): string {
  switch (method) {
    case "entra-browser":
      return "Open browser sign-in";
    case "entra-device-code":
      return "Start device code";
    case "azurite":
      return "Attach emulator";
    default:
      return "Connect";
  }
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "An unexpected error occurred.";
}

function isTauriRuntimeAvailable(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  return Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
}

function loadPersistedShellSnapshot(): PersistedShellSnapshot | null {
  if (typeof window === "undefined") {
    return null;
  }

  try {
    const raw = window.localStorage.getItem(SHELL_STATE_STORAGE_KEY);
    if (!raw) {
      return null;
    }

    const parsed = JSON.parse(raw) as Partial<PersistedShellSnapshot>;
    if (parsed.version !== 1 || !Array.isArray(parsed.tabs)) {
      return null;
    }

    return {
      version: 1,
      activeTabId: typeof parsed.activeTabId === "string" ? parsed.activeTabId : null,
      activeConnectionId:
        typeof parsed.activeConnectionId === "string" ? parsed.activeConnectionId : null,
      previewPaneRatio: typeof parsed.previewPaneRatio === "number" ? parsed.previewPaneRatio : undefined,
      sidebarWidth:
        typeof parsed.sidebarWidth === "number"
          ? clampNumber(parsed.sidebarWidth, SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
          : undefined,
      detailPaneWidth:
        typeof parsed.detailPaneWidth === "number"
          ? clampNumber(parsed.detailPaneWidth, DETAIL_PANE_MIN_WIDTH, DETAIL_PANE_MAX_WIDTH)
          : typeof parsed.previewPaneRatio === "number"
            ? clampNumber(DETAIL_PANE_DEFAULT_WIDTH * parsed.previewPaneRatio * 2, DETAIL_PANE_MIN_WIDTH, DETAIL_PANE_MAX_WIDTH)
            : undefined,
      sidebarDetailsHeight:
        typeof parsed.sidebarDetailsHeight === "number"
          ? clampNumber(parsed.sidebarDetailsHeight, SIDEBAR_DETAILS_MIN_HEIGHT, SIDEBAR_DETAILS_MAX_HEIGHT)
          : undefined,
      activityPaneHeight:
        typeof parsed.activityPaneHeight === "number"
          ? clampNumber(parsed.activityPaneHeight, ACTIVITY_PANE_MIN_HEIGHT, ACTIVITY_PANE_MAX_HEIGHT)
          : undefined,
      expandedSignIns: sanitizeBooleanRecord(parsed.expandedSignIns),
      expandedSubscriptions: sanitizeBooleanRecord(parsed.expandedSubscriptions),
      expandedAccounts: sanitizeBooleanRecord(parsed.expandedAccounts),
      tabs: parsed.tabs
        .filter((tab): tab is PersistedBrowserTab => typeof tab?.containerName === "string" && tab.containerName.length > 0)
        .map((tab) => ({
          id: typeof tab.id === "string" ? tab.id : undefined,
          connectionId: typeof tab.connectionId === "string" ? tab.connectionId : null,
          originSignInId: typeof tab.originSignInId === "string" ? tab.originSignInId : null,
          originSubscriptionId:
            typeof tab.originSubscriptionId === "string" ? tab.originSubscriptionId : null,
          accountName: typeof tab.accountName === "string" ? tab.accountName : null,
          endpoint: typeof tab.endpoint === "string" ? tab.endpoint : null,
          containerName: tab.containerName,
          prefix: typeof tab.prefix === "string" ? tab.prefix : "",
          filter: typeof tab.filter === "string" ? tab.filter : "",
        })),
    };
  } catch {
    return null;
  }
}

function writePersistedShellSnapshot(snapshot: PersistedShellSnapshot) {
  if (typeof window === "undefined") {
    return;
  }

  try {
    window.localStorage.setItem(SHELL_STATE_STORAGE_KEY, JSON.stringify(snapshot));
  } catch {
    // Shell state is a convenience. Failure should never block live browsing.
  }
}

function sanitizeBooleanRecord(value: unknown): Record<string, boolean> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }

  const entries = Object.entries(value as Record<string, unknown>).filter(
    (entry): entry is [string, boolean] => entry[0].length > 0 && typeof entry[1] === "boolean",
  );
  return Object.fromEntries(entries);
}

const styles: Record<string, CSSProperties> = {
  appRoot: {
    display: "flex",
    flexDirection: "column",
    height: "100vh",
    background: "linear-gradient(180deg, #08090c 0%, #050507 100%)",
  },
  shell: {
    flex: 1,
    minHeight: 0,
    display: "flex",
    padding: 8,
    background:
      "radial-gradient(circle at 82% 4%, rgba(63,157,246,0.10), transparent 34%), linear-gradient(180deg, rgba(12,13,17,0.98) 0%, rgba(6,7,10,1) 100%)",
  },
  sidebar: {
    minWidth: 0,
    flexShrink: 0,
    display: "flex",
    flexDirection: "column",
    border: "1px solid var(--border-1)",
    borderRadius: 14,
    overflow: "hidden",
    background:
      "linear-gradient(180deg, rgba(17,18,23,0.98) 0%, rgba(10,11,15,0.98) 100%)",
    boxShadow: "0 18px 48px rgba(0,0,0,0.28)",
  },
  shellVerticalResizeHandle: {
    width: PANE_RESIZE_HANDLE_WIDTH,
    flexShrink: 0,
    cursor: "col-resize",
    background:
      "linear-gradient(90deg, transparent 0, transparent 2px, rgba(63,157,246,0.18) 3px, transparent 5px)",
  },
  horizontalPaneResizeHandle: {
    height: PANE_RESIZE_HANDLE_WIDTH,
    flexShrink: 0,
    cursor: "row-resize",
    background:
      "linear-gradient(180deg, transparent 0, transparent 2px, rgba(63,157,246,0.16) 3px, transparent 5px)",
  },
  sidebarHeader: {
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    padding: "14px 14px 10px",
    borderBottom: "1px solid var(--border-0)",
  },
  sidebarEyebrow: {
    fontSize: 10,
    fontFamily: "var(--mono)",
    textTransform: "uppercase",
    letterSpacing: "0.08em",
    color: "var(--fg-3)",
  },
  sidebarTitle: {
    fontSize: 18,
    fontWeight: 600,
    color: "var(--fg-0)",
    marginTop: 2,
  },
  sidebarHeaderActions: {
    display: "flex",
    gap: 6,
  },
  sidebarBody: {
    flex: 1,
    overflow: "auto",
    paddingBottom: 12,
  },
  sidebarFooter: {
    borderTop: "1px solid var(--border-0)",
    padding: "10px 14px 12px",
    display: "flex",
    flexDirection: "column",
    gap: 4,
  },
  sidebarFooterLabel: {
    fontSize: 10,
    fontFamily: "var(--mono)",
    textTransform: "uppercase",
    letterSpacing: "0.08em",
    color: "var(--fg-3)",
  },
  sidebarFooterText: {
    fontSize: 11,
    color: "var(--fg-2)",
    lineHeight: 1.5,
  },
  sidebarDetailsPanel: {
    borderTop: "1px solid var(--border-0)",
    background: "rgba(12, 12, 15, 0.94)",
    display: "flex",
    flexDirection: "column",
    flexShrink: 0,
  },
  sidebarPanelTabs: {
    display: "flex",
    alignItems: "center",
    height: 30,
    borderBottom: "1px solid var(--border-0)",
  },
  sidebarPanelTab: {
    height: 30,
    padding: "0 12px",
    color: "var(--fg-2)",
    fontFamily: "var(--mono)",
    fontSize: 10,
    borderRight: "1px solid var(--border-0)",
  },
  sidebarPanelTabActive: {
    color: "var(--fg-0)",
    background: "var(--bg-1)",
  },
  sidebarPanelBody: {
    padding: "8px 10px",
    display: "flex",
    flexDirection: "column",
    gap: 5,
    overflow: "auto",
  },
  sidebarActionButton: {
    minHeight: 20,
    color: "var(--accent)",
    textAlign: "left",
    fontFamily: "var(--sans)",
    fontSize: 11,
    borderRadius: 3,
    padding: "2px 6px",
  },
  sidebarActionButtonDanger: {
    color: "var(--red)",
  },
  sidebarActionButtonDisabled: {
    color: "var(--fg-4)",
    cursor: "not-allowed",
  },
  sidebarPropertyRow: {
    display: "grid",
    gridTemplateColumns: "86px minmax(0, 1fr)",
    gap: 8,
    minHeight: 18,
    alignItems: "center",
    fontSize: 10,
    fontFamily: "var(--mono)",
  },
  sidebarPropertyLabel: {
    color: "var(--fg-3)",
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    fontWeight: 600,
  },
  sidebarPropertyValue: {
    color: "var(--fg-1)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  sidebarEmptyCard: {
    margin: "8px 12px 0",
    padding: 12,
    borderRadius: 10,
    border: "1px solid var(--border-1)",
    background: "rgba(63, 157, 246, 0.05)",
    display: "flex",
    flexDirection: "column",
    gap: 8,
  },
  sidebarEmptyTitle: {
    fontWeight: 600,
    color: "var(--fg-0)",
  },
  sidebarEmptyBody: {
    fontSize: 11,
    lineHeight: 1.5,
    color: "var(--fg-2)",
  },
  sidebarEmptyAction: {
    alignSelf: "flex-start",
    padding: "6px 10px",
    borderRadius: 6,
    background: "var(--accent)",
    color: "#08111c",
    fontWeight: 600,
  },
  discoveryGroup: {
    marginBottom: 6,
  },
  connectionMeta: {
    marginLeft: 34,
    marginTop: 2,
    fontSize: 10,
    fontFamily: "var(--mono)",
    color: "var(--fg-3)",
  },
  inlineEmptyBlock: {
    margin: "8px 12px 0",
    padding: 10,
    borderRadius: 8,
    background: "var(--bg-2)",
    border: "1px solid var(--border-0)",
    fontSize: 11,
    color: "var(--fg-3)",
    lineHeight: 1.6,
  },
  main: {
    flex: 1,
    minWidth: 0,
    minHeight: 0,
    display: "flex",
    flexDirection: "column",
    border: "1px solid var(--border-1)",
    borderRadius: 14,
    overflow: "hidden",
    background:
      "radial-gradient(circle at top right, rgba(63, 157, 246, 0.08), transparent 30%), linear-gradient(180deg, rgba(15,16,21,0.98), rgba(10,11,15,0.98))",
    boxShadow: "0 18px 48px rgba(0,0,0,0.28)",
  },
  toolbar: {
    height: 46,
    display: "flex",
    alignItems: "center",
    gap: 8,
    padding: "0 14px",
    borderBottom: "1px solid var(--border-0)",
    background: "rgba(16,16,19,0.9)",
    flexShrink: 0,
  },
  toolbarButton: {
    height: 28,
    padding: "0 10px",
    borderRadius: 6,
    border: "1px solid var(--border-1)",
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    color: "var(--fg-1)",
    background: "var(--bg-2)",
    fontSize: 11,
    fontFamily: "var(--mono)",
  },
  toolbarButtonDanger: {
    color: "var(--red)",
  },
  toolbarButtonDisabled: {
    opacity: 0.45,
    cursor: "not-allowed",
  },
  toolbarPill: {
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    padding: "0 10px",
    height: 28,
    borderRadius: 999,
    background: "var(--bg-2)",
    border: "1px solid var(--border-1)",
    fontSize: 11,
    fontFamily: "var(--mono)",
    color: "var(--fg-2)",
  },
  toolbarPillDivider: {
    color: "var(--fg-3)",
  },
  pathBar: {
    height: 38,
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 12,
    padding: "0 14px",
    borderBottom: "1px solid var(--border-0)",
    background: "rgba(21,21,26,0.88)",
    flexShrink: 0,
  },
  pathTrail: {
    display: "flex",
    alignItems: "center",
    minWidth: 0,
    overflow: "auto",
    gap: 2,
    fontFamily: "var(--mono)",
    fontSize: 11,
  },
  pathButton: {
    padding: "4px 8px",
    borderRadius: 4,
    color: "var(--fg-2)",
    whiteSpace: "nowrap",
  },
  pathButtonActive: {
    padding: "4px 8px",
    borderRadius: 4,
    background: "var(--accent-ghost)",
    color: "var(--fg-0)",
    whiteSpace: "nowrap",
  },
  pathDivider: {
    color: "var(--fg-4)",
    fontFamily: "var(--mono)",
  },
  pathMeta: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    flexShrink: 0,
  },
  pathFilterInput: {
    width: 190,
    height: 24,
    border: "1px solid var(--border-1)",
    borderRadius: 3,
    background: "var(--bg-1)",
    color: "var(--fg-1)",
    padding: "0 8px",
    fontFamily: "var(--mono)",
    fontSize: 10,
    outline: "none",
  },
  pathStatus: {
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    color: "var(--fg-2)",
    fontSize: 11,
    fontFamily: "var(--mono)",
  },
  pathCount: {
    color: "var(--fg-3)",
    fontSize: 11,
    fontFamily: "var(--mono)",
  },
  browserPane: {
    flex: 1,
    minHeight: 0,
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr) 7px 340px",
  },
  tablePane: {
    minWidth: 0,
    minHeight: 0,
    display: "flex",
    flexDirection: "column",
  },
  blobListFooter: {
    height: 28,
    flexShrink: 0,
    borderTop: "1px solid var(--border-0)",
    background: "var(--bg-1)",
    display: "flex",
    alignItems: "center",
    gap: 10,
    padding: "0 10px",
    color: "var(--fg-3)",
    fontFamily: "var(--mono)",
    fontSize: 10,
  },
  blobListFooterHint: {
    color: "var(--fg-2)",
  },
  blobListFooterButton: {
    height: 20,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-1)",
    borderRadius: 3,
    padding: "0 8px",
    fontFamily: "var(--mono)",
    fontSize: 10,
    cursor: "pointer",
  },
  blobListFooterButtonDisabled: {
    opacity: 0.45,
    cursor: "default",
  },
  inspectorPane: {
    background: "var(--bg-1)",
    display: "flex",
    flexDirection: "column",
  },
  inspectorHeader: {
    height: 32,
    padding: "0 12px",
    borderBottom: "1px solid var(--border-0)",
    display: "flex",
    alignItems: "center",
    gap: 6,
    fontFamily: "var(--mono)",
    fontSize: 10,
    fontWeight: 600,
    textTransform: "uppercase",
    letterSpacing: "0.08em",
    color: "var(--fg-3)",
  },
  inspectorEmpty: {
    padding: 16,
    color: "var(--fg-2)",
    lineHeight: 1.6,
  },
  mainEmptyWrap: {
    flex: 1,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    padding: 24,
  },
  mainEmptyCard: {
    width: "100%",
    maxWidth: 560,
    borderRadius: 18,
    border: "1px solid var(--border-1)",
    background: "linear-gradient(180deg, rgba(28,28,34,0.94) 0%, rgba(16,16,19,0.98) 100%)",
    padding: "28px 28px 26px",
    display: "flex",
    flexDirection: "column",
    alignItems: "flex-start",
    gap: 12,
    boxShadow: "0 18px 60px rgba(0,0,0,0.28)",
    animation: "arkived-scale-in 160ms ease-out",
  },
  mainEmptyIcon: {
    width: 42,
    height: 42,
    borderRadius: 12,
    background: "var(--accent-ghost)",
    color: "var(--accent)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
  },
  mainEmptyTitle: {
    margin: 0,
    fontSize: 24,
    lineHeight: 1.1,
    fontWeight: 700,
    color: "var(--fg-0)",
  },
  mainEmptyBody: {
    margin: 0,
    color: "var(--fg-2)",
    lineHeight: 1.7,
    maxWidth: 480,
  },
  mainEmptySecondary: {
    color: "var(--fg-3)",
    fontSize: 11,
    fontFamily: "var(--mono)",
  },
  overlay: {
    position: "fixed",
    inset: 0,
    background: "rgba(6,6,8,0.76)",
    backdropFilter: "blur(10px)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    padding: 24,
    zIndex: 10,
  },
  dialog: {
    width: "min(1080px, 100%)",
    minHeight: 620,
    display: "grid",
    gridTemplateColumns: "320px minmax(0, 1fr)",
    borderRadius: 20,
    overflow: "hidden",
    border: "1px solid var(--border-1)",
    boxShadow: "0 28px 90px rgba(0,0,0,0.45)",
    background: "var(--bg-1)",
    animation: "arkived-scale-in 160ms ease-out",
  },
  dialogSidebar: {
    borderRight: "1px solid var(--border-0)",
    background: "linear-gradient(180deg, rgba(15,15,19,1) 0%, rgba(10,10,12,1) 100%)",
    padding: 18,
    display: "flex",
    flexDirection: "column",
    gap: 16,
  },
  dialogSidebarTitle: {
    fontSize: 18,
    fontWeight: 700,
    color: "var(--fg-0)",
  },
  dialogSidebarBody: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
  },
  manageAccountCard: {
    borderRadius: 12,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    padding: 12,
    display: "flex",
    flexDirection: "column",
    gap: 6,
  },
  manageAccountTitle: {
    color: "var(--fg-0)",
    fontWeight: 700,
  },
  manageAccountText: {
    color: "var(--fg-2)",
    fontSize: 11,
    lineHeight: 1.5,
  },
  dialogMain: {
    minWidth: 0,
    display: "flex",
    flexDirection: "column",
    background: "radial-gradient(circle at top right, rgba(63,157,246,0.08), transparent 25%), var(--bg-1)",
  },
  dialogHeader: {
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    gap: 16,
    padding: "22px 24px 18px",
    borderBottom: "1px solid var(--border-0)",
  },
  dialogEyebrow: {
    fontSize: 10,
    fontFamily: "var(--mono)",
    textTransform: "uppercase",
    letterSpacing: "0.08em",
    color: "var(--fg-3)",
    marginBottom: 6,
  },
  dialogTitle: {
    margin: 0,
    fontSize: 28,
    lineHeight: 1.05,
    color: "var(--fg-0)",
  },
  dialogDescription: {
    margin: "8px 0 0",
    color: "var(--fg-2)",
    lineHeight: 1.6,
    maxWidth: 560,
  },
  closeButton: {
    height: 32,
    padding: "0 12px",
    borderRadius: 8,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-1)",
    fontFamily: "var(--mono)",
    fontSize: 11,
  },
  dialogContent: {
    flex: 1,
    overflow: "auto",
    padding: 24,
    display: "flex",
    flexDirection: "column",
    gap: 16,
  },
  dialogFooter: {
    padding: "18px 24px",
    borderTop: "1px solid var(--border-0)",
    display: "flex",
    justifyContent: "flex-end",
    gap: 10,
    background: "rgba(16,16,19,0.9)",
  },
  methodButton: {
    width: "100%",
    borderRadius: 12,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    padding: "12px 12px 12px 14px",
    textAlign: "left",
    display: "flex",
    alignItems: "flex-start",
    gap: 10,
  },
  methodButtonActive: {
    borderColor: "var(--accent-dim)",
    background: "rgba(63, 157, 246, 0.1)",
  },
  methodButtonLocked: {
    opacity: 0.85,
  },
  methodIcon: {
    color: "var(--accent)",
    marginTop: 2,
    display: "flex",
  },
  methodTextBlock: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
  },
  methodLabel: {
    fontWeight: 600,
    color: "var(--fg-0)",
  },
  methodDescription: {
    color: "var(--fg-2)",
    fontSize: 11,
    lineHeight: 1.5,
  },
  tenantCard: {
    borderRadius: 16,
    border: "1px solid var(--border-1)",
    background: "rgba(18,18,22,0.9)",
    padding: 16,
    display: "flex",
    flexDirection: "column",
    gap: 12,
  },
  tenantCardHeader: {
    display: "grid",
    gridTemplateColumns: "20px minmax(0, 1fr) auto",
    gap: 12,
    alignItems: "start",
  },
  tenantCardCopy: {
    minWidth: 0,
    display: "flex",
    flexDirection: "column",
    gap: 4,
  },
  tenantCardTitle: {
    color: "var(--fg-0)",
    fontWeight: 700,
  },
  tenantCardMeta: {
    color: "var(--fg-3)",
    fontSize: 11,
    fontFamily: "var(--mono)",
    wordBreak: "break-word",
  },
  tenantCardCounts: {
    color: "var(--fg-2)",
    fontSize: 11,
    fontFamily: "var(--mono)",
    whiteSpace: "nowrap",
  },
  tenantCardError: {
    borderRadius: 10,
    border: "1px solid rgba(224, 113, 110, 0.24)",
    background: "rgba(224, 113, 110, 0.12)",
    color: "var(--red)",
    padding: "10px 12px",
    fontSize: 12,
    lineHeight: 1.5,
  },
  subscriptionList: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    paddingLeft: 32,
  },
  subscriptionRow: {
    display: "grid",
    gridTemplateColumns: "20px minmax(0, 1fr) auto",
    gap: 10,
    alignItems: "center",
    color: "var(--fg-1)",
  },
  subscriptionName: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  subscriptionMeta: {
    color: "var(--fg-3)",
    fontSize: 11,
    fontFamily: "var(--mono)",
    whiteSpace: "nowrap",
  },
  subscriptionEmpty: {
    color: "var(--fg-3)",
    fontSize: 11,
    lineHeight: 1.5,
  },
  tenantModeGroup: {
    display: "grid",
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    gap: 10,
  },
  tenantModeButton: {
    borderRadius: 12,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    padding: 12,
    textAlign: "left",
    display: "flex",
    flexDirection: "column",
    gap: 6,
    minHeight: 108,
    color: "var(--fg-2)",
  },
  tenantModeButtonActive: {
    borderColor: "var(--accent-dim)",
    background: "rgba(63, 157, 246, 0.1)",
  },
  tenantModeLabel: {
    color: "var(--fg-0)",
    fontWeight: 600,
  },
  tenantModeText: {
    color: "var(--fg-2)",
    fontSize: 11,
    lineHeight: 1.5,
  },
  field: {
    display: "flex",
    flexDirection: "column",
    gap: 6,
  },
  fieldLabel: {
    fontSize: 11,
    fontFamily: "var(--mono)",
    color: "var(--fg-2)",
    textTransform: "uppercase",
    letterSpacing: "0.04em",
  },
  fieldInput: {
    width: "100%",
    minHeight: 40,
    padding: "10px 12px",
    borderRadius: 10,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-0)",
    lineHeight: 1.5,
  },
  fieldTextarea: {
    resize: "vertical",
    minHeight: 108,
  },
  infoCard: {
    borderRadius: 14,
    border: "1px solid var(--border-1)",
    background: "rgba(63, 157, 246, 0.06)",
    padding: 16,
    display: "flex",
    flexDirection: "column",
    gap: 8,
  },
  infoTitle: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    color: "var(--fg-0)",
    fontWeight: 600,
  },
  infoText: {
    color: "var(--fg-2)",
    lineHeight: 1.6,
  },
  promptCard: {
    borderRadius: 16,
    border: "1px solid var(--border-1)",
    background: "linear-gradient(180deg, rgba(22, 32, 44, 0.85), rgba(16, 16, 19, 0.95))",
    padding: 20,
    display: "flex",
    flexDirection: "column",
    gap: 16,
  },
  promptStatus: {
    display: "inline-flex",
    alignItems: "center",
    gap: 8,
    color: "var(--fg-1)",
    fontWeight: 600,
  },
  promptCodeRow: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 12,
    flexWrap: "wrap",
  },
  promptCode: {
    padding: "12px 16px",
    borderRadius: 12,
    background: "rgba(63, 157, 246, 0.12)",
    border: "1px solid rgba(63, 157, 246, 0.28)",
    color: "var(--fg-0)",
    fontSize: 24,
    fontWeight: 700,
    fontFamily: "var(--mono)",
    letterSpacing: "0.12em",
  },
  promptGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    gap: 12,
  },
  promptField: {
    borderRadius: 12,
    border: "1px solid var(--border-1)",
    background: "rgba(0, 0, 0, 0.18)",
    padding: 12,
    display: "flex",
    flexDirection: "column",
    gap: 6,
  },
  promptFieldLabel: {
    fontSize: 10,
    textTransform: "uppercase",
    letterSpacing: "0.08em",
    fontFamily: "var(--mono)",
    color: "var(--fg-3)",
  },
  promptFieldValue: {
    fontFamily: "var(--mono)",
    color: "var(--fg-1)",
    lineHeight: 1.5,
    wordBreak: "break-word",
  },
  promptMessage: {
    borderRadius: 12,
    border: "1px dashed var(--border-2)",
    padding: 14,
    color: "var(--fg-2)",
    lineHeight: 1.6,
    background: "rgba(0, 0, 0, 0.16)",
  },
  promptActions: {
    display: "flex",
    gap: 10,
    flexWrap: "wrap",
  },
  azuriteCard: {
    borderRadius: 16,
    border: "1px solid var(--border-1)",
    background: "linear-gradient(180deg, rgba(18, 25, 32, 0.85), rgba(16, 16, 19, 0.96))",
    padding: 18,
    display: "flex",
    flexDirection: "column",
    gap: 10,
  },
  azuriteTitle: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    fontWeight: 600,
    color: "var(--fg-0)",
  },
  azuriteValue: {
    fontFamily: "var(--mono)",
    color: "var(--accent)",
    fontSize: 12,
  },
  azuriteText: {
    color: "var(--fg-2)",
    lineHeight: 1.6,
  },
  previewPane: {
    minWidth: 0,
    minHeight: 0,
    background: "var(--bg-1)",
    display: "flex",
    flexDirection: "column",
    overflow: "hidden",
  },
  previewResizeHandle: {
    width: PANE_RESIZE_HANDLE_WIDTH,
    minHeight: 0,
    cursor: "col-resize",
    background:
      "linear-gradient(90deg, rgba(255,255,255,0.02), rgba(63,157,246,0.18), rgba(255,255,255,0.02))",
    borderLeft: "1px solid var(--border-0)",
    borderRight: "1px solid var(--border-0)",
  },
  previewHeader: {
    height: 38,
    display: "flex",
    alignItems: "center",
    gap: 8,
    padding: "0 8px",
    borderBottom: "1px solid var(--border-0)",
    background: "var(--bg-2)",
    flexShrink: 0,
  },
  previewIcon: {
    width: 24,
    height: 24,
    borderRadius: 6,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    color: "var(--accent)",
    background: "rgba(63, 157, 246, 0.12)",
    border: "1px solid rgba(63, 157, 246, 0.22)",
  },
  previewTitle: {
    color: "var(--fg-0)",
    fontSize: 12,
    fontWeight: 650,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  previewPath: {
    color: "var(--fg-3)",
    fontFamily: "var(--mono)",
    fontSize: 10,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  previewHeaderButton: {
    height: 26,
    padding: "0 8px",
    borderRadius: 5,
    border: "1px solid var(--border-1)",
    background: "var(--bg-1)",
    color: "var(--fg-1)",
    display: "inline-flex",
    alignItems: "center",
    gap: 5,
    fontFamily: "var(--mono)",
    fontSize: 10,
  },
  previewCloseButton: {
    width: 26,
    height: 26,
    borderRadius: 5,
    border: "1px solid var(--border-1)",
    background: "var(--bg-1)",
    color: "var(--fg-1)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
  },
  previewBody: {
    flex: 1,
    minHeight: 0,
    display: "flex",
    flexDirection: "column",
    gap: 6,
    padding: 6,
    overflow: "hidden",
  },
  previewCentered: {
    flex: 1,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    gap: 10,
    color: "var(--fg-2)",
    fontFamily: "var(--mono)",
  },
  previewError: {
    borderRadius: 8,
    border: "1px solid rgba(224, 113, 110, 0.28)",
    background: "rgba(224, 113, 110, 0.1)",
    color: "var(--red)",
    padding: 12,
    display: "flex",
    gap: 10,
    alignItems: "flex-start",
  },
  previewErrorTitle: {
    color: "var(--fg-0)",
    fontWeight: 700,
    marginBottom: 4,
  },
  previewErrorBody: {
    color: "var(--red)",
    lineHeight: 1.5,
  },
  previewMetaStrip: {
    display: "flex",
    alignItems: "center",
    flexWrap: "wrap",
    gap: 10,
    minHeight: 24,
    padding: "3px 8px",
    borderRadius: 5,
    border: "1px solid var(--border-0)",
    background: "rgba(255, 255, 255, 0.02)",
    color: "var(--fg-3)",
    fontFamily: "var(--mono)",
    fontSize: 10,
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    flexShrink: 0,
  },
  previewPager: {
    marginLeft: "auto",
    display: "inline-flex",
    alignItems: "center",
    gap: 4,
    flexWrap: "wrap",
    justifyContent: "flex-end",
  },
  previewPagerButton: {
    height: 18,
    padding: "0 7px",
    borderRadius: 4,
    border: "1px solid var(--border-1)",
    background: "var(--bg-1)",
    color: "var(--fg-1)",
    fontFamily: "var(--mono)",
    fontSize: 9,
    textTransform: "uppercase",
    letterSpacing: "0.04em",
  },
  previewPageSizeSelect: {
    height: 20,
    borderRadius: 4,
    border: "1px solid var(--border-1)",
    background: "var(--bg-1)",
    color: "var(--fg-1)",
    fontFamily: "var(--mono)",
    fontSize: 9,
    textTransform: "uppercase",
    outline: "none",
  },
  previewWarning: {
    flexShrink: 0,
    display: "flex",
    alignItems: "flex-start",
    gap: 6,
    borderRadius: 6,
    border: "1px solid rgba(216, 184, 96, 0.24)",
    background: "rgba(216, 184, 96, 0.1)",
    color: "var(--yellow)",
    padding: "6px 8px",
    fontSize: 11,
    lineHeight: 1.45,
  },
  previewTableWrap: {
    flex: 1,
    minHeight: 0,
    overflow: "hidden",
    borderRadius: 3,
    border: "1px solid var(--border-0)",
    background: "var(--bg-1)",
    fontFamily: "var(--mono)",
    fontSize: 10,
    display: "flex",
    flexDirection: "column",
  },
  previewTableScroll: {
    flex: 1,
    minHeight: 0,
    overflow: "auto",
  },
  previewTableGrid: {
    display: "grid",
    minWidth: "max-content",
  },
  previewTableCellHeader: {
    position: "sticky",
    top: 0,
    zIndex: 2,
    padding: "5px 13px 5px 7px",
    borderRight: "1px solid var(--border-0)",
    borderBottom: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-2)",
    textTransform: "uppercase",
    letterSpacing: "0.035em",
    fontSize: 9,
    fontWeight: 700,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  previewTableCellText: {
    display: "block",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  previewColumnResizeHandle: {
    position: "absolute",
    top: 0,
    right: 0,
    width: 7,
    height: "100%",
    cursor: "col-resize",
    zIndex: 3,
    background: "linear-gradient(to right, transparent, transparent 3px, rgba(77, 166, 255, 0.28) 3px, rgba(77, 166, 255, 0.28) 4px, transparent 4px)",
  },
  previewTableCell: {
    padding: "5px 7px",
    borderRight: "1px solid var(--border-0)",
    borderBottom: "1px solid var(--border-0)",
    color: "var(--fg-1)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    cursor: "default",
  },
  previewTableCellSelected: {
    background: "var(--accent-ghost-strong)",
    color: "#ffffff",
  },
  previewTableFooter: {
    minHeight: 28,
    flexShrink: 0,
    display: "flex",
    alignItems: "center",
    gap: 8,
    padding: "4px 7px",
    borderTop: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-3)",
  },
  previewTableFooterStatus: {
    flex: 1,
    minWidth: 160,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  previewPageLabel: {
    minWidth: 58,
    color: "var(--fg-2)",
    textAlign: "center",
  },
  previewText: {
    flex: 1,
    minHeight: 0,
    overflow: "auto",
    margin: 0,
    borderRadius: 4,
    border: "1px solid var(--border-0)",
    background: "var(--bg-1)",
    color: "var(--fg-1)",
    padding: 10,
    fontFamily: "var(--mono)",
    fontSize: 11,
    lineHeight: 1.45,
    whiteSpace: "pre-wrap",
  },
  previewImageWrap: {
    flex: 1,
    minHeight: 0,
    overflow: "auto",
    borderRadius: 4,
    border: "1px solid var(--border-0)",
    background: "var(--bg-1)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    padding: 16,
  },
  previewImage: {
    maxWidth: "100%",
    maxHeight: "100%",
    objectFit: "contain",
  },
  previewEmpty: {
    borderRadius: 4,
    border: "1px solid var(--border-0)",
    background: "var(--bg-1)",
    color: "var(--fg-2)",
    padding: 18,
    lineHeight: 1.6,
  },
  banner: {
    margin: "0 12px",
    borderRadius: 10,
    padding: "10px 12px",
    display: "flex",
    alignItems: "flex-start",
    gap: 8,
    fontSize: 12,
    lineHeight: 1.5,
  },
  bannerWarn: {
    background: "rgba(216, 184, 96, 0.12)",
    color: "var(--yellow)",
    border: "1px solid rgba(216, 184, 96, 0.24)",
  },
  bannerError: {
    background: "rgba(224, 113, 110, 0.12)",
    color: "var(--red)",
    border: "1px solid rgba(224, 113, 110, 0.24)",
  },
  bannerIcon: {
    marginTop: 1,
    display: "flex",
  },
  iconButton: {
    width: 28,
    height: 28,
    borderRadius: 6,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    color: "var(--fg-2)",
    background: "var(--bg-2)",
    border: "1px solid var(--border-1)",
  },
  iconButtonDisabled: {
    opacity: 0.45,
    cursor: "not-allowed",
  },
  primaryButton: {
    height: 36,
    padding: "0 14px",
    borderRadius: 10,
    border: "1px solid rgba(63, 157, 246, 0.45)",
    background: "linear-gradient(180deg, rgba(85, 170, 247, 1) 0%, rgba(63, 157, 246, 1) 100%)",
    color: "#07111d",
    fontWeight: 700,
    display: "inline-flex",
    alignItems: "center",
    gap: 8,
  },
  secondaryButton: {
    height: 34,
    padding: "0 12px",
    borderRadius: 9,
    border: "1px solid var(--border-1)",
    background: "var(--bg-2)",
    color: "var(--fg-1)",
    display: "inline-flex",
    alignItems: "center",
    gap: 8,
    fontFamily: "var(--mono)",
    fontSize: 11,
  },
};

function treeHintStyle(depth: number): CSSProperties {
  return {
    marginLeft: 14 + depth * 12,
    display: "flex",
    alignItems: "center",
    gap: 6,
    color: "var(--fg-2)",
    fontSize: 11,
    padding: "6px 8px",
  };
}

function treeErrorStyle(depth: number): CSSProperties {
  return {
    marginLeft: 14 + depth * 12,
    color: "var(--red)",
    fontSize: 11,
    lineHeight: 1.5,
    padding: "4px 8px",
  };
}

function treeEmptyStyle(depth: number): CSSProperties {
  return {
    marginLeft: 14 + depth * 12,
    color: "var(--fg-3)",
    fontSize: 11,
    padding: "4px 8px",
  };
}

export default App;
