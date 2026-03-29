import { computed, reactive, ref } from "vue";
import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppConfig,
  AppSnapshot,
  FileMapping,
  MappingRuntime,
  RemoteBrowserEntry,
  RuntimeSnapshot,
  SyncLogEntry,
  TabKey,
} from "../types/app";
import { useSyncResourceActions } from "../composables/useSyncResourceActions";
import {
  cloneConfig,
  createDefaultConfig,
  createDefaultRuntime,
  createEmptyRuntimeStatus,
  createMapping,
  createMappingFromPath,
  formatDateTime,
  fullRemotePath as resolveFullRemotePath,
  inferPathTemplateFromLocal,
  inferRemotePathFromLocal,
  notify,
  statusTone,
} from "../utils/appState";

// eslint-disable-next-line max-lines-per-function
export const useSyncAppStore = defineStore("sync-app", () => {
  const currentTab = ref<TabKey>("dashboard");
  const leftDrawerOpen = ref(false);
  const isAutoSaving = ref(false);
  const isBooting = ref(true);
  const isDarkMode = ref(false);
  const isLoadingRemote = ref(false);
  const isMappingDropActive = ref(false);
  const logs = ref<SyncLogEntry[]>([]);
  const remoteEntries = ref<RemoteBrowserEntry[]>([]);
  const remotePath = ref("");
  const config = reactive<AppConfig>(createDefaultConfig());
  const runtime = ref<RuntimeSnapshot>(createDefaultRuntime());

  const runtimeById = computed(
    () => new Map(runtime.value.mappings.map((item) => [item.mappingId, item])),
  );
  const conflictItems = computed(() =>
    config.mappings.filter((item) => runtimeById.value.get(item.id)?.status === "conflict"),
  );
  const recentLogs = computed(() => logs.value.slice(0, 6));
  const remoteRootPath = computed(() =>
    [config.webdav.remoteDir.trim(), config.webdav.spaceId.trim()].filter(Boolean).join("/"),
  );
  const remotePathLabel = computed(() =>
    remotePath.value ? `${remoteRootPath.value}/${remotePath.value}` : remoteRootPath.value,
  );
  const canSync = computed(
    () =>
      !!config.webdav.baseUrl.trim() &&
      !!config.webdav.username.trim() &&
      config.mappings.length > 0,
  );
  const canBrowseRemote = computed(
    () =>
      !!config.webdav.baseUrl.trim() &&
      !!config.webdav.username.trim() &&
      !!config.webdav.remoteDir.trim() &&
      !!config.webdav.spaceId.trim(),
  );

  let refreshTimer: number | undefined;
  let refreshLogsTimer: number | undefined;
  let autoSaveTimer: number | undefined;
  let autoSaveEnabled = false;
  let unlistenDragDrop: (() => void) | undefined;
  let lastSavedConfig: AppConfig | null = null;

  function applyTheme(next: boolean) {
    isDarkMode.value = next;
    document.documentElement.classList.toggle("dark", next);
    window.localStorage.setItem("my-sync-theme", next ? "dark" : "light");
  }

  function fullRemotePath(path: string) {
    return resolveFullRemotePath(remoteRootPath.value, path);
  }

  function runtimeStatus(id: string): MappingRuntime {
    return runtimeById.value.get(id) ?? createEmptyRuntimeStatus(id);
  }

  function snapshotConfig(): AppConfig {
    return {
      webdav: {
        baseUrl: config.webdav.baseUrl.trim(),
        username: config.webdav.username.trim(),
        password: config.webdav.password,
        remoteDir: config.webdav.remoteDir.trim(),
        spaceId: config.webdav.spaceId.trim(),
        deviceId: config.webdav.deviceId.trim(),
        deviceName: config.webdav.deviceName.trim(),
        syncIntervalSecs: Number(config.webdav.syncIntervalSecs) || 30,
        autoSync: true,
      },
      sync: {
        defaultConflictStrategy: config.sync.defaultConflictStrategy,
        fsWatchEnabled: !!config.sync.fsWatchEnabled,
        debounceDelaySecs: Number(config.sync.debounceDelaySecs) || 15,
        launchOnBoot: !!config.sync.launchOnBoot,
      },
      mappings: config.mappings.map((item) => ({
        id: item.id,
        name: item.name.trim(),
        localPath: item.localPath.trim(),
        remotePath: item.remotePath.trim().replace(/\\/g, "/"),
        pathTemplate: item.pathTemplate.trim().replace(/\\/g, "/"),
        bindingStatus: item.bindingStatus || (item.localPath.trim() ? "bound" : "pending_bind"),
      })),
    };
  }

  function resetConfig(next: AppConfig) {
    const normalized = cloneConfig(next);
    config.webdav.baseUrl = normalized.webdav.baseUrl ?? "";
    config.webdav.username = normalized.webdav.username ?? "";
    config.webdav.password = normalized.webdav.password ?? "";
    config.webdav.remoteDir = normalized.webdav.remoteDir ?? "my-sync";
    config.webdav.spaceId = normalized.webdav.spaceId ?? "default";
    config.webdav.deviceId = normalized.webdav.deviceId ?? "";
    config.webdav.deviceName = normalized.webdav.deviceName ?? "";
    config.webdav.syncIntervalSecs = normalized.webdav.syncIntervalSecs ?? 30;
    config.webdav.autoSync = true;
    config.sync.defaultConflictStrategy = normalized.sync.defaultConflictStrategy ?? "manual";
    config.sync.fsWatchEnabled = normalized.sync.fsWatchEnabled ?? true;
    config.sync.debounceDelaySecs = normalized.sync.debounceDelaySecs ?? 15;
    config.sync.launchOnBoot = normalized.sync.launchOnBoot ?? false;
    config.mappings.splice(
      0,
      config.mappings.length,
      ...normalized.mappings.map((item) => ({
        ...item,
        bindingStatus: item.bindingStatus || (item.localPath?.trim() ? "bound" : "pending_bind"),
      })),
    );
  }

  async function validateLocalFilePath(localPath: string) {
    await invoke("validate_local_file", { path: localPath });
  }

  async function loadAppState() {
    const snapshot = await invoke<AppSnapshot>("load_app_state");
    resetConfig(snapshot.config);
    lastSavedConfig = cloneConfig(snapshot.config);
    runtime.value = snapshot.runtime;
  }

  async function loadLogs() {
    logs.value = await invoke<SyncLogEntry[]>("get_sync_logs");
  }

  async function loadRemoteFiles(path = "") {
    if (!canBrowseRemote.value) {
      remoteEntries.value = [];
      remotePath.value = "";
      return;
    }

    isLoadingRemote.value = true;
    try {
      remoteEntries.value = await invoke<RemoteBrowserEntry[]>("list_remote_files", {
        path: path || null,
      });
      remotePath.value = path;
    } catch (error) {
      notify("negative", `读取远端文件失败：${String(error)}`);
    } finally {
      isLoadingRemote.value = false;
    }
  }

  async function refreshRuntime() {
    const previousLastRunAt = runtime.value.lastRunAt;
    const nextRuntime = await invoke<RuntimeSnapshot>("get_runtime_state");
    runtime.value = nextRuntime;

    if (nextRuntime.lastRunAt && nextRuntime.lastRunAt !== previousLastRunAt) {
      await loadLogs();

      if (refreshLogsTimer) {
        window.clearTimeout(refreshLogsTimer);
      }

      refreshLogsTimer = window.setTimeout(() => {
        void loadLogs();
      }, 300);
    }
  }

  async function persistConfig() {
    const nextConfig = snapshotConfig();
    const currentSaved = lastSavedConfig ?? snapshotConfig();
    if (JSON.stringify(nextConfig) === JSON.stringify(currentSaved)) {
      return;
    }

    isAutoSaving.value = true;
    try {
      const snapshot = await invoke<AppSnapshot>("save_app_config", {
        config: nextConfig,
      });
      resetConfig(snapshot.config);
      lastSavedConfig = cloneConfig(snapshot.config);
      runtime.value = snapshot.runtime;
      if (canBrowseRemote.value) {
        await loadRemoteFiles(remotePath.value);
      }
    } catch (error) {
      notify("negative", `自动保存失败：${String(error)}`);
    } finally {
      isAutoSaving.value = false;
    }
  }

  function schedulePersistConfig() {
    if (!autoSaveEnabled) {
      return;
    }

    if (autoSaveTimer) {
      window.clearTimeout(autoSaveTimer);
    }

    autoSaveTimer = window.setTimeout(() => {
      void persistConfig();
    }, 500);
  }

  function handleConfigUpdated() {
    schedulePersistConfig();
  }

  async function addMappingsFromPaths(paths: string[]) {
    const normalizedPaths = paths
      .map((item) => item.trim())
      .filter(Boolean)
      .filter((item) => !config.mappings.some((mapping) => mapping.localPath === item));

    if (normalizedPaths.length === 0) {
      notify("warning", "拖入的文件已存在于映射列表中");
      return;
    }

    const acceptedPaths: string[] = [];
    const rejectedMessages: string[] = [];

    for (const path of normalizedPaths) {
      try {
        await validateLocalFilePath(path);
        acceptedPaths.push(path);
      } catch (error) {
        rejectedMessages.push(`${path}：${String(error)}`);
      }
    }

    if (acceptedPaths.length > 0) {
      config.mappings.push(...acceptedPaths.map((item) => createMappingFromPath(item)));
      schedulePersistConfig();
      notify("positive", `已添加 ${acceptedPaths.length} 个文件映射`);
    }

    if (rejectedMessages.length > 0) {
      notify("negative", `以下文件未添加：${rejectedMessages.join("；")}`);
    }
  }

  function addMapping() {
    config.mappings.push(createMapping());
    schedulePersistConfig();
  }

  function removeMapping(id: string) {
    const index = config.mappings.findIndex((item) => item.id === id);
    if (index >= 0) {
      config.mappings.splice(index, 1);
      schedulePersistConfig();
    }
  }

  function clearBinding(id: string) {
    const target = config.mappings.find((item) => item.id === id);
    if (!target) {
      return;
    }

    target.localPath = "";
    target.bindingStatus = "pending_bind";
    schedulePersistConfig();
  }

  async function chooseFile(mapping?: FileMapping) {
    const selected = await open({
      multiple: false,
      directory: false,
      title: mapping ? "修改本地落点" : "选择要新增同步的配置文件",
    });

    if (typeof selected !== "string") {
      return;
    }

    // 如果没有传入 mapping，说明是“新增”场景，复用 addMappingsFromPaths 逻辑
    if (!mapping) {
      await addMappingsFromPaths([selected]);
      return;
    }

    try {
      await validateLocalFilePath(selected);
    } catch (error) {
      notify("negative", `文件不可添加：${String(error)}`);
      return;
    }

    mapping.localPath = selected;
    const normalized = selected.replace(/\\/g, "/");
    const filename = normalized.split("/").pop() || "config";
    if (!mapping.name.trim()) {
      mapping.name = filename;
    }
    if (!mapping.remotePath.trim()) {
      mapping.remotePath = inferRemotePathFromLocal(selected);
    }
    if (!mapping.pathTemplate.trim()) {
      mapping.pathTemplate = inferPathTemplateFromLocal(selected);
    }
    mapping.bindingStatus = "bound";
    schedulePersistConfig();
  }

  async function mapRemoteEntryToLocal(entry: RemoteBrowserEntry) {
    if (entry.isDir) {
      notify("warning", "目录不能直接创建文件映射");
      return;
    }

    if (config.mappings.some((item) => item.remotePath === entry.path)) {
      notify("warning", "该远端文件已存在映射");
      return;
    }

    const selected = await open({
      multiple: false,
      directory: false,
      title: `选择要绑定到 ${entry.name} 的本地文件`,
    });

    if (typeof selected !== "string") {
      return;
    }

    try {
      await validateLocalFilePath(selected);
    } catch (error) {
      notify("negative", `文件不可添加：${String(error)}`);
      return;
    }

    if (config.mappings.some((item) => item.localPath === selected)) {
      notify("warning", "该本地文件已存在映射");
      return;
    }

    config.mappings.push({
      id: crypto.randomUUID(),
      name: entry.name,
      localPath: selected,
      remotePath: entry.path,
      pathTemplate: inferPathTemplateFromLocal(selected),
      bindingStatus: "bound",
    });
    schedulePersistConfig();
    notify("positive", "已从远端文件创建映射");
  }

  function renameMapping(mapping: FileMapping) {
    const nextName = window.prompt("输入新的映射名称", mapping.name || mapping.remotePath)?.trim();
    if (!nextName || nextName === mapping.name) {
      return;
    }

    mapping.name = nextName;
    schedulePersistConfig();
    notify("positive", "映射名称已更新");
  }

  async function syncNow() {
    try {
      runtime.value = await invoke<RuntimeSnapshot>("sync_now");
      await Promise.all([loadLogs(), loadRemoteFiles(remotePath.value)]);
      notify("positive", "同步已执行");
    } catch (error) {
      notify("negative", `同步失败：${String(error)}`);
    }
  }

  const resourceActions = useSyncResourceActions({
    loadLogs,
    loadRemoteFiles,
    logs,
    remoteEntries,
    remotePath,
    runtime,
    validateLocalFilePath,
  });

  function selectTab(tab: TabKey, collapseDrawer = false) {
    currentTab.value = tab;
    if (collapseDrawer) {
      leftDrawerOpen.value = false;
    }
  }

  function toggleDrawer() {
    leftDrawerOpen.value = !leftDrawerOpen.value;
  }

  async function setupDragDrop() {
    unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "leave") {
        isMappingDropActive.value = false;
        return;
      }

      const acceptsDrop = currentTab.value === "settings";
      isMappingDropActive.value = acceptsDrop;

      if (payload.type === "drop") {
        if (acceptsDrop) {
          void addMappingsFromPaths(payload.paths);
        }
        isMappingDropActive.value = false;
      }
    });
  }

  function startAutoRefresh() {
    if (refreshTimer) {
      window.clearInterval(refreshTimer);
    }

    refreshTimer = window.setInterval(() => {
      void refreshRuntime();
    }, 4000);
  }

  async function bootstrap(isDesktopDrawerOpen: boolean) {
    applyTheme(window.localStorage.getItem("my-sync-theme") === "dark");
    leftDrawerOpen.value = isDesktopDrawerOpen;

    try {
      await Promise.all([loadAppState(), loadLogs()]);
      await loadRemoteFiles();
      await setupDragDrop();
      autoSaveEnabled = true;
    } catch (error) {
      notify("negative", `初始化失败：${String(error)}`);
    } finally {
      isBooting.value = false;
    }

    startAutoRefresh();
  }

  function teardown() {
    if (refreshTimer) {
      window.clearInterval(refreshTimer);
    }
    if (autoSaveTimer) {
      window.clearTimeout(autoSaveTimer);
    }
    if (refreshLogsTimer) {
      window.clearTimeout(refreshLogsTimer);
    }
    if (unlistenDragDrop) {
      unlistenDragDrop();
    }
  }

  return {
    canBrowseRemote,
    canSync,
    config,
    conflictItems,
    currentTab,
    fullRemotePath,
    handleConfigUpdated,
    isAutoSaving,
    isBooting,
    isDarkMode,
    isLoadingRemote,
    isMappingDropActive,
    leftDrawerOpen,
    logs,
    recentLogs,
    remoteEntries,
    remotePath,
    remotePathLabel,
    remoteRootPath,
    runtime,
    runtimeStatus,
    addMapping,
    addMappingsFromPaths,
    applyTheme,
    bootstrap,
    chooseFile,
    formatDateTime,
    loadRemoteFiles,
    mapRemoteEntryToLocal,
    renameMapping,
    clearBinding,
    removeMapping,
    selectTab,
    statusTone,
    syncNow,
    teardown,
    toggleDrawer,
    ...resourceActions,
  };
});
