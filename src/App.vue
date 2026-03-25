<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useQuasar } from "quasar";

type FileMapping = {
  id: string;
  name: string;
  localPath: string;
  remotePath: string;
};

type WebDavSettings = {
  baseUrl: string;
  username: string;
  password: string;
  remoteDir: string;
  clientId: string;
  syncIntervalSecs: number;
  autoSync: boolean;
};

type SyncSettings = {
  defaultConflictStrategy: "manual" | "local" | "remote";
  fsWatchEnabled: boolean;
  debounceDelaySecs: number;
  launchOnBoot: boolean;
};

type AppConfig = {
  webdav: WebDavSettings;
  sync: SyncSettings;
  mappings: FileMapping[];
};

type MappingRuntime = {
  mappingId: string;
  status: string;
  detail: string;
  lastSyncAt: string | null;
  lastSyncedHash: string | null;
  lastConflictPath: string | null;
};

type RuntimeSnapshot = {
  isSyncing: boolean;
  isReady: boolean;
  readinessDetail: string;
  conflictCount: number;
  errorCount: number;
  lastRunAt: string | null;
  lastSummary: string;
  mappings: MappingRuntime[];
};

type SyncLogEntry = {
  id: string;
  timestamp: string;
  level: string;
  action: string;
  summary: string;
  detail: string;
  httpStatus: number | null;
  mappingId: string | null;
  mappingName: string | null;
  localPath: string | null;
  remotePath: string | null;
  targetPath: string | null;
};

type RemoteBrowserEntry = {
  name: string;
  path: string;
  isDir: boolean;
  size: number | null;
  modifiedAt: string | null;
};

type AppSnapshot = {
  config: AppConfig;
  runtime: RuntimeSnapshot;
};

const $q = useQuasar();
const currentTab = ref("dashboard");
const leftDrawerOpen = ref(false);
const isAutoSaving = ref(false);
const isBooting = ref(true);
const isDarkMode = ref(false);
const logs = ref<SyncLogEntry[]>([]);
const remoteEntries = ref<RemoteBrowserEntry[]>([]);
const remotePath = ref("");
const isLoadingRemote = ref(false);
const isMappingDropActive = ref(false);
const lastSavedConfig = ref<AppConfig | null>(null);
let refreshTimer: number | undefined;
let unlistenDragDrop: (() => void) | undefined;
let autoSaveTimer: number | undefined;
let autoSaveEnabled = false;

const config = reactive<AppConfig>({
  webdav: {
    baseUrl: "",
    username: "",
    password: "",
    remoteDir: "my-sync",
    clientId: "",
    syncIntervalSecs: 30,
    autoSync: true,
  },
  sync: {
    defaultConflictStrategy: "manual",
    fsWatchEnabled: true,
    debounceDelaySecs: 15,
    launchOnBoot: false,
  },
  mappings: [],
});

const runtime = ref<RuntimeSnapshot>({
  isSyncing: false,
  isReady: false,
  readinessDetail: "尚未初始化",
  conflictCount: 0,
  errorCount: 0,
  lastRunAt: null,
  lastSummary: "尚未同步",
  mappings: [],
});

const runtimeById = computed(
  () => new Map(runtime.value.mappings.map((item) => [item.mappingId, item])),
);
const conflictItems = computed(() =>
  config.mappings.filter((item) => runtimeById.value.get(item.id)?.status === "conflict"),
);
const recentLogs = computed(() => logs.value.slice(0, 6));
const remoteRootPath = computed(() =>
  [config.webdav.remoteDir.trim(), config.webdav.clientId.trim()].filter(Boolean).join("/"),
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
    !!config.webdav.clientId.trim(),
);
const navItems = [
  { key: "dashboard", label: "首页", icon: "sym_r_dashboard" },
  { key: "files", label: "文件", icon: "sym_r_folder_managed" },
  { key: "logs", label: "日志", icon: "sym_r_receipt_long" },
  { key: "conflicts", label: "冲突", icon: "sym_r_merge_type" },
  { key: "settings", label: "设置", icon: "sym_r_settings" },
  { key: "about", label: "关于", icon: "sym_r_info" },
] as const;

function applyTheme(next: boolean) {
  isDarkMode.value = next;
  document.documentElement.classList.toggle("dark", next);
  window.localStorage.setItem("my-sync-theme", next ? "dark" : "light");
}

function fullRemotePath(path: string) {
  const relativePath = path.trim().replace(/^\/+/, "");
  if (!relativePath) {
    return remoteRootPath.value || "未生成";
  }
  return remoteRootPath.value ? `${remoteRootPath.value}/${relativePath}` : relativePath;
}

