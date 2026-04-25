# Azure Storage Explorer Parity And Beyond

Arkived's product target is full practical parity with Microsoft Azure Storage
Explorer, followed by capabilities Storage Explorer does not attempt to own:
agent-safe automation, scriptable workflows, richer diagnostics, and eventually
multi-cloud storage operations.

This document is the parity contract. It is intentionally broader than the
current implementation.

## Source Baseline

Parity is measured against three inputs:

- Microsoft Learn and Azure product documentation for Storage Explorer.
- Current Storage Explorer desktop behavior verified manually.
- Arkived user workflows that already depend on Storage Explorer.

The implementation source of truth stays in code and tests. This document is a
planning map, not a claim that every item is shipped.

## Parity Pillars

### 1. Account, Sign-In, And Attach

Required parity:

- Browser Microsoft Entra sign-in with account picker.
- Device-code fallback.
- Multiple signed-in accounts at once.
- Tenant discovery, tenant filtering, subscription filtering, and tenant reauth.
- Azure public cloud, Azure China, Azure US Government, Azure Stack/custom clouds.
- Direct attach by connection string, account name and key, SAS token, SAS URL,
  anonymous public container, and Azurite.
- Attach data-plane resources with Entra ID when the user has data permissions
  but lacks ARM management permission.
- Persist accounts and direct attachments across restarts with secrets in the OS
  credential store.
- Setting to disable account-key fallback and force Entra data-plane auth.

Beyond parity:

- Per-account auth diagnostics that explain which path was used: Entra data
  token, ARM listKeys, shared key, SAS, anonymous, or Azurite.
- "Why can Storage Explorer open this but Arkived cannot?" checks for RBAC,
  shared-key policy, tenant CA/MFA, endpoint/DNS, and private endpoint mismatch.
- Import/export non-secret connection profile metadata.

### 2. Explorer Shell And Navigation

Required parity:

- Tree model: Azure account -> tenant/subscription -> storage account -> Blob
  Containers, File Shares, Queues, Tables, and Disks.
- Local & Attached resources node.
- Quick Access / pinned resources.
- Multiple expanded branches at once.
- One tab per opened container/share/queue/table/disk view.
- Breadcrumb navigation inside blob containers and file shares.
- Search/filter in tree and opened resource views.
- Properties and Actions panes for tree nodes and selected items.
- Context menus with Storage Explorer wording and grouping.
- Keyboard navigation, refresh, delete, upload/download shortcuts, and tree
  expansion shortcuts.

Beyond parity:

- Command palette that executes every visible action.
- Saved workspaces: restore open tabs, expanded tree state, selected account,
  active filters, and pane layout.
- Fast global resource search across cached ARM inventory.

### 3. Blob Containers

Required parity:

- List, create, rename where supported, delete, refresh.
- Open in tab and open in new tab.
- Set public access level.
- Container properties: URL, ETag, last modified, lease state/status, access
  policy, HNS/public access indicators.
- Manage stored access policies.
- Generate container SAS and copy SAS URL.
- Acquire, release, and break container leases.
- Configure blob soft-delete policy from the Blob Containers node.
- List deleted containers and undelete where the service supports it.

Beyond parity:

- Diff container settings across accounts/subscriptions.
- Policy simulation before public access or retention changes.

### 4. Blobs And Virtual Directories

Required parity:

- List blobs and virtual directories with paging and continuation tokens.
- Sort columns, resize columns, and filter by prefix/name.
- Upload files, upload folders, and upload VHD/VHDX as page blobs.
- Download blobs and directories.
- Open/preview blobs using the OS or built-in previewer.
- Copy, paste, clone, rename, and delete blobs/directories.
- Copy URL, copy path, copy direct link.
- Blob properties: content type, encoding, cache-control, metadata, tags, ETag,
  MD5, size, tier, blob type, lease, last modified, creation time, immutability,
  legal hold, version/snapshot identifiers.
- Edit metadata and index tags.
- Set access tier: Hot, Cool, Cold, Archive.
- Rehydrate archived blobs with standard or high priority.
- Snapshots: create, list, promote, delete, compare, download.
- Versions: view versions, promote version, delete version, download version.
- Soft delete: view deleted items, undelete selected items, recursive undelete,
  and version-aware soft-delete behavior.
- HNS-aware deletion IDs for same-named deleted paths.

Beyond parity:

- Dry-run bulk plans before copy/delete/tier/metadata operations.
- Cost and risk estimate before tier changes, deletes, and cross-region copies.
- Content-aware preview for JSON, CSV, Parquet, images, PDFs, logs, and common
  archive formats.

### 5. ADLS Gen2

Required parity:

- Detect HNS-enabled accounts and expose filesystem semantics.
- Real directory create, rename, move, delete, and recursive operations.
- ACL viewer/editor for access ACLs and default ACLs.
- Recursive ACL propagation.
- Owner/group/permissions display.
- Blob and DFS endpoint handling, including private endpoint diagnostics.
- Soft-delete behavior specific to HNS accounts.

Beyond parity:

- ACL diff/preview before applying recursively.
- Explain effective access for a selected identity.
- Export/import ACL templates.

### 6. Azure Files

Required parity:

- List, create, rename where supported, delete file shares.
- Set share quota and inspect share properties.
- Browse directories and files.
- Create, rename, move, delete directories and files.
- Upload files/folders and download files/folders.
- Copy/paste within and across shares/accounts where supported.
- Manage stored access policies.
- Generate SAS for share/file/directory.
- Share snapshots: create, browse, restore/copy from snapshot, delete snapshot.
- SMB connection helper where feasible.

Beyond parity:

- Generate mount commands for Windows, macOS, and Linux with environment checks.
- Compare share snapshots.

