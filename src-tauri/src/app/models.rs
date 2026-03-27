const STORE_FILE_NAME: &str = "state.json";
const DEFAULT_REMOTE_DIR: &str = "my-sync";
const MAX_SYNC_FILE_BYTES: u64 = 1024 * 1024;
const MAX_LOG_ENTRIES: usize = 500;
const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ICON_ID: &str = "main-tray";
const TRAY_MENU_SHOW_ID: &str = "tray-show";
const TRAY_MENU_QUIT_ID: &str = "tray-quit";
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileMapping {
    id: String,
    name: String,
    local_path: String,
    remote_path: String,
    #[serde(default)]
    path_template: String,
    #[serde(default)]
    binding_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebDavSettings {
    base_url: String,
    username: String,
    password: String,
    remote_dir: String,
    #[serde(default = "default_space_id", alias = "spaceId")]
    space_id: String,
    #[serde(default, alias = "clientId")]
    device_id: String,
    #[serde(default)]
    device_name: String,
    sync_interval_secs: u64,
    auto_sync: bool,
}

impl Default for WebDavSettings {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            username: String::new(),
            password: String::new(),
            remote_dir: DEFAULT_REMOTE_DIR.into(),
            space_id: default_space_id(),
            device_id: String::new(),
            device_name: String::new(),
            sync_interval_secs: 30,
            auto_sync: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SyncSettings {
    default_conflict_strategy: String,
    fs_watch_enabled: bool,
    debounce_delay_secs: u64,
    launch_on_boot: bool,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            default_conflict_strategy: "manual".into(),
            fs_watch_enabled: true,
            debounce_delay_secs: 15,
            launch_on_boot: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AppConfig {
    webdav: WebDavSettings,
    sync: SyncSettings,
    mappings: Vec<FileMapping>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRuntimeState {
    last_synced_hash: Option<String>,
    status: String,
    detail: String,
    last_sync_at: Option<String>,
    last_conflict_signature: Option<String>,
    last_conflict_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PersistedStore {
    config: AppConfig,
    file_states: HashMap<String, FileRuntimeState>,
    sync_logs: Vec<SyncLogEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SyncLogEntry {
    id: String,
    timestamp: String,
    level: String,
    action: String,
    summary: String,
    detail: String,
    http_status: Option<u16>,
    mapping_id: Option<String>,
    mapping_name: Option<String>,
    local_path: Option<String>,
    remote_path: Option<String>,
    target_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MappingRuntimeSnapshot {
    mapping_id: String,
    status: String,
    detail: String,
    last_sync_at: Option<String>,
    last_synced_hash: Option<String>,
    last_conflict_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSnapshot {
    is_syncing: bool,
    is_ready: bool,
    readiness_detail: String,
    conflict_count: usize,
    error_count: usize,
    last_run_at: Option<String>,
    last_summary: String,
    mappings: Vec<MappingRuntimeSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    config: AppConfig,
    runtime: RuntimeSnapshot,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
struct SharedSyncItem {
    id: String,
    name: String,
    remote_path: String,
    path_template: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SharedConfigFile {
    items: Vec<SharedSyncItem>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
struct DeviceBinding {
    sync_item_id: String,
    local_path: String,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
struct DeviceBindingsFile {
    device_id: String,
    device_name: String,
    bindings: Vec<DeviceBinding>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteBrowserEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: Option<u64>,
    modified_at: Option<String>,
}

#[derive(Debug, Clone)]
struct InMemoryState {
    config: AppConfig,
    file_states: HashMap<String, FileRuntimeState>,
    sync_logs: Vec<SyncLogEntry>,
    is_syncing: bool,
    last_run_at: Option<String>,
    last_summary: String,
    pending_sync_paths: HashSet<String>,
    pending_sync_started_at: Option<Instant>,
}

impl From<PersistedStore> for InMemoryState {
    fn from(value: PersistedStore) -> Self {
        Self {
            config: normalize_config(value.config),
            file_states: value.file_states,
            sync_logs: value.sync_logs,
            is_syncing: false,
            last_run_at: None,
            last_summary: "尚未同步".into(),
            pending_sync_paths: HashSet::new(),
            pending_sync_started_at: None,
        }
    }
}

#[derive(Clone)]
struct SharedState(Arc<Mutex<InMemoryState>>);

struct RemoteFile {
    bytes: Option<Vec<u8>>,
    modified_at: Option<DateTime<Utc>>,
    http_status: Option<u16>,
}

struct LocalFile {
    bytes: Option<Vec<u8>>,
    modified_at: Option<DateTime<Utc>>,
    size_bytes: Option<u64>,
}

struct SyncOutcome {
    file_states: HashMap<String, FileRuntimeState>,
    summary: String,
    logs: Vec<SyncLogEntry>,
}

#[derive(Debug, Deserialize)]
struct MultiStatus {
    #[serde(rename = "response", default)]
    responses: Vec<PropfindResponse>,
}

#[derive(Debug, Deserialize)]
struct PropfindResponse {
    href: String,
    #[serde(rename = "propstat", default)]
    propstats: Vec<PropStat>,
}

#[derive(Debug, Deserialize)]
struct PropStat {
    prop: PropNode,
}

#[derive(Debug, Deserialize)]
struct PropNode {
    #[serde(rename = "displayname")]
    display_name: Option<String>,
    #[serde(rename = "getcontentlength")]
    content_length: Option<String>,
    #[serde(rename = "getlastmodified")]
    last_modified: Option<String>,
    #[serde(rename = "resourcetype")]
    resource_type: Option<ResourceType>,
}

#[derive(Debug, Deserialize)]
struct ResourceType {
    collection: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConflictStrategy {
    Manual,
    Local,
    Remote,
}

fn default_space_id() -> String {
    "default".into()
}
