use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use quick_xml::de::from_str as from_xml_str;
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tokio::{sync::Mutex, time::sleep};
use url::Url;

const STORE_FILE_NAME: &str = "state.json";
const DEFAULT_REMOTE_DIR: &str = "my-sync";
const MAX_SYNC_FILE_BYTES: u64 = 1024 * 1024;
const MAX_LOG_ENTRIES: usize = 500;
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebDavSettings {
    base_url: String,
    username: String,
    password: String,
    remote_dir: String,
    client_id: String,
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
            client_id: String::new(),
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
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            default_conflict_strategy: "manual".into(),
            fs_watch_enabled: true,
            debounce_delay_secs: 15,
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
                http_status: Some(StatusCode::NOT_FOUND.as_u16()),
            });
        }

        let status = response.status().as_u16();
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
            http_status: Some(status),
        })
    }

    async fn upload_file(&self, remote_path: &str, bytes: Vec<u8>) -> Result<u16, String> {
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

        let status = response.status().as_u16();
        response.error_for_status().map_err(|err| err.to_string())?;
        Ok(status)
    }

    async fn ensure_root_collection(&self) -> Result<(), String> {
        self.ensure_collection_chain(&self.remote_root_segments())
            .await
    }

    async fn ensure_parent_collections(&self, remote_path: &str) -> Result<(), String> {
        let mut all_segments = self.remote_root_segments();
        let file_segments = normalize_segments(remote_path);

        if file_segments.len() > 1 {
            all_segments.extend(file_segments[..file_segments.len() - 1].iter().cloned());
        }

        self.ensure_collection_chain(&all_segments)
            .await
    }

    async fn list_remote_entries(&self, remote_path: &str) -> Result<Vec<RemoteBrowserEntry>, String> {
        let propfind = Method::from_bytes(b"PROPFIND").map_err(|err| err.to_string())?;
        let url = self.collection_url(&join_remote_segments_with_client(
            &self.settings.remote_dir,
            &self.settings.client_id,
            remote_path,
        ))?;
        let response = self
            .client
            .request(propfind, url.clone())
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .header("Depth", "1")
            .body(
                r#"<?xml version="1.0" encoding="utf-8"?><propfind xmlns="DAV:"><prop><displayname/><resourcetype/><getcontentlength/><getlastmodified/></prop></propfind>"#,
            )
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let response = response.error_for_status().map_err(|err| err.to_string())?;
        let xml = response.text().await.map_err(|err| err.to_string())?;
        let parsed: MultiStatus = from_xml_str(&xml).map_err(|err| err.to_string())?;
        let current_segments = join_remote_segments_with_client(
            &self.settings.remote_dir,
            &self.settings.client_id,
            remote_path,
        );
        let current_path = format!("/{}", current_segments.join("/"));

        let mut entries = Vec::new();
        for item in parsed.responses {
            let href_path = extract_href_path(&item.href)?;
            if href_path.trim_end_matches('/') == current_path.trim_end_matches('/') {
                continue;
            }

            let relative = relative_remote_path(&href_path, &current_path);
            if relative.is_empty() || relative.contains('/') && !href_path.ends_with('/') {
                // keep immediate children only
                if relative.contains('/') {
                    continue;
                }
            }

            let prop = item.propstats.into_iter().next().map(|value| value.prop);
            let display_name = prop
                .as_ref()
                .and_then(|value| value.display_name.clone())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| relative.clone());
            let is_dir = prop
                .as_ref()
                .and_then(|value| value.resource_type.as_ref())
                .and_then(|value| value.collection.as_ref())
                .is_some()
                || href_path.ends_with('/');
            let size = prop
                .as_ref()
                .and_then(|value| value.content_length.as_ref())
                .and_then(|value| value.parse::<u64>().ok());
            let modified_at = prop
                .as_ref()
                .and_then(|value| value.last_modified.as_ref())
                .and_then(|value| httpdate::parse_http_date(value).ok())
                .map(|value| DateTime::<Utc>::from(value).to_rfc3339());

            entries.push(RemoteBrowserEntry {
                name: display_name,
                path: if remote_path.trim().is_empty() {
                    relative.trim_end_matches('/').to_string()
                } else {
                    format!(
                        "{}/{}",
                        remote_path.trim_matches('/'),
                        relative.trim_end_matches('/')
                    )
                },
                is_dir,
                size,
                modified_at,
            });
        }

        entries.sort_by(|left, right| {
            left.is_dir
                .cmp(&right.is_dir)
                .reverse()
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(entries)
    }

    async fn delete_remote_entry(&self, remote_path: &str) -> Result<(), String> {
        let url = self.file_url(remote_path)?;
        let response = self
            .client
            .delete(url)
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .send()
            .await
            .map_err(|err| err.to_string())?;

        response.error_for_status().map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn move_remote_entry(
        &self,
        remote_path: &str,
        new_remote_path: &str,
        overwrite: bool,
    ) -> Result<u16, String> {
        let method = Method::from_bytes(b"MOVE").map_err(|err| err.to_string())?;
        let source_url = self.file_url(remote_path)?;
        let destination_url = self.file_url(new_remote_path)?;
        let response = self
            .client
            .request(method, source_url)
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .header("Destination", destination_url.as_str())
            .header("Overwrite", if overwrite { "T" } else { "F" })
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let status = response.status().as_u16();
        response.error_for_status().map_err(|err| err.to_string())?;
        Ok(status)
    }

    async fn ensure_collection_chain(&self, all_segments: &[String]) -> Result<(), String> {
        let mkcol = Method::from_bytes(b"MKCOL").map_err(|err| err.to_string())?;
        let mut progressive: Vec<String> = Vec::new();
        for segment in all_segments {
            progressive.push(segment.clone());
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
                    | StatusCode::CONFLICT
                    | StatusCode::MOVED_PERMANENTLY
                    | StatusCode::FOUND
            ) {
                return Err(format!("无法创建远端目录，状态码 {status}"));
            }
        }

        Ok(())
    }

    fn file_url(&self, remote_path: &str) -> Result<Url, String> {
        self.build_url_with_segments(&join_remote_segments_with_client(
            &self.settings.remote_dir,
            &self.settings.client_id,
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
            .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT_ENCODE_SET).to_string())
            .collect::<Vec<_>>()
            .join("/");

        url.set_path(&format!("/{}", encoded));
        Ok(url)
    }

    fn remote_root_segments(&self) -> Vec<String> {
        let mut segments = normalize_segments(&self.settings.remote_dir);
        if !self.settings.client_id.trim().is_empty() {
            segments.push(self.settings.client_id.trim().to_string());
        }
        segments
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
async fn get_sync_logs(state: State<'_, SharedState>) -> Result<Vec<SyncLogEntry>, String> {
    let guard = state.0.lock().await;
    Ok(guard.sync_logs.iter().rev().cloned().collect())
}

#[tauri::command]
async fn list_remote_files(
    state: State<'_, SharedState>,
    path: Option<String>,
) -> Result<Vec<RemoteBrowserEntry>, String> {
    let config = {
        let guard = state.0.lock().await;
        guard.config.clone()
    };
    if !can_prepare_remote_root(&config) {
        return Err("请先完成 WebDAV 设置".into());
    }
    let client = WebDavClient::new(config.webdav)?;
    client.list_remote_entries(path.as_deref().unwrap_or("")).await
}

#[tauri::command]
async fn download_remote_file(
    app: AppHandle,
    state: State<'_, SharedState>,
    remote_path: String,
    save_path: String,
) -> Result<(), String> {
    let config = {
        let guard = state.0.lock().await;
        guard.config.clone()
    };
    if !can_prepare_remote_root(&config) {
        return Err("请先完成 WebDAV 设置".into());
    }

    let client = WebDavClient::new(config.webdav)?;
    let remote = client.fetch_file(&remote_path).await?;
    let bytes = remote
        .bytes
        .ok_or_else(|| "远端文件不存在或不是普通文件".to_string())?;
    write_local_file(Path::new(&save_path), &bytes)?;

    let mut guard = state.0.lock().await;
    append_logs(
        &mut guard.sync_logs,
        vec![log_entry(
            None,
            "info",
            "download",
            "远端文件已下载",
            &format!("{} -> {}", remote_path, save_path),
            remote.http_status,
            None,
            Some(save_path),
        )],
    );
    persist_state(&app, &guard)?;
    Ok(())
}

#[tauri::command]
async fn upload_local_file(
    app: AppHandle,
    state: State<'_, SharedState>,
    local_path: String,
    remote_dir_path: Option<String>,
    overwrite: bool,
) -> Result<(), String> {
    let config = {
        let guard = state.0.lock().await;
        guard.config.clone()
    };
    if !can_prepare_remote_root(&config) {
        return Err("请先完成 WebDAV 设置".into());
    }

    let path = PathBuf::from(local_path.trim());
    let local = read_local_file(&path)?;
    let bytes = local
        .bytes
        .ok_or_else(|| "本地文件不存在或无法读取".to_string())?;
    validate_standalone_file_size(path.to_string_lossy().as_ref(), bytes.len() as u64)?;

    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "无法解析本地文件名".to_string())?;
    let remote_path = match remote_dir_path.as_deref() {
        Some(dir) if !dir.trim().is_empty() => format!("{}/{}", dir.trim_matches('/'), file_name),
        _ => file_name.to_string(),
    };

    let client = WebDavClient::new(config.webdav)?;
    let existing = client.fetch_file(&remote_path).await?;
    if existing.bytes.is_some() && !overwrite {
        return Err("远端已存在同名文件，请确认是否覆盖".into());
    }
    let http_status = client.upload_file(&remote_path, bytes).await?;
    let mut guard = state.0.lock().await;
    append_logs(
        &mut guard.sync_logs,
        vec![log_entry(
            None,
            "info",
            "upload",
            "本地文件已上传",
            &format!("{} -> {}", local_path, remote_path),
            Some(http_status),
            Some(local_path),
            Some(remote_path),
        )],
    );
    persist_state(&app, &guard)?;
    Ok(())
}

#[tauri::command]
async fn delete_remote_file(
    app: AppHandle,
    state: State<'_, SharedState>,
    remote_path: String,
) -> Result<(), String> {
    let config = {
        let guard = state.0.lock().await;
        guard.config.clone()
    };
    if !can_prepare_remote_root(&config) {
        return Err("请先完成 WebDAV 设置".into());
    }

    let client = WebDavClient::new(config.webdav)?;
    client.delete_remote_entry(&remote_path).await?;
    let mut guard = state.0.lock().await;
    append_logs(
        &mut guard.sync_logs,
        vec![log_entry(
            None,
            "warning",
            "delete-remote",
            "远端项目已删除",
            &remote_path,
            None,
            None,
            Some(remote_path.clone()),
        )],
    );
    persist_state(&app, &guard)?;
    Ok(())
}

#[tauri::command]
async fn rename_remote_file(
    app: AppHandle,
    state: State<'_, SharedState>,
    remote_path: String,
    new_name: String,
    overwrite: bool,
) -> Result<(), String> {
    let config = {
        let guard = state.0.lock().await;
        guard.config.clone()
    };
    if !can_prepare_remote_root(&config) {
        return Err("请先完成 WebDAV 设置".into());
    }

    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("新名称不能为空".into());
    }

    let parent = Path::new(remote_path.trim())
        .parent()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let target_path = if parent.is_empty() || parent == "." {
        new_name.to_string()
    } else {
        format!("{}/{}", parent.trim_matches('/'), new_name)
    };

    let client = WebDavClient::new(config.webdav)?;
    let existing = client.fetch_file(&target_path).await?;
    if existing.bytes.is_some() && !overwrite {
        return Err("远端已存在同名文件，请确认是否覆盖".into());
    }
    let http_status = client
        .move_remote_entry(&remote_path, &target_path, overwrite)
        .await?;

    let mut guard = state.0.lock().await;
    append_logs(
        &mut guard.sync_logs,
        vec![log_entry(
            None,
            "info",
            "rename-remote",
            "远端项目已重命名",
            &format!("{} -> {}", remote_path, target_path),
            Some(http_status),
            None,
            Some(target_path),
        )],
    );
    persist_state(&app, &guard)?;
    Ok(())
}

