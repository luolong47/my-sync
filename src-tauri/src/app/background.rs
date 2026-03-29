async fn sync_once_on_startup(app: AppHandle, shared: Arc<Mutex<InMemoryState>>) {
    sleep(Duration::from_millis(300)).await;

    let should_sync = {
        let guard = shared.lock().await;
        evaluate_readiness(&guard.config, &guard.file_states).0
    };

    if should_sync {
        let _ = perform_sync(app, shared).await;
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

        tauri::async_runtime::block_on(async {
            let mut guard = shared.lock().await;
            if !guard.pending_sync_paths.is_empty() {
                pending_paths.extend(guard.pending_sync_paths.drain());
                last_event_at = guard.pending_sync_started_at.take().or(Some(Instant::now()));
            }
        });

        let desired_dirs = if config.sync.fs_watch_enabled {
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

                tauri::async_runtime::block_on(async {
                    let result = perform_sync(app.clone(), shared.clone()).await;
                    if result.is_ok() {
                        let mut guard = shared.lock().await;
                        append_logs(
                            &mut guard.sync_logs,
                            vec![log_entry(LogEntryArgs {
                                mapping: None,
                                level: "info",
                                action: "fs-watch-sync",
                                summary: "文件监听触发同步",
                                detail: &format!("防抖后批量处理 {} 个本地变更事件", changed_count),
                                http_status: None,
                                local_path: None,
                                target_path: None,
                            })],
                        );
                        let _ = persist_state(&app, &guard);
                    }
                });
            }
        }
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.hide();
    }
}

fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, TRAY_MENU_SHOW_ID, "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, TRAY_MENU_QUIT_ID, "退出应用", true, None::<&str>)?;
    let tray_menu = Menu::with_items(
        app,
        &[&show_item, &PredefinedMenuItem::separator(app)?, &quit_item],
    )?;

    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ICON_ID)
        .menu(&tray_menu)
        .tooltip("My Sync")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW_ID => show_main_window(app),
            TRAY_MENU_QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray_builder = tray_builder.icon(icon);
    }

    tray_builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("app".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .max_file_size(5 * 1024 * 1024) // 5MB
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            info!("应用启动中...");
            build_tray(app.handle())?;
            let mut persisted = load_persisted_state(app.handle())?;
            sync_launch_on_boot_from_system(app.handle(), &mut persisted.config);
            app.manage(SharedState(Arc::new(Mutex::new(persisted.into()))));
            let state = app.state::<SharedState>().0.clone();
            let app_handle = app.handle().clone();
            let watch_state = state.clone();
            let watch_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                info!("执行启动首次同步...");
                sync_once_on_startup(app_handle, state).await;
            });
            std::thread::spawn(move || {
                info!("启动文件监听线程...");
                file_watch_loop(watch_handle, watch_state);
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                hide_main_window(window.app_handle());
            }
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
            create_remote_directory,
            clear_sync_logs,
            export_sync_logs,
            validate_local_file,
            save_app_config,
            sync_now,
            resolve_conflict
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