function formatDateTime(value: string | null) {
  if (!value) {
    return "尚未执行";
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(parsed);
}

function toggleDrawer() {
  leftDrawerOpen.value = !leftDrawerOpen.value;
}

function selectTab(tab: string) {
  currentTab.value = tab;
  if ($q.screen.lt.md) {
    leftDrawerOpen.value = false;
  }
}

function notify(type: "positive" | "negative" | "warning", message: string) {
  const iconByType = {
    positive: "sym_r_check_circle",
    negative: "sym_r_error",
    warning: "sym_r_warning",
  };
  $q.notify({
    type,
    message,
    icon: iconByType[type],
    position: "bottom-right",
    classes: "app-notify",
  });
}

function createMapping(): FileMapping {
  return {
    id: crypto.randomUUID(),
    name: "",
    localPath: "",
    remotePath: "",
  };
}

function createMappingFromPath(localPath: string): FileMapping {
  const normalized = localPath.replace(/\\/g, "/");
  const filename = normalized.split("/").pop() || "config";
  return {
    id: crypto.randomUUID(),
    name: filename,
    localPath,
    remotePath: inferRemotePathFromLocal(localPath),
  };
}

async function validateLocalFilePath(localPath: string) {
  await invoke("validate_local_file", { path: localPath });
}

function inferRemotePathFromLocal(localPath: string) {
  const normalized = localPath.replace(/\\/g, "/");
  const segments = normalized.split("/").filter(Boolean);
  if (segments.length >= 3) {
    const usersIndex = segments.findIndex((item) => item.toLowerCase() === "users");
    if (usersIndex >= 0 && segments.length > usersIndex + 2) {
      return segments.slice(usersIndex + 2).join("/");
    }
  }
  return segments.slice(-2).join("/") || segments[segments.length - 1] || "config";
}

function resetConfig(next: AppConfig) {
  config.webdav.baseUrl = next.webdav.baseUrl ?? "";
  config.webdav.username = next.webdav.username ?? "";
  config.webdav.password = next.webdav.password ?? "";
  config.webdav.remoteDir = next.webdav.remoteDir ?? "my-sync";
  config.webdav.clientId = next.webdav.clientId ?? "";
  config.webdav.syncIntervalSecs = next.webdav.syncIntervalSecs ?? 30;
  config.webdav.autoSync = next.webdav.autoSync ?? true;
  config.sync.defaultConflictStrategy = next.sync?.defaultConflictStrategy ?? "manual";
  config.sync.fsWatchEnabled = next.sync?.fsWatchEnabled ?? true;
  config.sync.debounceDelaySecs = next.sync?.debounceDelaySecs ?? 15;
  config.sync.launchOnBoot = next.sync?.launchOnBoot ?? false;
  config.mappings.splice(
    0,
    config.mappings.length,
    ...(next.mappings ?? []).map((item) => ({ ...item })),
  );
}

function cloneConfig(source: AppConfig): AppConfig {
  return {
    webdav: { ...source.webdav },
    sync: { ...source.sync },
    mappings: source.mappings.map((item) => ({ ...item })),
  };
}

function snapshotConfig(): AppConfig {
  return {
    webdav: {
      baseUrl: config.webdav.baseUrl.trim(),
      username: config.webdav.username.trim(),
      password: config.webdav.password,
      remoteDir: config.webdav.remoteDir.trim(),
      clientId: config.webdav.clientId.trim(),
      syncIntervalSecs: Number(config.webdav.syncIntervalSecs) || 30,
      autoSync: !!config.webdav.autoSync,
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
    })),
  };
}

function runtimeStatus(id: string): MappingRuntime {
  return (
    runtimeById.value.get(id) ?? {
      mappingId: id,
      status: "idle",
      detail: "尚未同步",
      lastSyncAt: null,
      lastSyncedHash: null,
      lastConflictPath: null,
    }
  );
}

function statusTone(status: string) {
  switch (status) {
    case "synced":
    case "resolved":
      return "positive";
    case "pushed":
      return "primary";
    case "pulled":
      return "neutral";
    case "conflict":
      return "warning";
    case "error":
      return "danger";
    default:
      return "neutral";
  }
}

async function loadAppState() {
  const snapshot = await invoke<AppSnapshot>("load_app_state");
  resetConfig(snapshot.config);
  lastSavedConfig.value = cloneConfig(snapshot.config);
  runtime.value = snapshot.runtime;
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
    notify("positive", `已添加 ${acceptedPaths.length} 个文件映射`);
  }

  if (rejectedMessages.length > 0) {
    notify("negative", `以下文件未添加：${rejectedMessages.join("；")}`);
  }
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
  runtime.value = await invoke<RuntimeSnapshot>("get_runtime_state");
}

function savedBaseConfig(): AppConfig {
  return lastSavedConfig.value ?? snapshotConfig();
}