### 7. Queues

Required parity:

- List, create, delete queues.
- View queue properties and metadata.
- Add messages as text or base64.
- Peek messages.
- Dequeue/delete messages.
- Clear queue.
- Show insertion time, expiration time, dequeue count, message ID, and TTL.
- Manage stored access policies and SAS.

Beyond parity:

- Message replay workflow between queues.
- Poison-message analysis and export.

### 8. Tables

Required parity:

- List, create, delete tables.
- Query entities with OData filters and selected columns.
- Add, edit, delete entities.
- Import/export CSV.
- Inspect partition key, row key, timestamp, ETag, and typed properties.
- Manage stored access policies and SAS.

Beyond parity:

- Query builder with saved queries.
- Schema inference and typed diff between tables.

### 9. Managed Disks

Required parity:

- List managed disks under subscriptions/resource groups.
- Upload VHD to new managed disk.
- Download managed disk to local VHD where supported.
- Copy managed disk across subscriptions/regions.
- Create disk snapshots.
- Show disk properties and snapshot metadata.

Beyond parity:

- Migration planner for region/account moves.
- Copy validation and post-copy integrity report.

### 10. SAS, Direct Links, And Sharing

Required parity:

- Account SAS, service SAS, user-delegation SAS.
- SAS based on stored access policy.
- Copy SAS, SAS URL, connection string, direct link.
- `storageexplorer://` style direct links for resources and paths.
- SAS direct links that attach at target root/directory or open/download target.
- Security warning when generating or copying sensitive links.

Beyond parity:

- Expiring link registry so generated SAS links are visible and revocable when
  backed by stored access policies.
- Least-privilege SAS generator that starts from intended action.

### 11. Transfers, Activities, And Jobs

Required parity:

- Upload/download/copy/delete operations appear in an Activities pane.
- Progress, throughput, remaining time, status, retry, cancel.
- Large transfer support with chunking and concurrency controls.
- Cross-account and cross-tenant copies.
- AzCopy-equivalent performance, or direct AzCopy integration where that is the
  pragmatic path.
- Transfer logs and copyable command/debug details.

Beyond parity:

- Transfer recipes: resumable job manifests, scheduled jobs, and repeatable
  transfer profiles.
- Agent-readable job graph for automation.

### 12. Settings, Diagnostics, Accessibility

Required parity:

- Theme support including dark, light, and high contrast.
- Proxy and certificate diagnostics.
- Clear token cache and re-enter credentials.
- Signed-in identity and tenant error diagnostics.
- Configurable key usage.
- HTTPS enforcement with explicit HTTP fallback for local/dev.
- Keyboard and screen-reader accessible explorer tree and dialogs.
- Release/update flow for desktop installers.

Beyond parity:

- `doctor` UI and CLI share the same diagnostic engine.
- Local audit log for destructive operations and generated SAS links.
- No default telemetry; opt-in diagnostics export only.

## Delivery Plan

### Phase 0: Stabilize Current Live Blob Browser

Exit criteria:

- Account sign-in, tenant filtering, storage account discovery, managed-key
  fallback, and credential persistence are reliable across app restarts.
- Multiple expanded accounts and container tabs work without losing state during
  refresh.
- Current blob listing has correct names, metadata, context menus, and
  properties.
- Real Azure and Azurite smoke tests cover the auth paths that already exist.

### Phase 1: Blob Daily-Use Parity

Exit criteria:

- Blob upload, download, open, copy URL/path, delete, rename, create folder,
  refresh, and properties are implemented end to end.
- Activities pane tracks all long-running blob operations.
- Destructive operations route through the shared Policy layer.

### Phase 2: Blob Depth Parity

Exit criteria:

- Snapshots, versions, soft-delete, undelete, access tiers, archive rehydrate,
  leases, metadata, index tags, immutability, and public access are implemented.
- Context menus no longer contain disabled placeholders for blob/container
  operations that Storage Explorer supports.

### Phase 3: ADLS Gen2 Parity

Exit criteria:

- HNS directories and ACL workflows match Storage Explorer's daily-use paths.
- Recursive ACL updates have preview, progress, cancellation, and policy gates.
- Blob/DFS endpoint diagnostics are clear enough for private endpoint setups.

### Phase 4: File Shares, Queues, Tables

Exit criteria:

- The tree exposes File Shares, Queues, and Tables under discovered accounts.
- Each service has create/list/open/properties/delete plus the service-specific
  operations listed above.
- SAS and stored access policies work for each service.

### Phase 5: Managed Disks, Direct Links, Installers

Exit criteria:

- Managed disk upload/download/copy/snapshot workflows are present.
- Direct links and SAS direct links work.
- Signed installers, updater, accessibility pass, and app settings are ready for
  non-developer use.

### Phase 6: Beyond Storage Explorer

Exit criteria:

- CLI, desktop, and future MCP/ACP surfaces share one backend and policy model.
- Arkived can produce safe operation plans for bulk changes before running them.
- Diagnostics explain auth, networking, endpoint, RBAC, and policy failures.
- Agent workflows can inspect, plan, and request approval without bypassing
  human policy gates.

## Engineering Rules

- Shared behavior belongs in `arkived-core`; UI, CLI, MCP, and ACP are thin
  surfaces.
- Every destructive action goes through `Policy::confirm`.
- Credentials stay in the OS credential store. SQLite stores metadata and
  references only.
- REST/API operations must have focused unit tests and at least one Azurite or
  real-Azure smoke path when the service supports it.
- Context-menu items should be disabled only while the backend operation is not
  implemented. Disabled parity placeholders should disappear as phases land.
- Parity wording should match Storage Explorer where users already rely on that
  vocabulary, but internal APIs should stay Arkived-native and testable.