#[tauri::command]
async fn clear_sync_logs(app: AppHandle, state: State<'_, SharedState>) -> Result<(), String> {
    let mut guard = state.0.lock().await;
    guard.sync_logs.clear();
    persist_state(&app, &guard)
}

#[tauri::command]
async fn export_sync_logs(state: State<'_, SharedState>, path: String) -> Result<(), String> {
    let guard = state.0.lock().await;
    let content = serde_json::to_string_pretty(&guard.sync_logs).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())
}

#[tauri::command]
async fn save_app_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: AppConfig,
) -> Result<RuntimeSnapshot, String> {
    let normalized = normalize_config(config);
    if can_prepare_remote_root(&normalized) {
        WebDavClient::new(normalized.webdav.clone())?
            .ensure_root_collection()
            .await?;
    }
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

#[tauri::command]
async fn resolve_conflict(
    app: AppHandle,
    state: State<'_, SharedState>,
    mapping_id: String,
    strategy: String,
) -> Result<RuntimeSnapshot, String> {
    let strategy = parse_conflict_strategy(&strategy);
    let (config, mapping, current) = {
        let mut guard = state.0.lock().await;
        if guard.is_syncing {
            return Ok(runtime_snapshot(&guard));
        }
        guard.is_syncing = true;
        let mapping = guard
            .config
            .mappings
            .iter()
            .find(|item| item.id == mapping_id)
            .cloned()
            .ok_or_else(|| "未找到对应映射".to_string())?;
        let current = guard
            .file_states
            .get(&mapping.id)
            .cloned()
            .unwrap_or_else(default_runtime_state);
        (guard.config.clone(), mapping, current)
    };

    let result = resolve_mapping_conflict(&config, &mapping, current, strategy).await;

    let mut guard = state.0.lock().await;
    guard.is_syncing = false;
    guard.last_run_at = Some(now_string());

    match result {
        Ok((next_state, summary)) => {
            guard.file_states.insert(mapping.id.clone(), next_state);
            guard.last_summary = summary.clone();
            append_logs(
                &mut guard.sync_logs,
                vec![log_entry(
                    Some(&mapping),
                    "warning",
                    "resolve-conflict",
                    "冲突已处理",
                    &summary,
                    None,
                    None,
                    None,
                )],
            );
            persist_state(&app, &guard)?;
            Ok(runtime_snapshot(&guard))
        }
        Err(err) => {
            guard.last_summary = format!("冲突处理失败：{err}");
            append_logs(
                &mut guard.sync_logs,
                vec![log_entry(
                    Some(&mapping),
                    "error",
                    "resolve-conflict",
                    "冲突处理失败",
                    &err,
                    None,
                    None,
                    None,
                )],
            );
            persist_state(&app, &guard)?;
            Err(err)
        }
    }
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
    let (is_ready, readiness_detail) = evaluate_readiness(&state.config, &state.file_states);
    let conflict_count = mappings.iter().filter(|item| item.status == "conflict").count();
    let error_count = mappings.iter().filter(|item| item.status == "error").count();

    RuntimeSnapshot {
        is_syncing: state.is_syncing,
        is_ready,
        readiness_detail,
        conflict_count,
        error_count,
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
            append_logs(&mut guard.sync_logs, result.logs);
            persist_state(&app, &guard)?;
            Ok(runtime_snapshot(&guard))
        }
        Err(err) => {
            guard.last_summary = format!("同步失败：{err}");
            append_logs(
                &mut guard.sync_logs,
                vec![log_entry(None, "error", "sync-run", "同步任务失败", &err, None, None, None)],
            );
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
    client.ensure_root_collection().await?;
    let strategy = parse_conflict_strategy(&config.sync.default_conflict_strategy);
    let mut success_count = 0usize;
    let mut conflict_count = 0usize;
    let mut error_count = 0usize;
    let mut logs = vec![log_entry(
        None,
        "info",
        "sync-run",
        "开始同步",
        &format!("共 {} 个映射待处理", config.mappings.len()),
        None,
        None,
        None,
    )];

    for mapping in &config.mappings {
        let current = file_states
            .get(&mapping.id)
            .cloned()
            .unwrap_or_else(default_runtime_state);

        match sync_single_mapping(mapping, current.clone(), &client, strategy).await {
            Ok((next_state, status_kind, detail)) => {
                if matches!(status_kind.as_str(), "synced" | "pushed" | "pulled" | "resolved") {
                    success_count += 1;
                }
                if status_kind == "conflict" {
                    conflict_count += 1;
                }
                logs.push(log_entry(
                    Some(mapping),
                    if status_kind == "conflict" { "warning" } else { "info" },
                    &status_kind,
                    "映射同步完成",
                    &detail,
                    None,
                    None,
                    None,
                ));
                file_states.insert(mapping.id.clone(), next_state);
            }
            Err(err) => {
                error_count += 1;
                logs.push(log_entry(
                    Some(mapping),
                    "error",
                    "mapping-error",
                    "映射同步失败",
                    &err,
                    None,
                    None,
                    None,
                ));
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

    let summary = format!(
        "成功 {} 项，冲突 {} 项，错误 {} 项",
        success_count, conflict_count, error_count
    );
    logs.push(log_entry(None, "info", "sync-run", "同步完成", &summary, None, None, None));

    Ok(SyncOutcome {
        file_states,
        summary,
        logs,
    })
}

async fn sync_single_mapping(
    mapping: &FileMapping,
    mut state: FileRuntimeState,
    client: &WebDavClient,
    strategy: ConflictStrategy,
) -> Result<(FileRuntimeState, String, String), String> {
    let local_path = PathBuf::from(mapping.local_path.trim());
    let local = read_local_file(&local_path)?;
    validate_local_file_size(mapping, &local)?;
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
            return Ok((state, "synced".into(), "本地与远端一致".into()));
        }
    }

    match (local_hash.clone(), remote_hash.clone(), previous_hash.clone()) {
        (None, None, _) => {
            state.status = "idle".into();
            state.detail = "本地和远端都不存在该文件".into();
            Ok((state, "idle".into(), "本地和远端都不存在该文件".into()))
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
            Ok((state, "pushed".into(), "远端不存在文件，已上传本地版本".into()))
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
            Ok((state, "pulled".into(), "本地不存在文件，已从远端恢复".into()))
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
                return Ok((state, "pulled".into(), "检测到远端更新，已拉取到本地".into()));
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
                return Ok((state, "pushed".into(), "检测到本地更新，已推送到远端".into()));
            }

            resolve_conflict_state(
                mapping,
                state,
                client,
                local_path,
                local.bytes,
                remote.bytes,
                local_hash,
                remote_hash,
                strategy,
                false,
            )
            .await
        }
        (Some(local_hash), Some(remote_hash), None) => {
            if strategy == ConflictStrategy::Manual && is_remote_newer(local.modified_at, remote.modified_at) {
                let bytes = remote
                    .bytes
                    .as_deref()
                    .ok_or_else(|| "读取远端文件失败".to_string())?;
                write_local_file(&local_path, bytes)?;
                state.status = "pulled".into();
                state.detail = "首次同步时发现远端较新，已拉取到本地".into();
                state.last_synced_hash = Some(remote_hash);
                state.last_sync_at = Some(now_string());
                return Ok((state, "pulled".into(), "首次同步发现远端较新，已拉取到本地".into()));
            }

            resolve_conflict_state(
                mapping,
                state,
                client,
                local_path,
                local.bytes,
                remote.bytes,
                local_hash,
                remote_hash,
                strategy,
                true,
            )
            .await
        }
    }
}

async fn resolve_mapping_conflict(
    config: &AppConfig,
    mapping: &FileMapping,
    state: FileRuntimeState,
    strategy: ConflictStrategy,
) -> Result<(FileRuntimeState, String), String> {
    let client = WebDavClient::new(config.webdav.clone())?;
    let local_path = PathBuf::from(mapping.local_path.trim());
    let local = read_local_file(&local_path)?;
    validate_local_file_size(mapping, &local)?;
    let remote = client.fetch_file(mapping.remote_path.trim()).await?;
    let local_hash = local
        .bytes
        .as_ref()
        .map(|bytes| hash_bytes(bytes))
        .ok_or_else(|| "本地文件不存在".to_string())?;
    let remote_hash = remote
        .bytes
        .as_ref()
        .map(|bytes| hash_bytes(bytes))
        .ok_or_else(|| "远端文件不存在".to_string())?;

    let (next_state, _, detail) = resolve_conflict_state(
        mapping,
        state,
        &client,
        local_path,
        local.bytes,
        remote.bytes,
        local_hash,
        remote_hash,
        strategy,
        false,
    )
    .await?;

    let next_state = match strategy {
        ConflictStrategy::Manual => next_state,
        _ => next_state,
    };

    Ok((next_state, detail))
}

async fn resolve_conflict_state(
    mapping: &FileMapping,
    mut state: FileRuntimeState,
    client: &WebDavClient,
    local_path: PathBuf,
    local_bytes: Option<Vec<u8>>,
    remote_bytes: Option<Vec<u8>>,
    local_hash: String,
    remote_hash: String,
    strategy: ConflictStrategy,
    is_initial: bool,
) -> Result<(FileRuntimeState, String, String), String> {
    let signature = format!("{local_hash}:{remote_hash}");

    if strategy == ConflictStrategy::Local {
        let bytes = local_bytes.ok_or_else(|| "读取本地文件失败".to_string())?;
        client.upload_file(mapping.remote_path.trim(), bytes).await?;
        state.status = "resolved".into();
        state.detail = "冲突已按本地版本解决".into();
        state.last_synced_hash = Some(local_hash);
        state.last_sync_at = Some(now_string());
        state.last_conflict_signature = None;
        state.last_conflict_path = None;
        return Ok((state, "resolved".into(), "冲突已按本地版本解决".into()));
    }

    if strategy == ConflictStrategy::Remote {
        let bytes = remote_bytes
            .as_deref()
            .ok_or_else(|| "读取远端文件失败".to_string())?;
        write_local_file(&local_path, bytes)?;
        state.status = "resolved".into();
        state.detail = "冲突已按远端版本解决".into();
        state.last_synced_hash = Some(remote_hash);
        state.last_sync_at = Some(now_string());
        state.last_conflict_signature = None;
        state.last_conflict_path = None;
        return Ok((state, "resolved".into(), "冲突已按远端版本解决".into()));
    }

    if state.last_conflict_signature.as_deref() != Some(signature.as_str()) {
        let conflict_path = write_conflict_copy(
            &local_path,
            remote_bytes
                .as_deref()
                .ok_or_else(|| "读取远端文件失败".to_string())?,
        )?;
        state.last_conflict_path = Some(conflict_path.to_string_lossy().to_string());
    }

    state.status = "conflict".into();
    state.detail = if is_initial {
        "首次同步发现本地和远端内容不同，已保留本地并写出远端冲突副本".into()
    } else {
        "本地和远端都发生了变化，已保留本地并生成远端冲突副本".into()
    };
    state.last_conflict_signature = Some(signature);
    Ok((state, "conflict".into(), "检测到冲突，已生成远端冲突副本".into()))
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
    config.webdav.client_id = if config.webdav.client_id.trim().is_empty() {
        generate_client_id()
    } else {
        config.webdav.client_id.trim().to_string()
    };
    config.webdav.remote_dir = if config.webdav.remote_dir.trim().is_empty() {
        DEFAULT_REMOTE_DIR.into()
    } else {
        config.webdav.remote_dir.trim().replace('\\', "/")
    };
    config.webdav.sync_interval_secs = config.webdav.sync_interval_secs.max(10);
    config.sync.default_conflict_strategy = match parse_conflict_strategy(&config.sync.default_conflict_strategy) {
        ConflictStrategy::Local => "local".into(),
        ConflictStrategy::Remote => "remote".into(),
        ConflictStrategy::Manual => "manual".into(),
    };
    config.sync.debounce_delay_secs = config.sync.debounce_delay_secs.clamp(3, 60);
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

fn parse_conflict_strategy(value: &str) -> ConflictStrategy {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => ConflictStrategy::Local,
        "remote" => ConflictStrategy::Remote,
        _ => ConflictStrategy::Manual,
    }
}

fn can_prepare_remote_root(config: &AppConfig) -> bool {
    !config.webdav.base_url.is_empty()
        && !config.webdav.username.is_empty()
        && !config.webdav.remote_dir.is_empty()
        && !config.webdav.client_id.is_empty()
}

fn evaluate_readiness(
    config: &AppConfig,
    file_states: &HashMap<String, FileRuntimeState>,
) -> (bool, String) {
    if config.webdav.base_url.trim().is_empty() {
        return (false, "缺少 WebDAV 服务地址".into());
    }
    if config.webdav.username.trim().is_empty() {
        return (false, "缺少 WebDAV 用户名".into());
    }
    if config.mappings.is_empty() {
        return (false, "还没有配置文件映射".into());
    }

    for mapping in &config.mappings {
        if mapping.local_path.trim().is_empty() {
            return (false, format!("{} 缺少本地路径", mapping.name));
        }
        if mapping.remote_path.trim().is_empty() {
            return (false, format!("{} 缺少远端路径", mapping.name));
        }
        let path = Path::new(&mapping.local_path);
        if !path.exists() {
            return (false, format!("{} 的本地文件不存在", mapping.name));
        }
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > MAX_SYNC_FILE_BYTES {
                return (false, format!("{} 超过 1MB 限制", mapping.name));
            }
        }
    }

    let conflicts = file_states.values().filter(|item| item.status == "conflict").count();
    if conflicts > 0 {
        return (false, format!("有 {} 个冲突待处理", conflicts));
    }

    (true, "配置完整，本地文件有效，可开始同步".into())
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
        sync_logs: state.sync_logs.clone(),
    };
    let content = serde_json::to_string_pretty(&persisted).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())
}