async function persistConfig() {
  const nextConfig = snapshotConfig();
  const currentSaved = savedBaseConfig();
  if (JSON.stringify(nextConfig) === JSON.stringify(currentSaved)) {
    return;
  }

  isAutoSaving.value = true;
  try {
    runtime.value = await invoke<RuntimeSnapshot>("save_app_config", {
      config: nextConfig,
    });
    lastSavedConfig.value = cloneConfig(nextConfig);
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

async function syncNow() {
  try {
    runtime.value = await invoke<RuntimeSnapshot>("sync_now");
    await loadLogs();
    await loadRemoteFiles(remotePath.value);
    notify("positive", "同步已执行");
  } catch (error) {
    notify("negative", `同步失败：${String(error)}`);
  }
}

async function chooseFile(mapping: FileMapping) {
  const selected = await open({
    multiple: false,
    directory: false,
    title: "选择要同步的配置文件",
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

  mapping.localPath = selected;
  const normalized = selected.replace(/\\/g, "/");
  const filename = normalized.split("/").pop() || "config";
  if (!mapping.name.trim()) {
    mapping.name = filename;
  }
  if (!mapping.remotePath.trim()) {
    mapping.remotePath = inferRemotePathFromLocal(selected);
  }
}

function addMapping() {
  config.mappings.push(createMapping());
}

function removeMapping(id: string) {
  const index = config.mappings.findIndex((item) => item.id === id);
  if (index >= 0) {
    config.mappings.splice(index, 1);
  }
}

async function clearLogs() {
  try {
    await invoke("clear_sync_logs");
    logs.value = [];
    notify("positive", "日志已清空");
  } catch (error) {
    notify("negative", `清空日志失败：${String(error)}`);
  }
}

async function exportLogs() {
  try {
    const target = await save({
      title: "导出同步日志",
      defaultPath: "my-sync-logs.json",
    });
    if (!target) {
      return;
    }
    await invoke("export_sync_logs", { path: target });
    notify("positive", "日志已导出");
  } catch (error) {
    notify("negative", `导出日志失败：${String(error)}`);
  }
}

async function resolveConflict(mappingId: string, strategy: "local" | "remote") {
  try {
    runtime.value = await invoke<RuntimeSnapshot>("resolve_conflict", {
      mappingId,
      strategy,
    });
    await loadLogs();
    await loadRemoteFiles(remotePath.value);
    notify("positive", "冲突已处理");
  } catch (error) {
    notify("negative", `处理冲突失败：${String(error)}`);
  }
}

async function downloadRemoteFile(entry: RemoteBrowserEntry) {
  try {
    const target = await save({
      title: `下载 ${entry.name}`,
      defaultPath: entry.name,
    });
    if (!target) {
      return;
    }
    await invoke("download_remote_file", {
      remotePath: entry.path,
      savePath: target,
    });
    notify("positive", "远端文件已下载");
  } catch (error) {
    notify("negative", `下载远端文件失败：${String(error)}`);
  }
}

async function uploadLocalFile() {
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      title: "选择要上传到当前远端目录的文件",
    });
    if (typeof selected !== "string") {
      return;
    }
    const filename = selected.replace(/\\/g, "/").split("/").pop() || "file";
    const fullRemotePath = remotePath.value
      ? `${remotePath.value.replace(/\/$/, "")}/${filename}`
      : filename;
    let overwrite = false;
    if (remoteEntries.value.some((item) => item.path === fullRemotePath)) {
      overwrite = window.confirm(`远端已存在同名文件“${filename}”，是否覆盖？`);
      if (!overwrite) {
        return;
      }
    }
    await invoke("upload_local_file", {
      localPath: selected,
      remoteDirPath: remotePath.value || null,
      overwrite,
    });
    await loadRemoteFiles(remotePath.value);
    notify("positive", "文件已上传到远端");
  } catch (error) {
    notify("negative", `上传文件失败：${String(error)}`);
  }
}

async function deleteRemoteEntry(entry: RemoteBrowserEntry) {
  try {
    const ok = window.confirm(
      `确认删除远端${entry.isDir ? "目录" : "文件"}“${entry.name}”？`,
    );
    if (!ok) {
      return;
    }
    await invoke("delete_remote_file", {
      remotePath: entry.path,
    });
    await loadRemoteFiles(remotePath.value);
    notify("positive", "远端项目已删除");
  } catch (error) {
    notify("negative", `删除远端项目失败：${String(error)}`);
  }
}

async function renameRemoteEntry(entry: RemoteBrowserEntry) {
  try {
    const newName = window.prompt("输入新的名称", entry.name)?.trim();
    if (!newName || newName === entry.name) {
      return;
    }
    const parentPrefix = entry.path.includes("/")
      ? entry.path.split("/").slice(0, -1).join("/")
      : "";
    const targetPath = parentPrefix ? `${parentPrefix}/${newName}` : newName;
    let overwrite = false;
    if (remoteEntries.value.some((item) => item.path === targetPath && item.path !== entry.path)) {
      overwrite = window.confirm(`远端已存在同名项目“${newName}”，是否覆盖？`);
      if (!overwrite) {
        return;
      }
    }
    await invoke("rename_remote_file", {
      remotePath: entry.path,
      newName,
      overwrite,
    });
    await loadRemoteFiles(remotePath.value);
    notify("positive", "远端项目已重命名");
  } catch (error) {
    notify("negative", `重命名远端项目失败：${String(error)}`);
  }
}

onMounted(async () => {
  applyTheme(window.localStorage.getItem("my-sync-theme") === "dark");
  leftDrawerOpen.value = $q.screen.gt.sm;
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
  try {
    await Promise.all([loadAppState(), loadLogs()]);
    await loadRemoteFiles();
    autoSaveEnabled = true;
  } catch (error) {
    notify("negative", `初始化失败：${String(error)}`);
  } finally {
    isBooting.value = false;
  }

  refreshTimer = window.setInterval(() => {
    void refreshRuntime();
  }, 4000);
});

onBeforeUnmount(() => {
  if (refreshTimer) {
    window.clearInterval(refreshTimer);
  }
  if (autoSaveTimer) {
    window.clearTimeout(autoSaveTimer);
  }
  if (unlistenDragDrop) {
    unlistenDragDrop();
  }
});

watch(
  snapshotConfig,
  () => {
    schedulePersistConfig();
  },
  { deep: true },
);
</script>

<template>
  <q-layout view="lHh Lpr lFf">
    <q-header v-if="$q.screen.lt.md" class="mobile-header">
      <q-toolbar class="mobile-toolbar">
        <q-btn flat round color="primary" icon="sym_r_menu" @click="toggleDrawer" />
        <q-toolbar-title class="mobile-title">
          <div class="brand-title">My Sync</div>
          <div class="brand-subtitle">Sparse Config Sync</div>
        </q-toolbar-title>
        <q-btn
          flat
          round
          color="primary"
          :icon="isDarkMode ? 'sym_r_light_mode' : 'sym_r_dark_mode'"
          @click="applyTheme(!isDarkMode)"
        />
      </q-toolbar>
    </q-header>

    <q-drawer
      v-model="leftDrawerOpen"
      show-if-above
      :breakpoint="820"
      :width="188"
      class="sidebar"
    >
      <div class="sidebar-brand">
        <q-icon name="sym_r_sync" size="30px" color="primary" />
        <div>
          <div class="brand-title">My Sync</div>
          <div class="brand-subtitle">Sparse Config Sync</div>
        </div>
        <q-space />
        <q-btn
          flat
          round
          color="primary"
          :icon="isDarkMode ? 'sym_r_light_mode' : 'sym_r_dark_mode'"
          @click="applyTheme(!isDarkMode)"
        />
      </div>

      <q-list padding>
        <q-item
          v-for="item in navItems"
          :key="item.key"
          clickable
          v-ripple
          :active="currentTab === item.key"
          class="nav-item"
          @click="selectTab(item.key)"
        >
          <q-item-section avatar>
            <q-icon :name="item.icon" />
          </q-item-section>
          <q-item-section>{{ item.label }}</q-item-section>
        </q-item>
      </q-list>

      <div class="sidebar-footer">
        <div class="sync-status" :class="runtime.isReady ? 'ready' : 'pending'">
          <div class="status-dot"></div>
          <div>
            <strong>{{ runtime.isSyncing ? "同步中" : runtime.isReady ? "系统就绪" : "待处理" }}</strong>
            <div>{{ runtime.readinessDetail }}</div>
          </div>
        </div>
      </div>
    </q-drawer>

    <q-page-container>
      <q-page class="page-shell">
        <div v-if="currentTab === 'dashboard'" class="view-shell">
          <div class="view-header dashboard-header">
            <div class="dashboard-title-block">
              <div class="eyebrow">Dashboard</div>
              <h1>仪表盘</h1>
            </div>
            <q-btn class="dashboard-sync-btn" color="primary" unelevated icon="sym_r_sync" label="立即同步" :disable="!canSync || runtime.isSyncing" :loading="runtime.isSyncing" @click="syncNow" />
          </div>

          <div class="stats-grid">
            <q-card flat class="panel-card stat-card theme-surface">
              <span class="stat-label">系统状态</span>
              <strong>{{ runtime.isReady ? "Ready" : "Blocked" }}</strong>
              <span>{{ runtime.readinessDetail }}</span>
            </q-card>
            <q-card flat class="panel-card stat-card theme-surface">
              <span class="stat-label">最近同步</span>
              <strong>{{ formatDateTime(runtime.lastRunAt) }}</strong>
              <span>{{ runtime.lastSummary }}</span>
            </q-card>
            <q-card flat class="panel-card stat-card theme-surface">
              <span class="stat-label">风险项</span>
              <strong>{{ runtime.conflictCount + runtime.errorCount }}</strong>
              <span>冲突 {{ runtime.conflictCount }}，错误 {{ runtime.errorCount }}</span>
            </q-card>
          </div>

          <div class="content-grid dashboard-content-grid">
            <q-card flat class="panel-card theme-surface dashboard-panel">
              <div class="panel-head">
                <div class="panel-title">最近日志</div>
                <q-btn flat class="subtle-action" label="查看全部" @click="currentTab = 'logs'" />
              </div>
              <div class="dashboard-panel-body">
                <div v-if="recentLogs.length === 0" class="empty-block">还没有同步日志</div>
                <q-list v-else separator class="dashboard-list">
                  <q-item v-for="entry in recentLogs" :key="entry.id" class="dashboard-log-item">
                    <q-item-section avatar>
                      <q-icon :name="entry.level === 'error' ? 'sym_r_error' : entry.level === 'warning' ? 'sym_r_warning' : 'sym_r_check_circle'" :color="entry.level === 'error' ? 'negative' : entry.level === 'warning' ? 'warning' : 'positive'" />
                    </q-item-section>
                    <q-item-section class="dashboard-log-main">
                      <q-item-label class="dashboard-log-summary">{{ entry.summary }}</q-item-label>
                      <q-item-label caption class="dashboard-log-detail">{{ entry.detail }}</q-item-label>
                      <q-item-label caption class="dashboard-log-time">
                        {{ formatDateTime(entry.timestamp) }}
                      </q-item-label>
                    </q-item-section>
                  </q-item>
                </q-list>
              </div>
            </q-card>

            <q-card flat class="panel-card theme-surface dashboard-panel">
              <div class="panel-head">
                <div class="panel-title">同步项状态</div>
                <q-btn flat class="subtle-action" label="前往设置" @click="currentTab = 'settings'" />
              </div>
              <div class="dashboard-panel-body">
                <q-list v-if="config.mappings.length > 0" separator class="dashboard-list">
                  <q-item v-for="item in config.mappings" :key="item.id">
                    <q-item-section>
                      <q-item-label>{{ item.name || item.remotePath || item.localPath }}</q-item-label>
                      <q-item-label caption>{{ runtimeStatus(item.id).detail }}</q-item-label>
                    </q-item-section>
                    <q-item-section side>
                      <q-chip dense class="status-pill" :class="`status-pill--${statusTone(runtimeStatus(item.id).status)}`">
                        {{ runtimeStatus(item.id).status }}
                      </q-chip>
                    </q-item-section>
                  </q-item>
                </q-list>
                <div v-else class="empty-block">还没有文件映射</div>
              </div>
            </q-card>
          </div>
        </div>

        <div v-else-if="currentTab === 'logs'" class="view-shell">
          <div class="view-header">
            <div>
              <div class="eyebrow">Logs</div>
              <h1>同步日志</h1>
            </div>
            <div class="row q-gutter-sm">
              <q-btn flat class="subtle-action" label="导出" @click="exportLogs" />
              <q-btn flat class="danger-action" label="清空" @click="clearLogs" />
            </div>
          </div>

          <q-card flat class="panel-card theme-surface">
            <div v-if="logs.length === 0" class="empty-block">还没有日志记录</div>
            <q-list v-else separator>
              <q-item v-for="entry in logs" :key="entry.id" class="log-item">
                <q-item-section avatar>
                  <q-icon :name="entry.level === 'error' ? 'sym_r_error' : entry.level === 'warning' ? 'sym_r_warning' : 'sym_r_check_circle'" :color="entry.level === 'error' ? 'negative' : entry.level === 'warning' ? 'warning' : 'positive'" />
                </q-item-section>
                <q-item-section>
                  <q-item-label>{{ entry.summary }}</q-item-label>
                  <q-item-label caption>{{ entry.detail }}</q-item-label>
                  <q-item-label caption v-if="entry.mappingName || entry.remotePath">
                    {{ entry.mappingName || '未命名映射' }} · {{ entry.remotePath || '无远端路径' }}
                  </q-item-label>
                  <q-item-label caption v-if="entry.httpStatus || entry.targetPath">
                    <span v-if="entry.httpStatus">HTTP {{ entry.httpStatus }}</span>
                    <span v-if="entry.httpStatus && entry.targetPath"> · </span>
                    <span v-if="entry.targetPath">目标 {{ entry.targetPath }}</span>
                  </q-item-label>
                </q-item-section>
                <q-item-section side top class="log-time-side">{{ formatDateTime(entry.timestamp) }}</q-item-section>
              </q-item>
            </q-list>
          </q-card>
        </div>

        <div v-else-if="currentTab === 'files'" class="view-shell">
          <div class="view-header">
            <div>
              <div class="eyebrow">Files</div>
              <h1>远端文件</h1>
            </div>
            <div class="row q-gutter-sm page-actions">
              <q-btn unelevated color="primary" icon="sym_r_upload_file" label="上传文件" @click="uploadLocalFile" />
              <q-btn
                flat
                class="subtle-action"
                icon="sym_r_arrow_upward"
                label="返回上级"
                :disable="!remotePath"
                @click="loadRemoteFiles(remotePath.split('/').slice(0, -1).join('/'))"
              />
              <q-btn flat class="subtle-action" icon="sym_r_refresh" label="刷新" @click="loadRemoteFiles(remotePath)" />
            </div>
          </div>

          <div class="content-grid">
            <q-card flat class="panel-card theme-surface">
              <div class="panel-title">远端根路径</div>
              <div class="q-mt-md remote-root-box">
                <div><strong>根目录</strong> {{ config.webdav.remoteDir }}</div>
                <div><strong>客户端 ID</strong> {{ config.webdav.clientId }}</div>
                <div><strong>当前路径</strong> {{ remotePathLabel || "未配置" }}</div>
              </div>
            </q-card>

            <q-card flat class="panel-card theme-surface">
              <div class="panel-title">映射提示</div>
              <div class="q-mt-md remote-root-box">
                <div>当前文件菜单是 WebDAV 远端浏览视图。</div>
                <div>文件映射编辑入口已保留在“设置”页。</div>
                <div>后续可继续补远端下载、删除、重命名等操作。</div>
              </div>
            </q-card>
          </div>

          <q-card flat class="panel-card theme-surface">
            <div v-if="isLoadingRemote" class="empty-block">正在读取远端目录...</div>
            <div v-else-if="remoteEntries.length === 0" class="empty-block">当前目录为空</div>
            <q-list v-else separator>
              <q-item
                v-for="entry in remoteEntries"
                :key="entry.path"
                clickable
                @click="entry.isDir ? loadRemoteFiles(entry.path) : undefined"
              >
                <q-item-section avatar>
                    <q-icon
                      :name="entry.isDir ? 'sym_r_folder' : 'sym_r_description'"
                      :color="entry.isDir ? 'primary' : 'secondary'"
                    />
                </q-item-section>
                <q-item-section>
                  <q-item-label>{{ entry.name }}</q-item-label>
                  <q-item-label caption>{{ entry.path }}</q-item-label>
                </q-item-section>
                <q-item-section side top>
                  <div class="remote-entry-meta">
                    <span>{{ entry.isDir ? "目录" : `${entry.size ?? 0} B` }}</span>
                    <span v-if="entry.modifiedAt">{{ entry.modifiedAt }}</span>
                    <q-btn
                      v-if="!entry.isDir"
                      flat
                      dense
                      size="sm"
                      class="subtle-action compact-action"
                      icon="sym_r_download"
                      label="下载"
                      @click.stop="downloadRemoteFile(entry)"
                    />
                    <q-btn
                      flat
                      dense
                      size="sm"
                      class="subtle-action compact-action"
                      icon="sym_r_drive_file_rename_outline"
                      label="重命名"
                      @click.stop="renameRemoteEntry(entry)"
                    />
                    <q-btn
                      flat
                      dense
                      size="sm"
                      class="danger-action compact-action"
                      icon="sym_r_delete"
                      label="删除"
                      @click.stop="deleteRemoteEntry(entry)"
                    />
                  </div>
                </q-item-section>
              </q-item>
            </q-list>
          </q-card>
        </div>

        <div v-else-if="currentTab === 'conflicts'" class="view-shell">
          <div class="view-header">
            <div>
              <div class="eyebrow">Conflicts</div>
              <h1>冲突处理</h1>
            </div>
          </div>

          <div v-if="conflictItems.length === 0" class="empty-card panel-card theme-surface">
            当前没有待处理冲突
          </div>
          <div v-else class="conflict-list">
            <q-card v-for="item in conflictItems" :key="item.id" flat class="panel-card conflict-card theme-surface">
              <div class="panel-head">
                <div>
                  <div class="panel-title">{{ item.name || item.remotePath }}</div>
                  <div class="panel-subtitle">{{ runtimeStatus(item.id).detail }}</div>
                </div>
                <q-chip class="status-pill status-pill--warning">conflict</q-chip>
              </div>
              <div class="conflict-meta">
                <div>本地：{{ item.localPath }}</div>
                <div>远端：{{ item.remotePath }}</div>
                <div v-if="runtimeStatus(item.id).lastConflictPath">冲突副本：{{ runtimeStatus(item.id).lastConflictPath }}</div>
              </div>
              <div class="row q-gutter-sm q-mt-md">
                <q-btn color="primary" unelevated label="以本地为准" @click="resolveConflict(item.id, 'local')" />
                <q-btn outline color="primary" label="以远端为准" @click="resolveConflict(item.id, 'remote')" />
              </div>
            </q-card>
          </div>
        </div>

        <div v-else-if="currentTab === 'settings'" class="view-shell">
          <div class="view-header">
            <div>
              <div class="eyebrow">Settings</div>
              <h1>设置</h1>
            </div>
            <q-chip dense class="status-pill" :class="`status-pill--${isAutoSaving ? 'primary' : 'neutral'}`">
              {{ isAutoSaving ? "自动保存中" : "已自动保存" }}
            </q-chip>
          </div>

          <div class="settings-grid">
            <q-card flat class="panel-card theme-surface">
              <div class="panel-title">WebDAV 设置</div>
              <div class="form-grid q-mt-md">
                <q-input v-model="config.webdav.baseUrl" outlined label="WebDAV 地址" placeholder="https://..." />
                <q-input v-model="config.webdav.remoteDir" outlined label="远端同步根目录" readonly />
                <q-input v-model="config.webdav.clientId" outlined label="客户端 ID" readonly />
                <q-input v-model="config.webdav.username" outlined label="用户名" />
                <q-input v-model="config.webdav.password" outlined type="password" label="密码 / Token" />
                <q-input v-model.number="config.webdav.syncIntervalSecs" outlined type="number" min="10" label="轮询间隔（秒）" />
                <div class="toggle-box">
                  <q-toggle v-model="config.webdav.autoSync" label="启用后台自动同步" color="primary" />
                </div>
                <q-banner rounded class="info-banner full-span">
                  远端实际同步根路径为：{{ remoteRootPath || "未生成" }}
                </q-banner>
              </div>
            </q-card>

            <q-card flat class="panel-card theme-surface">
              <div class="panel-title">同步策略</div>
              <div class="q-mt-md q-gutter-y-md">
                <q-select
                  v-model="config.sync.defaultConflictStrategy"
                  outlined
                  emit-value
                  map-options
                  label="默认冲突解决方案"
                  :options="[
                    { label: '手动处理', value: 'manual' },
                    { label: '以本地为准', value: 'local' },
                    { label: '以远端为准', value: 'remote' },
                  ]"
                />
                <div class="row items-center justify-between q-gutter-md">
                  <div class="col">
                    <div class="text-weight-medium">开机自启动</div>
                    <div class="text-caption caption-soft">
                      登录系统后自动启动应用，适合需要长期驻留托盘的场景
                    </div>
                  </div>
                  <q-toggle v-model="config.sync.launchOnBoot" color="primary" />
                </div>
                <div class="row items-center justify-between q-gutter-md">
                  <div class="col">
                    <div class="text-weight-medium">启用文件系统监听</div>
                    <div class="text-caption caption-soft">
                      开启后优先使用本地文件变更事件，替代纯轮询触发同步
                    </div>
                  </div>
                  <q-toggle v-model="config.sync.fsWatchEnabled" color="primary" />
                </div>
                <q-input
                  v-model.number="config.sync.debounceDelaySecs"
                  outlined
                  type="number"
                  min="3"
                  max="60"
                  label="监听防抖延时（秒）"
                />
                <q-banner rounded class="info-banner">
                  单文件大小限制为 1MB。当前“系统就绪”会检查 WebDAV 基本配置、本地路径是否存在，以及是否有待处理冲突。开启文件监听后，会在检测到变更并静默 {{ config.sync.debounceDelaySecs || 15 }} 秒后批量触发同步。
                </q-banner>
              </div>
            </q-card>
          </div>

          <q-card
            flat
            class="panel-card q-mt-lg theme-surface mapping-drop-zone"
            :class="{ 'mapping-drop-zone--active': isMappingDropActive }"
          >
            <div class="panel-head">
              <div>
                <div class="panel-title">文件映射</div>
                <div class="panel-subtitle">支持拖拽本地文件到此区域快速添加映射</div>
              </div>
              <div class="row q-gutter-sm mapping-toolbar">
                <q-btn color="primary" unelevated icon="sym_r_add" label="新增映射" @click="addMapping" />
              </div>
            </div>
            <div v-if="config.mappings.length === 0" class="empty-block">拖拽文件到这里，或点击“新增映射”</div>
            <div v-else class="mapping-list">
              <q-card v-for="item in config.mappings" :key="item.id" flat class="mapping-card theme-surface">
                <div class="mapping-head">
                  <q-input v-model="item.name" dense borderless placeholder="映射名称" class="mapping-title" />
                  <div class="row items-center q-gutter-sm">
                    <q-chip dense class="status-pill" :class="`status-pill--${statusTone(runtimeStatus(item.id).status)}`">
                      {{ runtimeStatus(item.id).status }}
                    </q-chip>
                    <q-btn round flat dense class="danger-action" icon="sym_r_delete" @click="removeMapping(item.id)" />
                  </div>
                </div>
                <div class="mapping-grid">
                  <q-input v-model="item.localPath" dense outlined label="本地文件" class="compact-field">
                    <template #append>
                      <q-btn flat dense icon="sym_r_folder_open" @click="chooseFile(item)" />
                    </template>
                  </q-input>
                  <q-input
                    :model-value="fullRemotePath(item.remotePath)"
                    dense
                    outlined
                    readonly
                    label="远端完整路径"
                    class="compact-field"
                  />
                </div>
              </q-card>
            </div>
          </q-card>
        </div>

        <div v-else class="view-shell">
          <div class="view-header">
            <div>
              <div class="eyebrow">About</div>
              <h1>关于</h1>
            </div>
          </div>
          <q-card flat class="panel-card about-card theme-surface">
            <q-icon name="sym_r_sync" size="72px" color="primary" />
            <h2>My Sync</h2>
            <p>用于将零散配置文件映射到 WebDAV 的桌面同步工具。</p>
            <p>当前已支持日志查看、冲突处理、后台轮询同步和便携版构建。</p>
          </q-card>
        </div>

        <q-inner-loading :showing="isBooting">
          <q-spinner-dots size="50px" color="primary" />
        </q-inner-loading>
      </q-page>
    </q-page-container>
  </q-layout>
