//! Shared enums used across subsystems.

use serde::{Deserialize, Serialize};

/// Azure storage resource categories — used by `AuthProvider::supports`
/// and the attach flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    /// The entire storage account.
    StorageAccount,
    /// A blob container.
    BlobContainer,
    /// An ADLS Gen2 container (hierarchical namespace).
    AdlsContainer,
    /// A directory inside an ADLS Gen2 container.
    AdlsDirectory,
    /// An Azure Files SMB file share.
    FileShare,
    /// An Azure Queue.
    Queue,
    /// An Azure Table.
    Table,
}

/// Classification of an `AuthProvider` — informs UI and CLI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthKind {
    /// Microsoft Entra ID via interactive browser.
    EntraInteractive,
    /// Microsoft Entra ID via device code flow.
    EntraDeviceCode,
    /// Storage account shared key.
    AccountKey,
    /// Full Azure Storage connection string.
    ConnectionString,
    /// Shared access signature (URL or token).
    SasToken,
    /// Anonymous (no credential; public containers only).
    Anonymous,
    /// Entra service principal (client_id + client_secret).
    ServicePrincipal,
    /// Managed identity (for Azure-hosted runtimes).
    ManagedIdentity,
    /// Workload identity (federated credentials).
    WorkloadIdentity,
    /// The local Azurite emulator's well-known dev credentials.
    AzuriteEmulator,
}

/// Azure cloud environment. Stored per-connection so one install can talk
/// to multiple clouds simultaneously.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AzureEnvironment {
    /// Public Azure (`core.windows.net`).
    Public,
    /// Azure China operated by 21Vianet (`core.chinacloudapi.cn`).
    China,
    /// Azure US Government (`core.usgovcloudapi.net`).
    UsGov,
    /// Azure Germany (`core.cloudapi.de`).
    Germany,
    /// Custom / sovereign / on-prem environment.
    Custom {
        /// Active Directory (Entra) endpoint URL.
        active_directory_url: String,
        /// DNS suffix for storage endpoints.
        storage_suffix: String,
    },
}

impl AzureEnvironment {
    /// Return the storage DNS suffix for this environment.
    pub fn storage_suffix(&self) -> &str {
        match self {
            Self::Public  => "core.windows.net",
            Self::China   => "core.chinacloudapi.cn",
            Self::UsGov   => "core.usgovcloudapi.net",
            Self::Germany => "core.cloudapi.de",
            Self::Custom { storage_suffix, .. } => storage_suffix,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_suffixes() {
        assert_eq!(AzureEnvironment::Public.storage_suffix(), "core.windows.net");
        assert_eq!(AzureEnvironment::China.storage_suffix(), "core.chinacloudapi.cn");
    }

    #[test]
    fn custom_suffix_passthrough() {
        let env = AzureEnvironment::Custom {
            active_directory_url: "https://ad.example".into(),
            storage_suffix: "blob.example.com".into(),
        };
        assert_eq!(env.storage_suffix(), "blob.example.com");
    }

    #[test]
    fn resource_kind_serde() {
        let r = ResourceKind::BlobContainer;
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#""blob-container""#);
        let back: ResourceKind = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
