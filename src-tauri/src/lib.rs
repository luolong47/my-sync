use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use chrono::{DateTime, Utc};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tokio::{sync::Mutex, time::sleep};
use url::Url;

const STORE_FILE_NAME: &str = "state.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileMapping {
    id: String,
    name: String,
    local_path: String,
    remote_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebDavSettings {
    base_url: String,
    username: String,
    password: String,
    remote_dir: String,
    sync_interval_secs: u64,
    auto_sync: bool,
}

impl Default for WebDavSettings {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            username: String::new(),
            password: String::new(),
            remote_dir: "configs".into(),
            sync_interval_secs: 30,
            auto_sync: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppConfig {
    webdav: WebDavSettings,
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
#[serde(rename_all = "camelCase")]
struct PersistedStore {
    config: AppConfig,
    file_states: HashMap<String, FileRuntimeState>,
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

#[derive(Debug, Clone)]
struct InMemoryState {
    config: AppConfig,
    file_states: HashMap<String, FileRuntimeState>,
    is_syncing: bool,
    last_run_at: Option<String>,
    last_summary: String,
}

impl From<PersistedStore> for InMemoryState {
    fn from(value: PersistedStore) -> Self {
        Self {
            config: normalize_config(value.config),
            file_states: value.file_states,
            is_syncing: false,
            last_run_at: None,
            last_summary: "尚未同步".into(),
        }
    }
}

#[derive(Clone)]
struct SharedState(Arc<Mutex<InMemoryState>>);

struct RemoteFile {
    bytes: Option<Vec<u8>>,
    modified_at: Option<DateTime<Utc>>,
}

struct LocalFile {
    bytes: Option<Vec<u8>>,
    modified_at: Option<DateTime<Utc>>,
}

struct SyncOutcome {
    file_states: HashMap<String, FileRuntimeState>,
    summary: String,
}

#[derive(Clone)]
struct WebDavClient {
    client: reqwest::Client,
    settings: WebDavSettings,
}

impl WebDavClient {
    fn new(settings: WebDavSettings) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .user_agent("my-sync/0.1")
            .build()
            .map_err(|err| err.to_string())?;

        Ok(Self { client, settings })
    }

    async fn fetch_file(&self, remote_path: &str) -> Result<RemoteFile, String> {
        let url = self.file_url(remote_path)?;
        let response = self
            .client
            .get(url)
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .send()
            .await
            .map_err(|err| err.to_string())?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(RemoteFile {
                bytes: None,
                modified_at: None,
            });
        }

        let response = response.error_for_status().map_err(|err| err.to_string())?;
        let modified_at = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| httpdate::parse_http_date(value).ok())
            .map(DateTime::<Utc>::from);
        let bytes = response.bytes().await.map_err(|err| err.to_string())?.to_vec();

        Ok(RemoteFile {
            bytes: Some(bytes),
            modified_at,
        })
    }