</template>

<style lang="scss">
:root {
  --app-text: #17201f;
  --app-text-muted: rgba(23, 32, 31, 0.66);
  --app-text-soft: rgba(23, 32, 31, 0.58);
  --app-text-faint: rgba(23, 32, 31, 0.52);
  --app-primary-strong: #2f7d6b;
  --app-primary-soft: rgba(47, 125, 107, 0.12);
  --app-primary-soft-strong: rgba(47, 125, 107, 0.18);
  --app-danger-soft: rgba(201, 79, 79, 0.12);
  --app-warning-soft: rgba(211, 156, 52, 0.14);
  --app-border: rgba(23, 32, 31, 0.08);
  --app-panel-bg: rgba(250, 251, 249, 0.86);
  --app-panel-bg-strong: rgba(243, 247, 244, 0.92);
  --app-panel-bg-soft: rgba(239, 244, 241, 0.84);
  --app-shadow: 0 18px 48px rgba(31, 45, 61, 0.08);
  --app-header-bg: rgba(229, 238, 233, 0.9);
  --app-sidebar-bg: rgba(243, 247, 244, 0.9);
  --app-control-bg: rgba(255, 255, 255, 0.56);
  --app-control-bg-strong: rgba(255, 255, 255, 0.72);
  --app-control-border: rgba(23, 32, 31, 0.08);
  --app-surface-bg:
    radial-gradient(circle at top left, rgba(244, 199, 133, 0.35), transparent 30%),
    radial-gradient(circle at 90% 10%, rgba(92, 143, 255, 0.12), transparent 20%),
    linear-gradient(160deg, #f6f1e8 0%, #eef3f7 52%, #dce5ea 100%);
}

:root.dark {
  --app-text: #edf3f1;
  --app-text-muted: rgba(237, 243, 241, 0.78);
  --app-text-soft: rgba(237, 243, 241, 0.64);
  --app-text-faint: rgba(237, 243, 241, 0.52);
  --app-primary-strong: #67b39f;
  --app-primary-soft: rgba(103, 179, 159, 0.14);
  --app-primary-soft-strong: rgba(103, 179, 159, 0.22);
  --app-danger-soft: rgba(201, 79, 79, 0.18);
  --app-warning-soft: rgba(211, 156, 52, 0.18);
  --app-border: rgba(176, 223, 209, 0.14);
  --app-panel-bg: rgba(17, 26, 34, 0.84);
  --app-panel-bg-strong: rgba(20, 31, 41, 0.9);
  --app-panel-bg-soft: rgba(24, 37, 48, 0.82);
  --app-shadow: 0 24px 56px rgba(0, 0, 0, 0.34);
  --app-header-bg: rgba(32, 63, 72, 0.86);
  --app-sidebar-bg: rgba(28, 39, 47, 0.92);
  --app-control-bg: rgba(11, 20, 31, 0.88);
  --app-control-bg-strong: rgba(18, 29, 41, 0.94);
  --app-control-border: rgba(176, 223, 209, 0.12);
  --app-surface-bg:
    radial-gradient(circle at top left, rgba(43, 98, 87, 0.22), transparent 28%),
    radial-gradient(circle at 85% 12%, rgba(59, 130, 246, 0.14), transparent 20%),
    linear-gradient(180deg, #07101f 0%, #091427 48%, #040814 100%);
}

body {
  margin: 0;
  font-family: "Segoe UI", "PingFang SC", sans-serif;
  color: var(--app-text);
  background: var(--app-surface-bg);
}

#app {
  min-height: 100vh;
  color: var(--app-text);
  background: var(--app-surface-bg);
}

.q-layout,
.q-page-container,
.q-page {
  color: var(--app-text);
  background: transparent;
}

.q-notification__icon,
.app-notify .q-icon {
  font-family: "Material Symbols Rounded" !important;
  font-variation-settings: "FILL" 1, "wght" 400, "GRAD" 0, "opsz" 24;
}

.sidebar {
  background: var(--app-sidebar-bg);
  backdrop-filter: blur(18px);
  border-right: 1px solid var(--app-border);
}

.mobile-header {
  background: var(--app-header-bg);
  backdrop-filter: blur(18px);
  border-bottom: 1px solid var(--app-border);
}

.q-header {
  background: var(--app-header-bg);
  color: var(--app-text);
}

.mobile-toolbar {
  min-height: 72px;
  padding: 10px 14px;
}

.mobile-title {
  padding-left: 8px;
}

.sidebar-brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 20px 14px 14px;
}

