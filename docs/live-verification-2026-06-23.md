# Live Azure verification — findings (2026-06-23)

Results of exercising the `arkived` CLI (and, by extension, the shared
`arkived-core` backend it sits on) against **two real Azure Storage accounts**.
This validates the signing, REST round-trips, policy gates, and output
formatting that unit tests — which use fakes/mocks — cannot fully cover.

## Methodology

- Driver: the compiled `arkived` CLI (`cargo build -p arkived`), since it
  shares `arkived_core::ConnectionParts` and the `AzureBlobBackend` /
  `AzureQueueBackend` with the MCP server and desktop app. Verifying the CLI
  transitively verifies the core used by every surface.
- Credentials were supplied as connection strings and kept only in a
  gitignored job-temp env file — never committed, never echoed (SAS `sig`
  redacted in logs).
- All writes targeted **throwaway containers/queues** (`arktest-smoke`,
  `arkdepth-smoke`, `arkq-smoke`); every one was deleted afterward and absence
  re-confirmed. No pre-existing data was modified.

### Accounts

| Tag | Account | Notable trait | Endpoints exercised |
|-----|---------|---------------|---------------------|
| #1 | `stdinphoenixdevdlp` | **Hierarchical namespace (ADLS Gen2)** | Blob (Queue not provisioned) |
| #2 | `htstorageprod` | Flat namespace (GPv2) | Blob + Queue |

The two accounts were complementary: #1 proved the read/write/SAS paths and
surfaced two bugs; #2 (non-HNS, with the Queue service) verified the depth ops
and the full Queues lifecycle that #1 could not.

## Verified working (live)

### Blob — core
| Operation | Account | Result |
|-----------|---------|--------|
| `doctor` | #1, #2 | Connectivity OK; lists container count |
| `ls` (containers / blobs / virtual dirs) | #1, #2 | Correct, with tier + lease columns |
| `ls --format json` | #1 | Valid JSON (after the datetime fix below) |
| `cat` | #1 | Streams blob; content round-trips byte-for-byte |
| `cp` upload + download | #1, #2 | Bytes + ETag correct both directions |
| `properties` | #1 | content-length/type, md5, etag, lease state |
| `meta` / `set-meta` | #1 | Replaces & reads back user metadata |
| `set-props` | #1 | content-type + cache-control persisted |
| `container create` / `delete` | #1, #2 | Lifecycle OK |
| `rm` | #1 | Deletes blob (policy-gated) |

### SAS — the hand-rolled crypto
- `sas <blob> --permissions r --expiry-hours 1` produced an account-key
  **Service SAS** (`sv=2022-11-02&sr=b&sp=r&se=...&spr=https&sig=...`).
- The generated URL fetched **HTTP 200 with the exact blob bytes**.
- This is the single most security-sensitive, entirely hand-written code path
  (HMAC-SHA256 string-to-sign + canonical permission ordering). It is correct
  against live Azure.

### Blob — depth ops (account #2, non-HNS)
| Operation | Result |
|-----------|--------|
| `set-tags` / `tags` | `project=arkived, env=prod` round-tripped |
| `set-tier cool` | `ls` reflects tier `Cool` |
| `snapshot` | Returns snapshot timestamp |
| `lease acquire` / `lease break` | Lease id returned; break succeeds |

### Queues — full lifecycle (account #2)
`create → put×3 → list → peek → get → clear → delete`, with **correct
semantics**:
- `peek` is non-destructive (`dequeue_count` stays 0, `pop_receipt` null).
- `get` dequeues (`dequeue_count`→1, real `pop_receipt`).
- After `get`, `peek` returns only the still-visible message — the dequeued
  ones are correctly hidden for their 30s **visibility window**.
- `clear` empties the queue; `delete` removes it.
- XML message parsing, pop receipts, and visibility timeouts all correct.

### Blob — additional corners (account #2)
| Scenario | Result |
|----------|--------|
| Remote→remote copy | `cp arkcorner/s.txt arkcorner/s-copy.txt` — both present |
| Multi-block upload (5 MiB) | Uploaded via PutBlock/PutBlockList; **md5 round-trip matches** |
| Missing blob | `cat` → `not found: BlobNotFound` (`Error::NotFound`) |
| Overwrite without `--force` | `cp` → `conflict: BlobAlreadyExists` (`Error::Conflict`) |
| Malformed account key | `ls` → `authentication failed: SharedKey sign: BadKey` (`Error::AuthFailed`, caught locally before any network call) |

### MCP server — end-to-end over stdio (account #2)
Drove the `arkived mcp` stdio server with a JSON-RPC client against live Azure:
- `initialize` → server `arkived 0.0.1`; `tools/list` → **20 tools**.
- `list_containers` → live containers (incl. correct `public_access: blob`).
- `list_queues` → live queue list.
- `peek_messages` → returned a message seeded via the CLI.

This verifies the full MCP stack (stdio transport → rmcp → `ArkivedServer` →
`AzureBlobBackend`/`AzureQueueBackend` → live Azure). Read-only tools were
exercised directly; destructive tools require MCP elicitation (a client-side
confirm round-trip) and were not driven by the bare smoke client.

