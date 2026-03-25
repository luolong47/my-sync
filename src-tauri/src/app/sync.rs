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
                vec![log_entry(LogEntryArgs {
                    mapping: None,
                    level: "error",
                    action: "sync-run",
                    summary: "同步任务失败",
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
    let mut logs = vec![log_entry(LogEntryArgs {
        mapping: None,
        level: "info",
        action: "sync-run",
        summary: "开始同步",
        detail: &format!("共 {} 个映射待处理", config.mappings.len()),
        http_status: None,
        local_path: None,
        target_path: None,
    })];

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
                logs.push(log_entry(LogEntryArgs {
                    mapping: Some(mapping),
                    level: if status_kind == "conflict" {
                        "warning"
                    } else {
                        "info"
                    },
                    action: &status_kind,
                    summary: "映射同步完成",
                    detail: &detail,
                    http_status: None,
                    local_path: None,
                    target_path: None,
                }));
                file_states.insert(mapping.id.clone(), next_state);
            }
            Err(err) => {
                error_count += 1;
                logs.push(log_entry(LogEntryArgs {
                    mapping: Some(mapping),
                    level: "error",
                    action: "mapping-error",
                    summary: "映射同步失败",
                    detail: &err,
                    http_status: None,
                    local_path: None,
                    target_path: None,
                }));
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
    logs.push(log_entry(LogEntryArgs {
        mapping: None,
        level: "info",
        action: "sync-run",
        summary: "同步完成",
        detail: &summary,
        http_status: None,
        local_path: None,
        target_path: None,
    }));

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

            resolve_conflict_state(ConflictResolutionContext {
                mapping,
                state,
                client,
                local_path,
                local_bytes: local.bytes,
                remote_bytes: remote.bytes,
                local_hash,
                remote_hash,
                strategy,
                is_initial: false,
            })
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

            resolve_conflict_state(ConflictResolutionContext {
                mapping,
                state,
                client,
                local_path,
                local_bytes: local.bytes,
                remote_bytes: remote.bytes,
                local_hash,
                remote_hash,
                strategy,
                is_initial: true,
            })
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

    let (next_state, _, detail) = resolve_conflict_state(ConflictResolutionContext {
        mapping,
        state,
        client: &client,
        local_path,
        local_bytes: local.bytes,
        remote_bytes: remote.bytes,
        local_hash,
        remote_hash,
        strategy,
        is_initial: false,
    })
    .await?;

    let next_state = match strategy {
        ConflictStrategy::Manual => next_state,
        _ => next_state,
    };

    Ok((next_state, detail))
}

async fn resolve_conflict_state(
    context: ConflictResolutionContext<'_>,
) -> Result<(FileRuntimeState, String, String), String> {
    let ConflictResolutionContext {
        mapping,
        mut state,
        client,
        local_path,
        local_bytes,
        remote_bytes,
        local_hash,
        remote_hash,
        strategy,
        is_initial,
    } = context;
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

struct ConflictResolutionContext<'a> {
    mapping: &'a FileMapping,
    state: FileRuntimeState,
    client: &'a WebDavClient,
    local_path: PathBuf,
    local_bytes: Option<Vec<u8>>,
    remote_bytes: Option<Vec<u8>>,
    local_hash: String,
    remote_hash: String,
    strategy: ConflictStrategy,
    is_initial: bool,
}