.brand-title {
  font-size: 18px;
  font-weight: 800;
}

.brand-subtitle {
  font-size: 11px;
  color: var(--app-text-soft);
}

.nav-item {
  margin: 2px 8px;
  border-radius: 10px;
  min-height: 42px;
}

.nav-item.q-item--active {
  background: var(--app-primary-soft-strong);
  color: var(--q-primary);
}

.sidebar-footer {
  padding: 12px;
}

.sync-status {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 12px;
  border-radius: 14px;
  background: var(--app-panel-bg-soft);
  font-size: 11px;
}

.sync-status strong {
  display: block;
  font-size: 12px;
}

.status-dot {
  width: 10px;
  height: 10px;
  margin-top: 4px;
  border-radius: 999px;
  background: var(--app-text-faint);
}

.sync-status.ready .status-dot {
  background: var(--app-primary-strong);
}

.sync-status.pending .status-dot {
  background: var(--q-warning);
}

.page-shell {
  max-width: 1120px;
  margin: 0 auto;
  padding: 16px 14px 18px;
}

.view-shell {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.view-header {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-end;
}

.dashboard-header {
  align-items: center;
  margin-bottom: 2px;
}

.dashboard-title-block {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.dashboard-sync-btn {
  min-width: 156px;
}

.eyebrow {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--q-primary);
}

.view-header h1,
.about-card h2 {
  margin: 4px 0 0;
  font-size: 24px;
}

.stats-grid,
.content-grid,
.settings-grid,
.mapping-grid {
  display: grid;
  gap: 12px;
}

.stats-grid {
  grid-template-columns: repeat(3, minmax(140px, 1fr));
}

.content-grid,
.settings-grid {
  grid-template-columns: repeat(2, minmax(260px, 1fr));
}

.mapping-grid {
  grid-template-columns: minmax(220px, 1.2fr) minmax(180px, 1fr);
}

.panel-card,
.mapping-card,
.empty-card {
  background: linear-gradient(180deg, var(--app-panel-bg-strong) 0%, var(--app-panel-bg) 100%);
  backdrop-filter: blur(18px);
  border: 1px solid var(--app-border);
  box-shadow: var(--app-shadow);
  border-radius: 24px;
}

.panel-card {
  padding: 14px;
}

.stat-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-height: 136px;
  justify-content: space-between;
}