    async fn upload_file(&self, remote_path: &str, bytes: Vec<u8>) -> Result<(), String> {
        self.ensure_parent_collections(remote_path).await?;
        let url = self.file_url(remote_path)?;
        let response = self
            .client
            .put(url)
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .body(bytes)
            .send()
            .await
            .map_err(|err| err.to_string())?;

        response.error_for_status().map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn ensure_parent_collections(&self, remote_path: &str) -> Result<(), String> {
        let mut all_segments = normalize_segments(&self.settings.remote_dir);
        let file_segments = normalize_segments(remote_path);

        if file_segments.len() > 1 {
            all_segments.extend(file_segments[..file_segments.len() - 1].iter().cloned());
        }

        let mkcol = Method::from_bytes(b"MKCOL").map_err(|err| err.to_string())?;
        let mut progressive: Vec<String> = Vec::new();
        for segment in all_segments {
            progressive.push(segment);
            let url = self.collection_url(&progressive)?;
            let response = self
                .client
                .request(mkcol.clone(), url)
                .basic_auth(&self.settings.username, Some(&self.settings.password))
                .send()
                .await
                .map_err(|err| err.to_string())?;

            let status = response.status();
            if !matches!(
                status,
                StatusCode::CREATED
                    | StatusCode::OK
                    | StatusCode::METHOD_NOT_ALLOWED
                    | StatusCode::MOVED_PERMANENTLY
                    | StatusCode::FOUND
            ) {
                return Err(format!("无法创建远端目录，状态码 {status}"));
            }
        }

        Ok(())
    }

    fn file_url(&self, remote_path: &str) -> Result<Url, String> {
        self.build_url_with_segments(&join_remote_segments(
            &self.settings.remote_dir,
            remote_path,
        ))
    }

    fn collection_url(&self, segments: &[String]) -> Result<Url, String> {
        self.build_url_with_segments(segments)
    }

    fn build_url_with_segments(&self, extra_segments: &[String]) -> Result<Url, String> {
        let mut url = Url::parse(self.settings.base_url.trim()).map_err(|err| err.to_string())?;
        let mut path_segments: Vec<String> = url
            .path()
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect();

        path_segments.extend(extra_segments.iter().cloned());
        let encoded = path_segments
            .iter()
            .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
            .collect::<Vec<_>>()
            .join("/");

        url.set_path(&format!("/{}", encoded));
        Ok(url)
    }
}

#[tauri::command]
async fn load_app_state(state: State<'_, SharedState>) -> Result<AppSnapshot, String> {
    let guard = state.0.lock().await;
    Ok(AppSnapshot {
        config: guard.config.clone(),
        runtime: runtime_snapshot(&guard),
    })
}

#[tauri::command]
async fn get_runtime_state(state: State<'_, SharedState>) -> Result<RuntimeSnapshot, String> {
    let guard = state.0.lock().await;
    Ok(runtime_snapshot(&guard))
}

#[tauri::command]
async fn save_app_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: AppConfig,
) -> Result<RuntimeSnapshot, String> {
    let normalized = normalize_config(config);
    let snapshot = {
        let mut guard = state.0.lock().await;
        guard.config = normalized;
        persist_state(&app, &guard)?;
        runtime_snapshot(&guard)
    };

    Ok(snapshot)
}

#[tauri::command]
async fn sync_now(app: AppHandle, state: State<'_, SharedState>) -> Result<RuntimeSnapshot, String> {
    perform_sync(app, state.0.clone()).await
}

fn runtime_snapshot(state: &InMemoryState) -> RuntimeSnapshot {
    let mut mappings = state
        .config
        .mappings
        .iter()
        .map(|mapping| {
            let runtime = state
                .file_states
                .get(&mapping.id)
                .cloned()
                .unwrap_or_else(default_runtime_state);

            MappingRuntimeSnapshot {
                mapping_id: mapping.id.clone(),
                status: runtime.status,
                detail: runtime.detail,
                last_sync_at: runtime.last_sync_at,
                last_synced_hash: runtime.last_synced_hash,
                last_conflict_path: runtime.last_conflict_path,
            }
        })
        .collect::<Vec<_>>();

    mappings.sort_by(|left, right| left.mapping_id.cmp(&right.mapping_id));

    RuntimeSnapshot {
        is_syncing: state.is_syncing,
        last_run_at: state.last_run_at.clone(),
        last_summary: state.last_summary.clone(),
        mappings,
    }
}

async fn perform_sync(
    app: AppHandle,
    shared: Arc<Mutex<InMemoryState>>,
) -> Result<RuntimeSnapshot, String> {
    let (config, file_states) = {
        let mut guard = shared.lock().await;
        if guard.is_syncing {
            return Ok(runtime_snapshot(&guard));
        }
        guard.is_syncing = true;
        (guard.config.clone(), guard.file_states.clone())
    };

    let outcome = sync_all_mappings(config, file_states).await;

    let mut guard = shared.lock().await;
    guard.is_syncing = false;
    guard.last_run_at = Some(now_string());

    match outcome {
        Ok(result) => {
            guard.file_states = result.file_states;
            guard.last_summary = result.summary;
            persist_state(&app, &guard)?;
            Ok(runtime_snapshot(&guard))
        }
        Err(err) => {
            guard.last_summary = format!("同步失败：{err}");
            persist_state(&app, &guard)?;
            Err(err)
        }
    }
}

