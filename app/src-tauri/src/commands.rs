//! Stub IPC commands returning mock data that matches the design prototype.
//!
//! When `arkived-core` has a real Azure backend, these handlers will delegate
//! to it and route destructive ops through the Policy trait.

use serde::Serialize;

#[derive(Serialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub accounts: Vec<StorageAccount>,
}

#[derive(Serialize)]
pub struct StorageAccount {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub region: String,
    pub replication: String,
    pub tier: String,
    pub hns: bool,
    pub containers: Vec<Container>,
}

#[derive(Serialize)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub public_access: String,
    pub lease: String,
    pub blob_count: u64,
}

#[derive(Serialize)]
pub struct BlobRow {
    pub name: String,
    pub kind: String,
    pub size: Option<String>,
    pub tier: Option<String>,
    pub modified: String,
    pub etag: Option<String>,
    pub lease: Option<String>,
    pub icon: String,
}

#[derive(Serialize)]
pub struct Activity {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub detail: String,
    pub started: String,
    pub duration: Option<String>,
    pub progress: Option<f64>,
    pub result: Option<String>,
}

#[tauri::command]
pub fn list_subscriptions() -> Vec<Subscription> {
    vec![Subscription {
        id: "sub-dev".into(),
        name: "din — development".into(),
        owner: "hamza.abdagic@pontesolutions".into(),
        accounts: vec![StorageAccount {
            id: "stdlnphoenixproddlp".into(),
            name: "stdlnphoenixproddlp".into(),
            kind: "StorageV2 (ADLS Gen2)".into(),
            region: "West Europe".into(),
            replication: "LRS".into(),
            tier: "Premium".into(),
            hns: true,
            containers: vec![Container {
                id: "device-twins".into(),
                name: "device-twins".into(),
                public_access: "none".into(),
                lease: "available".into(),
                blob_count: 12843,
            }],
        }],
    }]
}

#[tauri::command]
pub fn list_blobs(_account: String, _container: String, _prefix: String) -> Vec<BlobRow> {
    // Mock data — mirrors design/data.jsx
    vec![
        BlobRow {
            name: "deviceSerialNumber_S=DA000405".into(),
            kind: "dir".into(),
            size: None,
            tier: None,
            modified: "2026-04-20 11:12:04".into(),
            etag: None,
            lease: None,
            icon: "folder".into(),
        },
        BlobRow {
            name: "part-00001-a2f4b8c.c000.snappy.parquet".into(),
            kind: "blob".into(),
            size: Some("14.2 MiB".into()),
            tier: Some("Hot".into()),
            modified: "2026-04-20 11:11:42".into(),
            etag: Some("0x8DC7A9F21B4E2C1".into()),
            lease: Some("avail".into()),
            icon: "parquet".into(),
        },
    ]
}

#[tauri::command]
pub fn list_activities() -> Vec<Activity> {
    vec![Activity {
        id: "a1".into(),
        kind: "upload".into(),
        status: "running".into(),
        title: "Upload 14 files → 'device-twins-sync/…'".into(),
        detail: "3 of 14 complete · 42 MiB/s".into(),
        started: "2026-04-20 11:13:18".into(),
        duration: None,
        progress: Some(0.34),
        result: None,
    }]
}

#[tauri::command]
pub fn agent_transcript() -> serde_json::Value {
    // Returns empty; frontend fallback uses embedded mock transcript until MCP wiring lands.
    serde_json::json!([])
}