.stat-label {
  font-size: 12px;
  color: var(--app-text-soft);
}

.stat-card strong {
  font-size: 20px;
}

.panel-head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
}

.panel-subtitle {
  margin-top: 4px;
  color: var(--app-text-soft);
  font-size: 12px;
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.full-span {
  grid-column: 1 / -1;
}

.toggle-box {
  display: flex;
  align-items: center;
}

.empty-block,
.empty-card {
  padding: 18px;
  text-align: center;
  color: var(--app-text-soft);
}

.dashboard-content-grid {
  align-items: stretch;
}

.dashboard-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-height: 268px;
}

.dashboard-panel-body {
  min-height: 0;
  flex: 1;
}

.dashboard-list {
  max-height: 190px;
  overflow: auto;
}

.dashboard-log-item {
  align-items: flex-start;
  padding-top: 10px;
  padding-bottom: 10px;
}

.dashboard-log-main {
  min-width: 0;
}

.dashboard-log-summary,
.dashboard-log-detail,
.dashboard-log-time {
  white-space: normal;
  word-break: break-word;
}

.dashboard-log-summary {
  font-weight: 600;
}

.dashboard-log-detail {
  margin-top: 4px;
}

.dashboard-log-time {
  margin-top: 6px;
  color: var(--app-text-faint);
}

