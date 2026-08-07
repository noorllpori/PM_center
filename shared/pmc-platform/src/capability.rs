use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum Capability {
    #[serde(rename = "app.profile.read")]
    AppProfileRead,
    #[serde(rename = "app.profile.write")]
    AppProfileWrite,
    #[serde(rename = "app.settings.read")]
    AppSettingsRead,
    #[serde(rename = "app.settings.write")]
    AppSettingsWrite,
    #[serde(rename = "notification.send")]
    NotificationSend,
    #[serde(rename = "clipboard.read")]
    ClipboardRead,
    #[serde(rename = "clipboard.write")]
    ClipboardWrite,
    #[serde(rename = "filesystem.dialog.open")]
    FilesystemDialogOpen,
    #[serde(rename = "filesystem.external.read")]
    FilesystemExternalRead,
    #[serde(rename = "filesystem.external.write")]
    FilesystemExternalWrite,
    #[serde(rename = "project.open")]
    ProjectOpen,
    #[serde(rename = "project.files.read")]
    ProjectFilesRead,
    #[serde(rename = "project.files.write")]
    ProjectFilesWrite,
    #[serde(rename = "project.metadata.read")]
    ProjectMetadataRead,
    #[serde(rename = "project.metadata.write")]
    ProjectMetadataWrite,
    #[serde(rename = "project.storage.read")]
    ProjectStorageRead,
    #[serde(rename = "project.storage.write")]
    ProjectStorageWrite,
    #[serde(rename = "project.storage.direct")]
    ProjectStorageDirect,
    #[serde(rename = "cache.inspect")]
    CacheInspect,
    #[serde(rename = "cache.maintain")]
    CacheMaintain,
    #[serde(rename = "task.run")]
    TaskRun,
    #[serde(rename = "task.cancel")]
    TaskCancel,
    #[serde(rename = "python.execute")]
    PythonExecute,
    #[serde(rename = "python.packages.manage")]
    PythonPackagesManage,
    #[serde(rename = "process.spawn")]
    ProcessSpawn,
    #[serde(rename = "network.http.request")]
    NetworkHttpRequest,
    #[serde(rename = "network.lan.discover")]
    NetworkLanDiscover,
    #[serde(rename = "network.lan.message")]
    NetworkLanMessage,
    #[serde(rename = "network.lan.transfer")]
    NetworkLanTransfer,
    #[serde(rename = "network.server.connect")]
    NetworkServerConnect,
    #[serde(rename = "render.inspect")]
    RenderInspect,
    #[serde(rename = "render.queue.read")]
    RenderQueueRead,
    #[serde(rename = "render.queue.write")]
    RenderQueueWrite,
    #[serde(rename = "render.worker.execute")]
    RenderWorkerExecute,
    #[serde(rename = "render.result.commit")]
    RenderResultCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityRisk {
    Normal,
    Sensitive,
    Critical,
}

impl Capability {
    pub const ALL: &'static [Self] = &[
        Self::AppProfileRead,
        Self::AppProfileWrite,
        Self::AppSettingsRead,
        Self::AppSettingsWrite,
        Self::NotificationSend,
        Self::ClipboardRead,
        Self::ClipboardWrite,
        Self::FilesystemDialogOpen,
        Self::FilesystemExternalRead,
        Self::FilesystemExternalWrite,
        Self::ProjectOpen,
        Self::ProjectFilesRead,
        Self::ProjectFilesWrite,
        Self::ProjectMetadataRead,
        Self::ProjectMetadataWrite,
        Self::ProjectStorageRead,
        Self::ProjectStorageWrite,
        Self::ProjectStorageDirect,
        Self::CacheInspect,
        Self::CacheMaintain,
        Self::TaskRun,
        Self::TaskCancel,
        Self::PythonExecute,
        Self::PythonPackagesManage,
        Self::ProcessSpawn,
        Self::NetworkHttpRequest,
        Self::NetworkLanDiscover,
        Self::NetworkLanMessage,
        Self::NetworkLanTransfer,
        Self::NetworkServerConnect,
        Self::RenderInspect,
        Self::RenderQueueRead,
        Self::RenderQueueWrite,
        Self::RenderWorkerExecute,
        Self::RenderResultCommit,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppProfileRead => "app.profile.read",
            Self::AppProfileWrite => "app.profile.write",
            Self::AppSettingsRead => "app.settings.read",
            Self::AppSettingsWrite => "app.settings.write",
            Self::NotificationSend => "notification.send",
            Self::ClipboardRead => "clipboard.read",
            Self::ClipboardWrite => "clipboard.write",
            Self::FilesystemDialogOpen => "filesystem.dialog.open",
            Self::FilesystemExternalRead => "filesystem.external.read",
            Self::FilesystemExternalWrite => "filesystem.external.write",
            Self::ProjectOpen => "project.open",
            Self::ProjectFilesRead => "project.files.read",
            Self::ProjectFilesWrite => "project.files.write",
            Self::ProjectMetadataRead => "project.metadata.read",
            Self::ProjectMetadataWrite => "project.metadata.write",
            Self::ProjectStorageRead => "project.storage.read",
            Self::ProjectStorageWrite => "project.storage.write",
            Self::ProjectStorageDirect => "project.storage.direct",
            Self::CacheInspect => "cache.inspect",
            Self::CacheMaintain => "cache.maintain",
            Self::TaskRun => "task.run",
            Self::TaskCancel => "task.cancel",
            Self::PythonExecute => "python.execute",
            Self::PythonPackagesManage => "python.packages.manage",
            Self::ProcessSpawn => "process.spawn",
            Self::NetworkHttpRequest => "network.http.request",
            Self::NetworkLanDiscover => "network.lan.discover",
            Self::NetworkLanMessage => "network.lan.message",
            Self::NetworkLanTransfer => "network.lan.transfer",
            Self::NetworkServerConnect => "network.server.connect",
            Self::RenderInspect => "render.inspect",
            Self::RenderQueueRead => "render.queue.read",
            Self::RenderQueueWrite => "render.queue.write",
            Self::RenderWorkerExecute => "render.worker.execute",
            Self::RenderResultCommit => "render.result.commit",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|capability| capability.as_str() == name)
    }

    pub const fn risk(self) -> CapabilityRisk {
        match self {
            Self::AppProfileRead
            | Self::AppSettingsRead
            | Self::NotificationSend
            | Self::FilesystemDialogOpen
            | Self::ProjectOpen
            | Self::ProjectFilesRead
            | Self::ProjectMetadataRead
            | Self::ProjectStorageRead
            | Self::CacheInspect
            | Self::TaskCancel
            | Self::NetworkLanDiscover
            | Self::RenderInspect
            | Self::RenderQueueRead => CapabilityRisk::Normal,
            Self::AppProfileWrite
            | Self::AppSettingsWrite
            | Self::ClipboardRead
            | Self::ClipboardWrite
            | Self::FilesystemExternalRead
            | Self::ProjectFilesWrite
            | Self::ProjectMetadataWrite
            | Self::ProjectStorageWrite
            | Self::TaskRun
            | Self::NetworkHttpRequest
            | Self::NetworkLanMessage
            | Self::NetworkLanTransfer
            | Self::NetworkServerConnect
            | Self::RenderQueueWrite => CapabilityRisk::Sensitive,
            Self::FilesystemExternalWrite
            | Self::ProjectStorageDirect
            | Self::CacheMaintain
            | Self::PythonExecute
            | Self::PythonPackagesManage
            | Self::ProcessSpawn
            | Self::RenderWorkerExecute
            | Self::RenderResultCommit => CapabilityRisk::Critical,
        }
    }
}
