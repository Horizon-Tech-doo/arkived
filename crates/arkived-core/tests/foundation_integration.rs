//! End-to-end integration test for the foundation layer: Store + types + context.
//!
//! This test exercises the realistic flow a CLI `login` + `context use` would trigger.

use arkived_core::store::{Store, SignIn, Subscription, StorageAccount, AttachedResource, CurrentContext, PolicyAllowEntry};
use arkived_core::types::{AuthKind, ResourceKind};
use chrono::Utc;
use tempfile::tempdir;

#[test]
fn full_flow_add_signin_discover_pick_context_policy_memory() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("state.db");

    // -- first process: user signs in, discovery populates subs + accounts
    {
        let store = Store::open(&db).unwrap();

        // 1. User signs in
        store.sign_in_insert(&SignIn {
            id: "si-hamza".into(),
            display_name: "Hamza Abdagić".into(),
            tenant_id: "tenant-abc".into(),
            environment: "azure".into(),
            user_principal: "hamza@horizon-tech.io".into(),
            added_at: Utc::now(),
        }).unwrap();

        // 2. Discovery finds two subscriptions
        for (id, name) in [("sub-dev", "din — development"), ("sub-prod", "Horizon Tech — Prod")] {
            store.subscription_upsert(&Subscription {
                id: id.into(),
                sign_in_id: "si-hamza".into(),
                name: name.into(),
                tenant_id: "tenant-abc".into(),
                discovered_at: Utc::now(),
            }).unwrap();
        }

        // 3. Discovery finds storage accounts under `sub-dev`
        for name in ["stdlnphoenixproddlp", "stdlnphoenixdevfunc"] {
            store.storage_account_upsert(&StorageAccount {
                name: name.into(),
                subscription_id: Some("sub-dev".into()),
                kind: "StorageV2 (ADLS Gen2)".into(),
                region: "West Europe".into(),
                replication: "LRS".into(),
                tier: "Premium".into(),
                hns: name.ends_with("proddlp"),
                endpoint: format!("https://{name}.blob.core.windows.net"),
                attached_directly: false,
            }).unwrap();
        }

        // 4. User attaches a read-only SAS container outside the sign-in
        store.attached_resource_insert(&AttachedResource {
            id: "att-1".into(),
            display_name: "dev-analytics-ro".into(),
            resource_kind: ResourceKind::BlobContainer,
            endpoint: "https://acme.blob.core.windows.net/readonly".into(),
            auth_kind: AuthKind::SasToken,
            keychain_ref: "arkived:connection:att-1".into(),
        }).unwrap();

        // 5. User picks a context: (si-hamza, sub-dev, stdlnphoenixproddlp)
        store.context_set_sign_in(Some("si-hamza")).unwrap();
        store.context_set_subscription(Some("sub-dev")).unwrap();
        store.context_set_account(Some("stdlnphoenixproddlp")).unwrap();

        // 6. A destructive op runs; user picks "Allow always for this kind, any target"
        store.policy_memory_allow("set_tier", None).unwrap();

        // 7. Sanity: everything is readable
        assert_eq!(store.sign_in_list().unwrap().len(), 1);
        assert_eq!(store.subscription_list_for_sign_in("si-hamza").unwrap().len(), 2);
        assert_eq!(store.storage_account_list_for_subscription("sub-dev").unwrap().len(), 2);
        assert_eq!(store.attached_resource_list().unwrap().len(), 1);
        assert_eq!(
            store.context_get().unwrap(),
            CurrentContext {
                sign_in_id: Some("si-hamza".into()),
                subscription_id: Some("sub-dev".into()),
                account_name: Some("stdlnphoenixproddlp".into()),
            }
        );
        assert!(store.policy_memory_is_allowed("set_tier", Some("anywhere")).unwrap());
    }

    // -- second process: reopen, verify persistence + policy_memory clear
    {
        let store = Store::open(&db).unwrap();

        // persistent data survives
        assert_eq!(store.sign_in_list().unwrap().len(), 1);
        assert_eq!(store.subscription_list_for_sign_in("si-hamza").unwrap().len(), 2);
        assert_eq!(store.storage_account_list_for_subscription("sub-dev").unwrap().len(), 2);
        assert_eq!(store.attached_resource_list().unwrap().len(), 1);
        assert_eq!(
            store.context_get().unwrap().account_name.as_deref(),
            Some("stdlnphoenixproddlp"),
        );

        // session-scope policy memory was truncated
        let entries: Vec<PolicyAllowEntry> = store.policy_memory_list().unwrap();
        assert_eq!(entries.len(), 0, "policy_memory must not survive process restart");
        assert!(!store.policy_memory_is_allowed("set_tier", Some("anywhere")).unwrap());

        // Cascade: deleting the sign-in nukes subscriptions (and via FK, any direct refs)
        store.sign_in_delete("si-hamza").unwrap();
        assert!(store.subscription_list_for_sign_in("si-hamza").unwrap().is_empty());
    }
}