.mapping-list,
.conflict-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.mapping-card {
  padding: 10px 12px;
}

.mapping-head {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  align-items: center;
  margin-bottom: 8px;
}

.mapping-title {
  min-width: 120px;
  font-size: 14px;
  font-weight: 700;
}

.mapping-toolbar {
  align-items: center;
}

.mapping-drop-zone {
  position: relative;
  transition:
    border-color 0.18s ease,
    box-shadow 0.18s ease,
    transform 0.18s ease;
}

.mapping-drop-zone--active {
  border-color: var(--app-primary-strong);
  box-shadow:
    0 0 0 1px var(--app-primary-strong),
    var(--app-shadow);
}

.mapping-drop-zone--active::after {
  content: "松开鼠标以添加文件映射";
  position: absolute;
  top: 14px;
  right: 14px;
  padding: 6px 10px;
  border-radius: 999px;
  background: var(--app-primary-soft-strong);
  color: var(--app-primary-strong);
  font-size: 12px;
  font-weight: 700;
  pointer-events: none;
}

.compact-field .q-field__label {
  font-size: 12px;
}

.compact-field .q-field__native,
.compact-field .q-field__input {
  font-size: 13px;
}

.compact-field.q-field--dense .q-field__control,
.compact-field .q-field__control {
  min-height: 48px;
}