### Policy gate
Destructive/mutating ops (`rm`, `container delete`, `set-meta`, `set-props`,
`set-tags`, `set-tier`, `snapshot`, `lease break`, `sas`, `queue clear/delete`)
correctly refuse in a non-interactive shell:
`operation denied by policy: no interactive terminal to confirm; re-run with --yes`.
Passing `--yes` allows them. The safety layer works as designed.

## Bugs found and fixed (TDD, committed)

### 1. `last_modified` serialized as a raw time-component array
- **Symptom:** `ls --format json` emitted
  `"last_modified": [2026, 76, 9, 48, 18, 0, 0, 0, 0]` — `time::OffsetDateTime`'s
  default serde representation (`76` is the ordinal day-of-year, not a month),
  which is neither human- nor machine-friendly.
- **Fix:** annotate the `Option<OffsetDateTime>` fields on `Container` and
  `BlobEntry::Blob` with `#[serde(with = "time::serde::rfc3339::option")]`.
  Output is now `"2026-03-17T09:48:18Z"`. These structs are populated from
  manually-parsed XML (not serde-deserialized), so Azure's RFC1123 dates on the
  wire are unaffected.
- **Tests:** 2 new serialization tests in `backend::types`.
- **Commit:** `c6c6f70`.

### 2. Connection errors retry-stormed into a ~90s hang
- **Symptom:** `queue list` against an account with no Queue service hung ~90s
  before failing.
- **Root cause:** every `reqwest` send error was mapped to
  `Error::NetworkTransient` (retryable), so an unresolvable/unreachable host was
  retried 8× with exponential backoff — and the clients had **no connect
  timeout** at all.
- **Fix:**
  - New non-retryable `Error::Connect` variant for connection-establishment
    failures (DNS failure, refused, connect timeout).
  - `classify_send_error` routes `reqwest::Error::is_connect()` → `Connect`;
    genuine mid-stream failures stay `NetworkTransient` (still retried).
  - `build_client()` adds a 15s `connect_timeout` (connection phase only, so
    large streaming uploads/downloads are **not** capped). Used by both the blob
    and queue backends.
- **Effect:** the same call now fails in ~12s with a clear
  `connection error: ...` message instead of hanging.
- **Tests:** connect-classification + fail-fast timing (`http`), non-retry
  (`retry`).
- **Commit:** `5c1bd11`.

## Not bugs — account capability limitations

On account #1 (HNS / ADLS Gen2) these failed; on account #2 (non-HNS) the same
commands succeeded — so arkived's requests are spec-correct and the failures are
purely account features:

| Operation | HNS error | Explanation |
|-----------|-----------|-------------|
| `tags` / `set-tags` | HTTP 400 `InvalidQueryParameterValue` | Blob index tags unsupported on HNS |
| `set-tier` | HTTP 400 `InvalidQueryParameterValue` | `comp=tier` sub-resource not exposed on HNS |
| `snapshot` | HTTP 409 `FeatureNotYetSupportedForHierarchicalNamespaceAccounts` | Explicit HNS limitation |

The explicit `...ForHierarchicalNamespaceAccounts` error on `snapshot`
definitively identified account #1 as HNS.

## Observations / future polish (not yet actioned)

- **Friendlier HNS errors.** When a feature is unsupported on the target account
  (the 400s above), arkived could detect HNS and surface a clearer message than
  the raw Azure code.
- **Stream `ls -r` output.** Recursive list buffers all pages before printing;
  on a very large container (account #1's `raw-device-telemetry`) this appears to
  hang. Streaming results per page would fix the perceived stall.
- **Explicit `QueueEndpoint`.** Account #2's connection string included a
  `QueueEndpoint`; `ConnectionParts::resolve_queue()` currently derives the queue
  host from the blob endpoint instead. They matched here, but honoring an
  explicit `QueueEndpoint` (and `TableEndpoint`/`FileEndpoint` when those services
  arrive) would be more correct for accounts with non-standard endpoints.

## Status snapshot

- Core tests: **156 pass** (4 added this session), clippy clean, fmt clean.
- Two fixes committed locally on `worktree-arkived-completion` (`c6c6f70`,
  `5c1bd11`); **not pushed**.
- Every CLI surface **and** the MCP server are now live-verified across the two
  accounts, including error paths (NotFound/Conflict/AuthFailed), multi-block
  upload integrity, and remote→remote copy.

## What remains un-verifiable here (gated)

These cannot be truly verified in a headless CLI session; they need a human,
a GUI, an AAD interaction, or signing material:

| Item | Why it's gated | What *can* be done without the gate |
|------|----------------|-------------------------------------|
| Desktop app (Tauri commands + UI, native policy) | Needs a rendered GUI to click | `cargo check` + `npm run build` (compile/typecheck only) |
| CLI `login` / `account` (AAD device-code) | Not implemented; needs interactive browser sign-in | Unit-test the device-code/ARM logic |
| User-delegation (AAD) SAS | Not implemented; needs a live Entra token | Unit-test the signing |
| Installers / auto-update | Needs signing certs + full platform toolchain | Configure bundle targets |
| Destructive MCP tools end-to-end | Need an MCP client that answers elicitation | Verified the gate exists; read tools verified live |

Everything that does **not** require one of those gates has been verified
against real Azure.
