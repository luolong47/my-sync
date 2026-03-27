#[tauri::command]
async fn load_app_state(app: AppHandle, state: State<'_, SharedState>) -> Result<AppSnapshot, String> {
    let mut config = {
        let guard = state.0.lock().await;
        guard.config.clone()
    };
    let sync_error = match sync_remote_config_and_bindings(config.clone()).await {
        Ok(next) => {
            config = next;
            None
        }
        Err(err) => Some(err),
    };
    sync_launch_on_boot_from_system(&app, &mut config);
    let mut guard = state.0.lock().await;
    guard.config = config.clone();
    if let Some(err) = sync_error {
        append_logs(
            &mut guard.sync_logs,
            vec![log_entry(LogEntryArgs {
                mapping: None,
                level: "warning",
                action: "load-remote-config",
                summary: "远端配置读取失败，已回退到本地缓存",
                detail: &err,
                http_status: None,
                local_path: None,
                target_path: None,
            })],
        );
    }
    persist_state(&app, &guard)?;
    Ok(AppSnapshot {
        config,
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
        vec![log_entry(LogEntryArgs {
            mapping: None,
            level: "info",
            action: "download",
            summary: "远端文件已下载",
            detail: &format!("{} -> {}", remote_path, save_path),
            http_status: remote.http_status,
            local_path: None,
            target_path: Some(save_path),
        })],
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
        vec![log_entry(LogEntryArgs {
            mapping: None,
            level: "info",
            action: "upload",
            summary: "本地文件已上传",
            detail: &format!("{} -> {}", local_path, remote_path),
            http_status: Some(http_status),
            local_path: Some(local_path),
            target_path: Some(remote_path),
        })],
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
        vec![log_entry(LogEntryArgs {
            mapping: None,
            level: "warning",
            action: "delete-remote",
            summary: "远端项目已删除",
            detail: &remote_path,
            http_status: None,
            local_path: None,
            target_path: Some(remote_path.clone()),
        })],
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
        vec![log_entry(LogEntryArgs {
            mapping: None,
            level: "info",
            action: "rename-remote",
            summary: "远端项目已重命名",
            detail: &format!("{} -> {}", remote_path, target_path),
            http_status: Some(http_status),
            local_path: None,
            target_path: Some(target_path),
        })],
    );
    persist_state(&app, &guard)?;
    Ok(())
}

#[tauri::command]
async fn create_remote_directory(
    app: AppHandle,
    state: State<'_, SharedState>,
    remote_dir_path: Option<String>,
    name: String,
) -> Result<(), String> {
    let config = {
        let guard = state.0.lock().await;
        guard.config.clone()
    };
    if !can_prepare_remote_root(&config) {
        return Err("请先完成 WebDAV 设置".into());
    }

    let name = name.trim().trim_matches('/');
    if name.is_empty() {
        return Err("目录名称不能为空".into());
    }

    let target_path = match remote_dir_path.as_deref() {
        Some(path) if !path.trim().is_empty() => format!("{}/{}", path.trim_matches('/'), name),
        _ => name.to_string(),
    };

    let client = WebDavClient::new(config.webdav)?;
    client.create_remote_directory(&target_path).await?;

    let mut guard = state.0.lock().await;
    append_logs(
        &mut guard.sync_logs,
        vec![log_entry(LogEntryArgs {
            mapping: None,
            level: "info",
            action: "create-remote-dir",
            summary: "远端目录已创建",
            detail: &target_path,
            http_status: None,
            local_path: None,
            target_path: Some(target_path.clone()),
        })],
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
async fn validate_local_file(path: String) -> Result<(), String> {
    let target = PathBuf::from(path.trim());
    let metadata = fs::metadata(&target).map_err(|err| err.to_string())?;
    if !metadata.is_file() {
        return Err("只能添加普通文件".into());
    }
    validate_standalone_file_size(target.to_string_lossy().as_ref(), metadata.len())
}

#[tauri::command]
async fn save_app_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: AppConfig,
) -> Result<RuntimeSnapshot, String> {
    let normalized = normalize_config(config);
    if can_prepare_remote_root(&normalized) {
        let client = WebDavClient::new(normalized.webdav.clone())?;
        client.ensure_root_collection().await?;
        client
            .save_shared_sync_items(&build_shared_sync_items(&normalized))
            .await?;
        client
            .save_device_bindings(&build_device_bindings(&normalized))
            .await?;
    }
    apply_launch_on_boot(&app, normalized.sync.launch_on_boot)?;
    let snapshot = {
        let mut guard = state.0.lock().await;
        let previous_config = guard.config.clone();
        let queued_paths = collect_new_mapping_paths(&previous_config, &normalized);
        guard.config = normalized;
        if !queued_paths.is_empty() {
            guard.pending_sync_paths.extend(queued_paths);
            guard.pending_sync_started_at = Some(Instant::now());
        }
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
                vec![log_entry(LogEntryArgs {
                    mapping: Some(&mapping),
                    level: "warning",
                    action: "resolve-conflict",
                    summary: "冲突已处理",
                    detail: &summary,
                    http_status: None,
                    local_path: None,
                    target_path: None,
                })],
            );
            persist_state(&app, &guard)?;
            Ok(runtime_snapshot(&guard))
        }
        Err(err) => {
            guard.last_summary = format!("冲突处理失败：{err}");
            append_logs(
                &mut guard.sync_logs,
                vec![log_entry(LogEntryArgs {
                    mapping: Some(&mapping),
                    level: "error",
                    action: "resolve-conflict",
                    summary: "冲突处理失败",
                    detail: &err,
                    http_status: None,
                    local_path: None,
                    target_path: None,
                })],
            );
            persist_state(&app, &guard)?;
            Err(err)
        }
    }
}

fn collect_new_mapping_paths(previous: &AppConfig, current: &AppConfig) -> Vec<String> {
    let previous_paths = previous
        .mappings
        .iter()
        .map(|item| item.local_path.trim().replace('\\', "/"))
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();

    current
        .mappings
        .iter()
        .map(|item| item.local_path.trim().replace('\\', "/"))
        .filter(|item| !item.is_empty())
        .filter(|path| !previous_paths.contains(path))
        .collect()
}

async fn sync_remote_config_and_bindings(config: AppConfig) -> Result<AppConfig, String> {
    if !can_prepare_remote_root(&config) {
        return Ok(config);
    }

    let client = WebDavClient::new(config.webdav.clone())?;
    client.ensure_root_collection().await?;
    let remote_items = client.fetch_shared_sync_items().await?;
    let remote_bindings = client.fetch_device_bindings().await?;
    let (merged_config, merged_items, merged_bindings) = sync_remote_state(
        &config,
        remote_items.clone(),
        remote_bindings.clone(),
    );
    if remote_items.as_ref() != Some(&merged_items) {
        client.save_shared_sync_items(&merged_items).await?;
    }
    if remote_bindings.as_ref() != Some(&merged_bindings) {
        client.save_device_bindings(&merged_bindings).await?;
    }
    Ok(merged_config)
}