.mapping-meta,
.conflict-meta,
.remote-root-box {
  margin-top: 12px;
  color: var(--app-text-muted);
  word-break: break-all;
}

.remote-entry-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-items: flex-end;
  color: var(--app-text-soft);
  font-size: 12px;
}

.about-card {
  text-align: center;
}

.log-item {
  align-items: flex-start;
}

.page-actions {
  align-items: center;
}

.subtle-action {
  color: var(--app-primary-strong) !important;
}

.subtle-action.q-btn--flat::before,
.subtle-action.q-btn--outline::before {
  background: var(--app-primary-soft);
}

.compact-action {
  min-height: 28px;
  padding-inline: 6px;
}

.danger-action {
  color: var(--q-negative) !important;
}

.danger-action.q-btn--flat::before,
.danger-action.q-btn--outline::before {
  background: var(--app-danger-soft);
}

.info-banner {
  background: linear-gradient(180deg, var(--app-panel-bg-soft) 0%, var(--app-panel-bg) 100%) !important;
  color: var(--app-text-muted);
  border: 1px solid var(--app-border);
}

.caption-soft {
  color: var(--app-text-soft);
}

.log-time-side {
  color: var(--app-text-faint);
  white-space: nowrap;
}

.status-pill {
  border: 1px solid var(--app-border);
  background: var(--app-panel-bg-soft);
  color: var(--app-text-muted);
  font-weight: 700;
  text-transform: lowercase;
}

.status-pill--primary,
.status-pill--positive {
  border-color: transparent;
  background: var(--app-primary-soft-strong);
  color: var(--app-primary-strong);
}

.status-pill--warning {
  border-color: transparent;
  background: var(--app-warning-soft);
  color: var(--q-warning);
}

.status-pill--danger {
  border-color: transparent;
  background: var(--app-danger-soft);
  color: var(--q-negative);
}

.status-pill--neutral {
  background: var(--app-panel-bg-soft);
}

:root.dark .q-field__native,
:root.dark .q-field__input,
:root.dark .q-field__marginal,
:root.dark .q-field__label,
:root.dark .q-item__label,
:root.dark .q-banner,
:root.dark .q-card,
:root.dark .q-chip,
:root.dark .q-item,
:root.dark .q-toolbar,
:root.dark .q-btn {
  color: var(--app-text);
}

:root.dark .q-field--outlined .q-field__control,
:root.dark .q-field--outlined .q-field__control::before,
:root.dark .q-field--outlined .q-field__control::after,
:root.dark .q-select__dropdown-icon,
:root.dark .q-banner,
:root.dark .q-list,
:root.dark .q-separator {
  border-color: var(--app-border);
}

:root.dark .q-field--outlined .q-field__control,
:root.dark .q-field--filled .q-field__control,
:root.dark .q-field__control {
  background: var(--app-control-bg);
  color: var(--app-text);
}

:root.dark .q-field--readonly .q-field__control,
:root.dark .q-field--disabled .q-field__control {
  background: var(--app-control-bg-strong);
}

:root.dark .q-field__native::placeholder,
:root.dark .q-field__input::placeholder {
  color: var(--app-text-faint);
}

:root.dark .q-field__bottom,
:root.dark .q-field__messages,
:root.dark .q-field__counter {
  color: var(--app-text-faint);
}

:root.dark .q-toggle__track {
  opacity: 1;
  background: rgba(237, 243, 241, 0.18);
}

:root.dark .q-toggle__thumb {
  color: var(--app-primary-strong);
}

:root.dark .q-toggle[aria-checked="true"] .q-toggle__track,
:root.dark .q-toggle.q-toggle--truthy .q-toggle__track {
  background: rgba(103, 179, 159, 0.36);
}

:root.dark .info-banner,
:root.dark .q-banner.info-banner {
  background: linear-gradient(180deg, rgba(22, 33, 45, 0.94) 0%, rgba(15, 24, 35, 0.92) 100%) !important;
  color: var(--app-text-soft) !important;
  border: 1px solid var(--app-border) !important;
}

:root.dark .page-actions .q-btn--unelevated,
:root.dark .dashboard-sync-btn.q-btn--unelevated,
:root.dark .q-btn.bg-primary {
  box-shadow: none;
}

:root.dark .subtle-action,
:root.dark .status-pill--primary,
:root.dark .status-pill--positive {
  color: var(--app-primary-strong) !important;
}

:root.dark .sidebar,
:root.dark .mobile-header,
:root.dark .q-header {
  box-shadow: inset 0 -1px 0 rgba(176, 223, 209, 0.08);
}

:root.dark .panel-card,
:root.dark .mapping-card,
:root.dark .empty-card {
  background: linear-gradient(180deg, rgba(20, 31, 41, 0.92) 0%, rgba(15, 24, 35, 0.9) 100%);
}

:root.dark .sync-status,
:root.dark .status-pill--neutral {
  background: rgba(22, 33, 45, 0.9);
}

:root.dark .nav-item.q-item--active {
  background: rgba(103, 179, 159, 0.16);
}

:root.dark .q-item__label--caption,
:root.dark .caption-soft,
:root.dark .remote-entry-meta,
:root.dark .remote-root-box,
:root.dark .mapping-meta,
:root.dark .conflict-meta,
:root.dark .log-time-side,
:root.dark .dashboard-log-time {
  color: var(--app-text-soft) !important;
}

@media (max-width: 840px) {
  .page-shell {
    padding: 18px 16px 28px;
  }

  .dashboard-list {
    max-height: none;
  }

  .stats-grid,
  .content-grid,
  .settings-grid,
  .form-grid,
  .mapping-grid {
    grid-template-columns: 1fr;
  }

  .view-header,
  .panel-head,
  .mapping-head {
    flex-direction: column;
    align-items: stretch;
  }

  .dashboard-sync-btn {
    width: 100%;
  }
}

@media (max-width: 720px) {
  .stats-grid,
  .content-grid,
  .settings-grid,
  .form-grid,
  .mapping-grid {
    grid-template-columns: 1fr;
  }
}
</style>