async fn sync_all_mappings(
    config: AppConfig,
    mut file_states: HashMap<String, FileRuntimeState>,
) -> Result<SyncOutcome, String> {
    let client = WebDavClient::new(config.webdav.clone())?;
    let mut success_count = 0usize;
    let mut conflict_count = 0usize;
    let mut error_count = 0usize;

    for mapping in &config.mappings {
        let current = file_states
            .get(&mapping.id)
            .cloned()
            .unwrap_or_else(default_runtime_state);

        match sync_single_mapping(mapping, current.clone(), &client).await {
            Ok((next_state, status_kind)) => {
                if matches!(status_kind.as_str(), "synced" | "pushed" | "pulled") {
                    success_count += 1;
                }
                if status_kind == "conflict" {
                    conflict_count += 1;
                }
                file_states.insert(mapping.id.clone(), next_state);
            }
            Err(err) => {
                error_count += 1;
                file_states.insert(
                    mapping.id.clone(),
                    FileRuntimeState {
                        status: "error".into(),
                        detail: err,
                        last_sync_at: current.last_sync_at,
                        last_synced_hash: current.last_synced_hash,
                        last_conflict_signature: current.last_conflict_signature,
                        last_conflict_path: current.last_conflict_path,
                    },
                );
            }
        }
    }

    Ok(SyncOutcome {
        file_states,
        summary: format!(
            "成功 {} 项，冲突 {} 项，错误 {} 项",
            success_count, conflict_count, error_count
        ),
    })
}