fn read_local_file(path: &Path) -> Result<LocalFile, String> {
    if !path.exists() {
        return Ok(LocalFile {
            bytes: None,
            modified_at: None,
            size_bytes: None,
        });
    }

    let metadata = fs::metadata(path).map_err(|err| err.to_string())?;
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    let modified_at = metadata.modified().ok().map(DateTime::<Utc>::from);

    Ok(LocalFile {
        bytes: Some(bytes),
        modified_at,
        size_bytes: Some(metadata.len()),
    })
}

fn validate_local_file_size(mapping: &FileMapping, local: &LocalFile) -> Result<(), String> {
    if let Some(size_bytes) = local.size_bytes {
        validate_standalone_file_size(
            if mapping.name.is_empty() {
                mapping.local_path.as_str()
            } else {
                mapping.name.as_str()
            },
            size_bytes,
        )?;
    }

    Ok(())
}

fn validate_standalone_file_size(display_name: &str, size_bytes: u64) -> Result<(), String> {
    if size_bytes > MAX_SYNC_FILE_BYTES {
        return Err(format!(
            "{} 超过 1MB 限制，当前大小 {:.2} KB",
            display_name,
            size_bytes as f64 / 1024.0
        ));
    }

    Ok(())
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

fn join_remote_segments_with_client(base: &str, client_id: &str, extra: &str) -> Vec<String> {
    let mut segments = normalize_segments(base);
    if !client_id.trim().is_empty() {
        segments.push(client_id.trim().to_string());
    }
    segments.extend(normalize_segments(extra));
    segments
}

fn extract_href_path(href: &str) -> Result<String, String> {
    if let Ok(url) = Url::parse(href) {
        return Ok(url.path().to_string());
    }
    Ok(href.to_string())
}

fn relative_remote_path(full_path: &str, current_path: &str) -> String {
    let normalized_full = full_path.trim_matches('/');
    let normalized_current = current_path.trim_matches('/');
    let relative = normalized_full
        .strip_prefix(normalized_current)
        .unwrap_or(normalized_full)
        .trim_matches('/');
    relative.to_string()
}

fn now_string() -> String {
    Utc::now().to_rfc3339()
}

fn generate_client_id() -> String {
    format!("client-{}", Utc::now().format("%Y%m%d%H%M%S"))
}

fn log_entry(
    mapping: Option<&FileMapping>,
    level: &str,
    action: &str,
    summary: &str,
    detail: &str,
    http_status: Option<u16>,
    local_path: Option<String>,
    target_path: Option<String>,
) -> SyncLogEntry {
    SyncLogEntry {
        id: format!("log-{}", Utc::now().timestamp_micros()),
        timestamp: now_string(),
        level: level.into(),
        action: action.into(),
        summary: summary.into(),
        detail: detail.into(),
        http_status,
        mapping_id: mapping.map(|item| item.id.clone()),
        mapping_name: mapping.map(|item| item.name.clone()),
        local_path: local_path.or_else(|| mapping.map(|item| item.local_path.clone())),
        remote_path: mapping.map(|item| item.remote_path.clone()),
        target_path,
    }
}

fn append_logs(target: &mut Vec<SyncLogEntry>, mut logs: Vec<SyncLogEntry>) {
    target.append(&mut logs);
    if target.len() > MAX_LOG_ENTRIES {
        let remove_count = target.len() - MAX_LOG_ENTRIES;
        target.drain(0..remove_count);
    }
}

fn last_run_due(last_run_at: &Option<String>, interval_secs: u64) -> bool {
    match last_run_at {
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map(|time| Utc::now() - time.with_timezone(&Utc) >= chrono::Duration::seconds(interval_secs as i64))
            .unwrap_or(true),
        None => true,
    }
}

fn watched_directories(config: &AppConfig) -> Vec<PathBuf> {
    let mut dirs = HashSet::new();
    for mapping in &config.mappings {
        let path = PathBuf::from(mapping.local_path.trim());
        if let Some(parent) = path.parent() {
            dirs.insert(parent.to_path_buf());
        }
    }
    dirs.into_iter().collect()
}

fn is_watched_path(config: &AppConfig, path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    config.mappings.iter().any(|mapping| {
        let target = PathBuf::from(mapping.local_path.trim())
            .to_string_lossy()
            .replace('\\', "/");
        normalized == target || normalized.starts_with(&format!("{target}."))
    })
}

async fn background_sync_loop(app: AppHandle, shared: Arc<Mutex<InMemoryState>>) {
    loop {
        sleep(Duration::from_secs(5)).await;

        let should_sync = {
            let guard = shared.lock().await;
            guard.config.webdav.auto_sync
                && !guard.config.sync.fs_watch_enabled
                && !guard.config.webdav.base_url.is_empty()
                && !guard.config.mappings.is_empty()
                && last_run_due(&guard.last_run_at, guard.config.webdav.sync_interval_secs)
        };

        if should_sync {
            let _ = perform_sync(app.clone(), shared.clone()).await;
        }
    }
}

fn file_watch_loop(app: AppHandle, shared: Arc<Mutex<InMemoryState>>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(
        move |result| {
            let _ = tx.send(result);
        },
        NotifyConfig::default(),
    ) {
        Ok(value) => value,
        Err(_) => return,
    };
    let mut watched = HashSet::<PathBuf>::new();
    let mut pending_paths = HashSet::<String>::new();
    let mut last_event_at: Option<Instant> = None;

    loop {
        let config = tauri::async_runtime::block_on(async {
            let guard = shared.lock().await;
            guard.config.clone()
        });

        let desired_dirs = if config.webdav.auto_sync && config.sync.fs_watch_enabled {
            watched_directories(&config)
        } else {
            Vec::new()
        };

        let new_dirs = desired_dirs
            .iter()
            .filter(|dir| !watched.contains(*dir))
            .cloned()
            .collect::<Vec<_>>();
        for dir in new_dirs {
            let _ = watcher.watch(&dir, RecursiveMode::NonRecursive);
            watched.insert(dir);
        }

        let stale_dirs = watched
            .iter()
            .filter(|dir| !desired_dirs.contains(*dir))
            .cloned()
            .collect::<Vec<_>>();
        for dir in stale_dirs {
            let _ = watcher.unwatch(&dir);
            watched.remove(&dir);
        }

        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(event)) => {
                for path in event.paths {
                    if is_watched_path(&config, &path) {
                        pending_paths.insert(path.to_string_lossy().to_string());
                        last_event_at = Some(Instant::now());
                    }
                }
            }
            Ok(Err(_)) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if let Some(last) = last_event_at {
            if !pending_paths.is_empty()
                && last.elapsed() >= Duration::from_secs(config.sync.debounce_delay_secs.max(3))
            {
                let changed_count = pending_paths.len();
                pending_paths.clear();
                last_event_at = None;

                let _ = tauri::async_runtime::block_on(async {
                    let result = perform_sync(app.clone(), shared.clone()).await;
                    if result.is_ok() {
                        let mut guard = shared.lock().await;
                        append_logs(
                            &mut guard.sync_logs,
                            vec![log_entry(
                                None,
                                "info",
                                "fs-watch-sync",
                                "文件监听触发同步",
                                &format!("防抖后批量处理 {} 个本地变更事件", changed_count),
                                None,
                                None,
                                None,
                            )],
                        );
                        let _ = persist_state(&app, &guard);
                    }
                });
            }
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
            let watch_state = state.clone();
            let watch_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                background_sync_loop(app_handle, state).await;
            });
            std::thread::spawn(move || {
                file_watch_loop(watch_handle, watch_state);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_app_state,
            get_runtime_state,
            get_sync_logs,
            list_remote_files,
            download_remote_file,
            upload_local_file,
            delete_remote_file,
            rename_remote_file,
            clear_sync_logs,
            export_sync_logs,
            save_app_config,
            sync_now,
            resolve_conflict
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
