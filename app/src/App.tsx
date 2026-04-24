import React, { CSSProperties, FormEvent, ReactNode, useEffect, useRef, useState } from "react";
import { GroupHeader, TitleBar, TreeRow } from "./chrome";
import { BlobTable, Inspector, TabsBar } from "./content";
import type { BlobRow } from "./data";
import {
  IconAlert,
  IconArrowUp,
  IconAzure,
  IconContainer,
  IconCopy,
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
} from "./icons";
import {
  BrowserConnection,
  BrowserContainer,
  BrowserLoginPrompt,
  BrowserSignIn,
  BrowserStorageAccount,
  BrowserSubscription,
  BrowserTenant,
  DeviceCodePrompt,
  connectAzurite,
  connectDiscoveredStorageAccount,
  connectWithAccountKey,
  connectWithConnectionString,
  connectWithSas,
  disconnectConnection,
  fetchBlobs,
  listConnections,
  listContainers,
  listDiscoveredStorageAccounts,
  listSignIns,
  listSignInTenants,
  pollEntraBrowserLogin,
  pollEntraDiscoveryLogin,
  pollSignInTenantReauth,
  startEntraBrowserLogin,
  startEntraDiscoveryLogin,
  startSignInTenantReauth,
  updateSignInFilter,
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
  rows: BlobRow[];
  busy: boolean;
  error: string | null;
  loaded: boolean;
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
  const [connectionsBusy, setConnectionsBusy] = useState(false);
  const [signInsBusy, setSignInsBusy] = useState(false);
  const [connectBusy, setConnectBusy] = useState(false);
  const [manageBusy, setManageBusy] = useState(false);
  const [tenantReauthBusy, setTenantReauthBusy] = useState(false);
  const [disconnectBusy, setDisconnectBusy] = useState(false);
  const [copiedCode, setCopiedCode] = useState(false);

  const containerRequestIds = useRef<Record<string, number>>({});
  const blobRequestIds = useRef<Record<string, number>>({});
  const openedDevicePromptId = useRef<string | null>(null);
  const tauriAvailable = useRef(isTauriRuntimeAvailable());
  const browserTabsRef = useRef<BrowserTabState[]>([]);
  const connectionsRef = useRef<BrowserConnection[]>([]);
  const containerStatesRef = useRef<Record<string, ContainerListState>>({});

  browserTabsRef.current = browserTabs;
  connectionsRef.current = connections;
  containerStatesRef.current = containerStatesByConnection;

  const activeTab = activeTabId ? browserTabs.find((tab) => tab.id === activeTabId) ?? null : null;
  const browsingConnectionId = activeTab?.connectionId ?? activeConnectionId;
  const activeConnection = connections.find((connection) => connection.id === browsingConnectionId) ?? null;
  const activeContainer = activeTab?.containerName ?? null;
  const activeRows = activeTab?.rows ?? [];
  const selectedIndices = activeTab?.selectedIndices ?? [];
  const selectedRows = new Set(selectedIndices);
  const selectedIndex = selectedIndices.length > 0 ? [...selectedIndices].sort((a, b) => a - b)[0] : null;
  const selectedRow = selectedIndex == null ? null : activeRows[selectedIndex] ?? null;
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
    updateTab(tabId, (currentTab) => ({
      ...currentTab,
      busy: true,
      error: null,
    }));

    try {
      const nextRows = await fetchBlobs(tab.connectionId, tab.containerName, tab.prefix || null);
      if (blobRequestIds.current[tabId] !== requestId) {
        return;
      }

      updateTab(tabId, (currentTab) => ({
        ...currentTab,
        rows: nextRows,
        busy: false,
        error: null,
        loaded: true,
        selectedIndices: [],
      }));
    } catch (error) {
      if (blobRequestIds.current[tabId] !== requestId) {
        return;
      }

      updateTab(tabId, (currentTab) => ({
        ...currentTab,
        rows: [],
        busy: false,
        error: getErrorMessage(error),
        loaded: true,
        selectedIndices: [],
      }));
    }
  }

  async function initializeShell() {
    if (!tauriAvailable.current) {
      setShellError("Live Azure browsing requires the Tauri desktop shell. Start this app with `npm run tauri:dev`.");
      return;
    }

    const [nextConnections, nextSignIns] = await Promise.all([refreshConnections(), refreshDiscoveryTree()]);
    if (nextConnections.length === 0 && nextSignIns.length === 0) {
      setConnectOpen(true);
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
          rows: [],
          busy: false,
          error: null,
          loaded: false,
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
      loaded: false,
      error: null,
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
      loaded: false,
      error: null,
      selectedIndices: [],
    }));
  }

  function handleToggleSelection(index: number) {
    if (!activeTab) {
      return;
    }

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

  function previewResource(url: string | null) {
    if (!url) {
      return;
    }
    window.open(url, "_blank", "noopener,noreferrer");
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
        agentOpen={false}
        onToggleAgent={() => undefined}
        activeConnection={titleConnection}
        connectionDetail={connectionDetail}
        connected={Boolean(activeConnection || signIns.length > 0) && !runtimeUnavailable}
        statusText={statusText}
        onRefresh={() => {
          void handleRefresh();
        }}
      />

      <div style={styles.shell}>
        <aside style={styles.sidebar}>
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

          <div style={styles.sidebarFooter}>
            <span style={styles.sidebarFooterLabel}>Mode</span>
            <span style={styles.sidebarFooterText}>
              Azure sign-in discovers subscriptions and storage accounts first, then activates the selected account into the live blob browser.
            </span>
          </div>
        </aside>

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
                        loaded: false,
                        error: null,
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
                            loaded: false,
                            error: null,
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
                  {activeTab.busy && (
                    <span style={styles.pathStatus}>
                      <IconLoader size={11} />
                      Loading…
                    </span>
                  )}
                  <span style={styles.pathCount}>
                    {activeRows.length} {activeRows.length === 1 ? "item" : "items"}
                  </span>
                </div>
              </div>

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
                <div style={styles.browserPane}>
                  <div style={styles.tablePane}>
                    {activeRows.length === 0 && !activeTab.busy ? (
                      <MainEmptyState
                        title="This prefix is empty"
                        body="The live container responded successfully, but there are no blobs or virtual directories at the current prefix."
                      />
                    ) : (
                      <BlobTable
                        rows={activeRows}
                        selected={selectedRows}
                        onToggleSelect={handleToggleSelection}
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
                          openContextMenu(event, [
                            {
                              label: row.kind === "dir" ? "Open" : "Open",
                              action: () => {
                                if (row.kind === "dir") {
                                  handleActivateRow(index);
                                } else {
                                  previewResource(rowUrl);
                                }
                              },
                            },
                            {
                              label: "Download",
                              disabled: row.kind === "dir" || !rowUrl,
                              hint: row.kind === "dir" ? undefined : "browser",
                              action: () => {
                                previewResource(rowUrl);
                              },
                            },
                            {
                              label: "Preview",
                              disabled: row.kind === "dir" || !rowUrl,
                              action: () => {
                                previewResource(rowUrl);
                              },
                            },
                            menuSeparator(),
                            {
                              label: "Copy",
                              action: () => {
                                void copyText(row.name);
                              },
                            },
                            {
                              label: "Paste",
                              disabled: true,
                              action: () => undefined,
                            },
                            {
                              label: "Clone…",
                              disabled: true,
                              action: () => undefined,
                            },
                            menuSeparator(),
                            {
                              label: "Delete",
                              disabled: true,
                              danger: true,
                              action: () => undefined,
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
                                  `${selectedRows.size || 1} selected • ${row.size ?? "size unavailable"}`,
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
                    )}
                  </div>

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
                </div>
              )}
            </>
          )}
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

const styles: Record<string, CSSProperties> = {
  appRoot: {
    display: "flex",
    flexDirection: "column",
    height: "100vh",
    background: "var(--bg-0)",
  },
  shell: {
    flex: 1,
    minHeight: 0,
    display: "flex",
    background: "linear-gradient(180deg, rgba(17,17,21,0.98) 0%, rgba(10,10,12,1) 100%)",
  },
  sidebar: {
    width: 340,
    minWidth: 300,
    display: "flex",
    flexDirection: "column",
    borderRight: "1px solid var(--border-0)",
    background: "linear-gradient(180deg, rgba(16,16,19,1) 0%, rgba(11,11,14,1) 100%)",
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
    background: "radial-gradient(circle at top right, rgba(63, 157, 246, 0.08), transparent 30%), var(--bg-1)",
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
    gridTemplateColumns: "minmax(0, 1fr) 320px",
  },
  tablePane: {
    minWidth: 0,
    minHeight: 0,
    display: "flex",
  },
  inspectorPane: {
    borderLeft: "1px solid var(--border-0)",
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