async fn sync_single_mapping(
    mapping: &FileMapping,
    mut state: FileRuntimeState,
    client: &WebDavClient,
) -> Result<(FileRuntimeState, String), String> {
    let local_path = PathBuf::from(mapping.local_path.trim());
    let local = read_local_file(&local_path)?;
    let remote = client.fetch_file(mapping.remote_path.trim()).await?;

    let local_hash = local.bytes.as_ref().map(|bytes| hash_bytes(bytes));
    let remote_hash = remote.bytes.as_ref().map(|bytes| hash_bytes(bytes));
    let previous_hash = state.last_synced_hash.clone();

    if let (Some(left), Some(right)) = (&local_hash, &remote_hash) {
        if left == right {
            state.status = "synced".into();
            state.detail = "本地与远端一致".into();
            state.last_synced_hash = Some(left.clone());
            state.last_sync_at = Some(now_string());
            state.last_conflict_signature = None;
            state.last_conflict_path = None;
            return Ok((state, "synced".into()));
        }
    }

    match (local_hash.clone(), remote_hash.clone(), previous_hash.clone()) {
        (None, None, _) => {
            state.status = "idle".into();
            state.detail = "本地和远端都不存在该文件".into();
            Ok((state, "idle".into()))
        }
        (Some(local_hash), None, _) => {
            let bytes = local
                .bytes
                .ok_or_else(|| "读取本地文件失败".to_string())?;
            client.upload_file(mapping.remote_path.trim(), bytes).await?;
            state.status = "pushed".into();
            state.detail = "已将本地变更上传到远端".into();
            state.last_synced_hash = Some(local_hash);
            state.last_sync_at = Some(now_string());
            state.last_conflict_signature = None;
            state.last_conflict_path = None;
            Ok((state, "pushed".into()))
        }
        (None, Some(remote_hash), _) => {
            let bytes = remote
                .bytes
                .ok_or_else(|| "读取远端文件失败".to_string())?;
            write_local_file(&local_path, &bytes)?;
            state.status = "pulled".into();
            state.detail = "已从远端恢复到本地".into();
            state.last_synced_hash = Some(remote_hash);
            state.last_sync_at = Some(now_string());
            state.last_conflict_signature = None;
            state.last_conflict_path = None;
            Ok((state, "pulled".into()))
        }
        (Some(local_hash), Some(remote_hash), Some(previous_hash)) => {
            if local_hash == previous_hash {
                let bytes = remote
                    .bytes
                    .ok_or_else(|| "读取远端文件失败".to_string())?;
                write_local_file(&local_path, &bytes)?;
                state.status = "pulled".into();
                state.detail = "检测到远端更新，已拉取到本地".into();
                state.last_synced_hash = Some(remote_hash);
                state.last_sync_at = Some(now_string());
                state.last_conflict_signature = None;
                state.last_conflict_path = None;
                return Ok((state, "pulled".into()));
            }

            if remote_hash == previous_hash {
                let bytes = local
                    .bytes
                    .ok_or_else(|| "读取本地文件失败".to_string())?;
                client.upload_file(mapping.remote_path.trim(), bytes).await?;
                state.status = "pushed".into();
                state.detail = "检测到本地更新，已推送到远端".into();
                state.last_synced_hash = Some(local_hash);
                state.last_sync_at = Some(now_string());
                state.last_conflict_signature = None;
                state.last_conflict_path = None;
                return Ok((state, "pushed".into()));
            }

            let signature = format!("{local_hash}:{remote_hash}");
            if state.last_conflict_signature.as_deref() != Some(signature.as_str()) {
                let conflict_path = write_conflict_copy(
                    &local_path,
                    remote
                        .bytes
                        .as_deref()
                        .ok_or_else(|| "读取远端文件失败".to_string())?,
                )?;
                state.last_conflict_path = Some(conflict_path.to_string_lossy().to_string());
            }

            state.status = "conflict".into();
            state.detail = "本地和远端都发生了变化，已保留本地并生成远端冲突副本".into();
            state.last_conflict_signature = Some(signature);
            Ok((state, "conflict".into()))
        }
        (Some(local_hash), Some(remote_hash), None) => {
            let signature = format!("{local_hash}:{remote_hash}");
            if state.last_conflict_signature.as_deref() != Some(signature.as_str()) {
                let choose_remote = is_remote_newer(local.modified_at, remote.modified_at);
                if choose_remote {
                    let bytes = remote
                        .bytes
                        .as_deref()
                        .ok_or_else(|| "读取远端文件失败".to_string())?;
                    write_local_file(&local_path, bytes)?;
                    state.status = "pulled".into();
                    state.detail = "首次同步时发现远端较新，已拉取到本地".into();
                    state.last_synced_hash = Some(remote_hash);
                    state.last_sync_at = Some(now_string());
                    return Ok((state, "pulled".into()));
                }

                let conflict_path = write_conflict_copy(
                    &local_path,
                    remote
                        .bytes
                        .as_deref()
                        .ok_or_else(|| "读取远端文件失败".to_string())?,
                )?;
                state.last_conflict_path = Some(conflict_path.to_string_lossy().to_string());
            }

            state.status = "conflict".into();
            state.detail = "首次同步发现本地和远端内容不同，已保留本地并写出远端冲突副本".into();
            state.last_conflict_signature = Some(signature);
            Ok((state, "conflict".into()))
        }
    }
}

