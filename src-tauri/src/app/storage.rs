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

fn sync_launch_on_boot_from_system(app: &AppHandle, config: &mut AppConfig) {
    #[cfg(desktop)]
    if let Ok(enabled) = app.autolaunch().is_enabled() {
        config.sync.launch_on_boot = enabled;
    }
}

fn apply_launch_on_boot(app: &AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(desktop)]
    {
        let manager = app.autolaunch();
        if enabled {
            manager.enable().map_err(|err| err.to_string())?;
        } else {
            manager.disable().map_err(|err| err.to_string())?;
        }
    }

    Ok(())
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

struct LogEntryArgs<'a> {
    mapping: Option<&'a FileMapping>,
    level: &'a str,
    action: &'a str,
    summary: &'a str,
    detail: &'a str,
    http_status: Option<u16>,
    local_path: Option<String>,
    target_path: Option<String>,
}

fn log_entry(args: LogEntryArgs<'_>) -> SyncLogEntry {
    SyncLogEntry {
        id: format!("log-{}", Utc::now().timestamp_micros()),
        timestamp: now_string(),
        level: args.level.into(),
        action: args.action.into(),
        summary: args.summary.into(),
        detail: args.detail.into(),
        http_status: args.http_status,
        mapping_id: args.mapping.map(|item| item.id.clone()),
        mapping_name: args.mapping.map(|item| item.name.clone()),
        local_path: args
            .local_path
            .or_else(|| args.mapping.map(|item| item.local_path.clone())),
        remote_path: args.mapping.map(|item| item.remote_path.clone()),
        target_path: args.target_path,
    }
}

fn append_logs(target: &mut Vec<SyncLogEntry>, mut logs: Vec<SyncLogEntry>) {
    target.append(&mut logs);
    if target.len() > MAX_LOG_ENTRIES {
        let remove_count = target.len() - MAX_LOG_ENTRIES;
        target.drain(0..remove_count);
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