fn is_remote_newer(local: Option<DateTime<Utc>>, remote: Option<DateTime<Utc>>) -> bool {
    match (local, remote) {
        (Some(local), Some(remote)) => remote > local,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn normalize_config(mut config: AppConfig) -> AppConfig {
    config.webdav.base_url = config.webdav.base_url.trim().to_string();
    config.webdav.username = config.webdav.username.trim().to_string();
    config.webdav.remote_dir = if config.webdav.remote_dir.trim().is_empty() {
        "configs".into()
    } else {
        config.webdav.remote_dir.trim().replace('\\', "/")
    };
    config.webdav.sync_interval_secs = config.webdav.sync_interval_secs.max(10);
    config.mappings = config
        .mappings
        .into_iter()
        .filter(|item| !item.id.trim().is_empty())
        .map(|mut item| {
            item.name = item.name.trim().to_string();
            item.local_path = item.local_path.trim().to_string();
            item.remote_path = item.remote_path.trim().replace('\\', "/");
            item
        })
        .collect();
    config
}

fn default_runtime_state() -> FileRuntimeState {
    FileRuntimeState {
        status: "idle".into(),
        detail: "尚未同步".into(),
        ..Default::default()
    }
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut dir = app.path().app_config_dir().map_err(|err| err.to_string())?;
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    dir.push(STORE_FILE_NAME);
    Ok(dir)
}

fn load_persisted_state(app: &AppHandle) -> Result<PersistedStore, String> {
    let path = store_path(app)?;
    if !path.exists() {
        return Ok(PersistedStore::default());
    }

    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&content).map_err(|err| err.to_string())
}

fn persist_state(app: &AppHandle, state: &InMemoryState) -> Result<(), String> {
    let path = store_path(app)?;
    let persisted = PersistedStore {
        config: state.config.clone(),
        file_states: state.file_states.clone(),
    };
    let content = serde_json::to_string_pretty(&persisted).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())
}

fn read_local_file(path: &Path) -> Result<LocalFile, String> {
    if !path.exists() {
        return Ok(LocalFile {
            bytes: None,
            modified_at: None,
        });
    }

    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    let modified_at = fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .map(DateTime::<Utc>::from);

    Ok(LocalFile {
        bytes: Some(bytes),
        modified_at,
    })
}

fn write_local_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, bytes).map_err(|err| err.to_string())
}

fn write_conflict_copy(path: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent).map_err(|err| err.to_string())?;

    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("");
    let file_name = if extension.is_empty() {
        format!("{stem}.remote-conflict-{}", Utc::now().format("%Y%m%d-%H%M%S"))
    } else {
        format!(
            "{stem}.remote-conflict-{}.{}",
            Utc::now().format("%Y%m%d-%H%M%S"),
            extension
        )
    };

    let conflict_path = parent.join(file_name);
    fs::write(&conflict_path, bytes).map_err(|err| err.to_string())?;
    Ok(conflict_path)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn normalize_segments(input: &str) -> Vec<String> {
    input
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(|segment| segment.to_string())
        .collect()
}

fn join_remote_segments(base: &str, extra: &str) -> Vec<String> {
    let mut segments = normalize_segments(base);
    segments.extend(normalize_segments(extra));
    segments
}

fn now_string() -> String {
    Utc::now().to_rfc3339()
}

fn last_run_due(last_run_at: &Option<String>, interval_secs: u64) -> bool {
    match last_run_at {
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map(|time| Utc::now() - time.with_timezone(&Utc) >= chrono::Duration::seconds(interval_secs as i64))
            .unwrap_or(true),
        None => true,
    }
}

async fn background_sync_loop(app: AppHandle, shared: Arc<Mutex<InMemoryState>>) {
    loop {
        sleep(Duration::from_secs(5)).await;

        let should_sync = {
            let guard = shared.lock().await;
            guard.config.webdav.auto_sync
                && !guard.config.webdav.base_url.is_empty()
                && !guard.config.mappings.is_empty()
                && last_run_due(&guard.last_run_at, guard.config.webdav.sync_interval_secs)
        };

        if should_sync {
            let _ = perform_sync(app.clone(), shared.clone()).await;
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let persisted = load_persisted_state(&app.handle())?;
            app.manage(SharedState(Arc::new(Mutex::new(persisted.into()))));
            let state = app.state::<SharedState>().0.clone();
            let app_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                background_sync_loop(app_handle, state).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_app_state,
            get_runtime_state,
            save_app_config,
            sync_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
