<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
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
  syncIntervalSecs: number;
  autoSync: boolean;
};

type AppConfig = {
  webdav: WebDavSettings;
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
  lastRunAt: string | null;
  lastSummary: string;
  mappings: MappingRuntime[];
};

type AppSnapshot = {
  config: AppConfig;
  runtime: RuntimeSnapshot;
};

const $q = useQuasar();

const config = reactive<AppConfig>({
  webdav: {
    baseUrl: "",
    username: "",
    password: "",
    remoteDir: "configs",
    syncIntervalSecs: 30,
    autoSync: true,
  },
  mappings: [],
});

const runtime = ref<RuntimeSnapshot>({
  isSyncing: false,
  lastRunAt: null,
  lastSummary: "尚未同步",
  mappings: [],
});

const isSaving = ref(false);
const isBooting = ref(true);
let refreshTimer: number | undefined;

const runtimeById = computed(() =>
  new Map(runtime.value.mappings.map((item) => [item.mappingId, item])),
);

const canSync = computed(
  () =>
    !!config.webdav.baseUrl.trim() &&
    !!config.webdav.username.trim() &&
    !!config.webdav.remoteDir.trim() &&
    config.mappings.length > 0,
);

function createMapping(): FileMapping {
  return {
    id: crypto.randomUUID(),
    name: "",
    localPath: "",
    remotePath: "",
  };
}

function resetConfig(next: AppConfig) {
  config.webdav.baseUrl = next.webdav.baseUrl ?? "";
  config.webdav.username = next.webdav.username ?? "";
  config.webdav.password = next.webdav.password ?? "";
  config.webdav.remoteDir = next.webdav.remoteDir ?? "configs";
  config.webdav.syncIntervalSecs = next.webdav.syncIntervalSecs ?? 30;
  config.webdav.autoSync = next.webdav.autoSync ?? true;
  config.mappings.splice(
    0,
    config.mappings.length,
    ...(next.mappings ?? []).map((item) => ({
      id: item.id,
      name: item.name,
      localPath: item.localPath,
      remotePath: item.remotePath,
    })),
  );
}

function snapshotConfig(): AppConfig {
  return {
    webdav: {
      baseUrl: config.webdav.baseUrl.trim(),
      username: config.webdav.username.trim(),
      password: config.webdav.password,
      remoteDir: config.webdav.remoteDir.trim(),
      syncIntervalSecs: Number(config.webdav.syncIntervalSecs) || 30,
      autoSync: !!config.webdav.autoSync,
    },
    mappings: config.mappings.map((item) => ({
      id: item.id,
      name: item.name.trim(),
      localPath: item.localPath.trim(),
      remotePath: item.remotePath.trim().replace(/\\/g, "/"),
    })),
  };
}

async function loadAppState() {
  const snapshot = await invoke<AppSnapshot>("load_app_state");
  resetConfig(snapshot.config);
  runtime.value = snapshot.runtime;
}

async function refreshRuntime() {
  runtime.value = await invoke<RuntimeSnapshot>("get_runtime_state");
}

async function saveConfig() {
  isSaving.value = true;
  try {
    const nextConfig = snapshotConfig();
    runtime.value = await invoke<RuntimeSnapshot>("save_app_config", {
      config: nextConfig,
    });
    $q.notify({ type: "positive", message: "配置已保存" });
  } catch (error) {
    $q.notify({
      type: "negative",
      message: `保存失败：${String(error)}`,
    });
  } finally {
    isSaving.value = false;
  }
}

async function syncNow() {
  try {
    runtime.value = await invoke<RuntimeSnapshot>("sync_now");
    $q.notify({ type: "positive", message: "同步已执行" });
  } catch (error) {
    $q.notify({
      type: "negative",
      message: `同步失败：${String(error)}`,
    });
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

  mapping.localPath = selected;
  if (!mapping.name.trim()) {
    const normalized = selected.replace(/\\/g, "/");
    const parts = normalized.split("/");
    mapping.name = parts[parts.length - 1] || "未命名配置";
  }
  if (!mapping.remotePath.trim()) {
    mapping.remotePath = `${mapping.name || "config"}`.replace(/ /g, "-");
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

function runtimeStatus(id: string) {
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

function statusColor(status: string) {
  switch (status) {
    case "synced":
      return "positive";
    case "pushed":
      return "primary";
    case "pulled":
      return "accent";
    case "conflict":
      return "warning";
    case "error":
      return "negative";
    default:
      return "grey-7";
  }
}

onMounted(async () => {
  try {
    await loadAppState();
  } catch (error) {
    $q.notify({
      type: "negative",
      message: `初始化失败：${String(error)}`,
    });
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
});
</script>

<template>
  <q-layout view="lHh Lpr lFf">
    <q-page-container>
      <q-page class="page-shell">
        <div class="hero-card">
          <div class="hero-copy">
            <div class="eyebrow">Sparse Config Sync</div>
            <h1>My Sync</h1>
            <p>
              把系统里零散的配置文件映射到 WebDAV，跨设备自动双向同步。不是整目录，不碰无关文件。
            </p>
          </div>
          <div class="hero-actions">
            <q-btn
              color="primary"
              unelevated
              label="立即同步"
              :disable="!canSync || runtime.isSyncing"
              @click="syncNow"
            />
            <q-btn
              color="dark"
              flat
              label="保存配置"
              :loading="isSaving"
              @click="saveConfig"
            />
          </div>
        </div>

        <div class="summary-grid">
          <q-card flat class="summary-card">
            <span class="summary-label">同步状态</span>
            <strong>{{ runtime.isSyncing ? "同步中" : "空闲" }}</strong>
            <span>{{ runtime.lastSummary }}</span>
          </q-card>
          <q-card flat class="summary-card">
            <span class="summary-label">最近一次</span>
            <strong>{{ runtime.lastRunAt || "尚未执行" }}</strong>
            <span>轮询间隔 {{ config.webdav.syncIntervalSecs || 30 }} 秒</span>
          </q-card>
          <q-card flat class="summary-card">
            <span class="summary-label">映射数量</span>
            <strong>{{ config.mappings.length }}</strong>
            <span>每个映射只同步一个明确文件</span>
          </q-card>
        </div>

        <div class="content-grid">
          <q-card flat class="panel-card">
            <div class="panel-head">
              <div>
                <div class="panel-title">WebDAV 设置</div>
                <div class="panel-subtitle">凭据保存在本机应用配置目录</div>
              </div>
            </div>

            <div class="form-grid">
              <q-input
                v-model="config.webdav.baseUrl"
                outlined
                label="WebDAV 地址"
                placeholder="https://dav.example.com/remote.php/dav/files/name"
              />
              <q-input
                v-model="config.webdav.remoteDir"
                outlined
                label="远端根目录"
                placeholder="configs"
              />
              <q-input
                v-model="config.webdav.username"
                outlined
                label="用户名"
              />
              <q-input
                v-model="config.webdav.password"
                outlined
                label="密码 / App Password"
                type="password"
              />
              <q-input
                v-model.number="config.webdav.syncIntervalSecs"
                outlined
                type="number"
                min="10"
                label="自动同步间隔（秒）"
              />
              <div class="toggle-box">
                <q-toggle
                  v-model="config.webdav.autoSync"
                  label="启用后台自动同步"
                  color="primary"
                />
              </div>
            </div>
          </q-card>

          <q-card flat class="panel-card">
            <div class="panel-head">
              <div>
                <div class="panel-title">文件映射</div>
                <div class="panel-subtitle">
                  同一份远端路径可以在不同设备指向不同本地绝对路径
                </div>
              </div>
              <q-btn
                color="primary"
                unelevated
                icon="sym_r_add"
                label="新增映射"
                @click="addMapping"
              />
            </div>

            <div v-if="config.mappings.length === 0" class="empty-state">
              <div class="empty-title">还没有同步项</div>
              <p>先添加几个配置文件，比如 PowerShell、Git、SSH、IDE 配置。</p>
            </div>

            <div v-else class="mapping-list">
              <q-card
                v-for="item in config.mappings"
                :key="item.id"
                flat
                class="mapping-card"
              >
                <div class="mapping-head">
                  <div class="mapping-title-wrap">
                    <q-input
                      v-model="item.name"
                      dense
                      borderless
                      placeholder="映射名称"
                      class="mapping-title"
                    />
                    <q-chip
                      dense
                      :color="statusColor(runtimeStatus(item.id).status)"
                      text-color="white"
                    >
                      {{ runtimeStatus(item.id).status }}
                    </q-chip>
                  </div>
                  <q-btn
                    round
                    flat
                    color="negative"
                    icon="sym_r_delete"
                    @click="removeMapping(item.id)"
                  />
                </div>

                <div class="mapping-grid">
                  <q-input
                    v-model="item.localPath"
                    outlined
                    label="本地文件"
                    placeholder="C:\\Users\\you\\.gitconfig"
                  >
                    <template #append>
                      <q-btn flat dense icon="sym_r_folder_open" @click="chooseFile(item)" />
                    </template>
                  </q-input>
                  <q-input
                    v-model="item.remotePath"
                    outlined
                    label="远端相对路径"
                    placeholder="git/.gitconfig"
                  />
                </div>

                <div class="mapping-meta">
                  <span>{{ runtimeStatus(item.id).detail }}</span>
                  <span v-if="runtimeStatus(item.id).lastSyncAt">
                    上次成功：{{ runtimeStatus(item.id).lastSyncAt }}
                  </span>
                  <span v-if="runtimeStatus(item.id).lastConflictPath">
                    冲突副本：{{ runtimeStatus(item.id).lastConflictPath }}
                  </span>
                </div>
              </q-card>
            </div>
          </q-card>
        </div>

        <q-inner-loading :showing="isBooting">
          <q-spinner-gears size="48px" color="primary" />
        </q-inner-loading>
      </q-page>
    </q-page-container>
  </q-layout>
</template>

<style lang="scss">
:root {
  color: #17201f;
  background:
    radial-gradient(circle at top left, rgba(244, 199, 133, 0.4), transparent 30%),
    radial-gradient(circle at 90% 10%, rgba(92, 143, 255, 0.18), transparent 20%),
    linear-gradient(160deg, #f6f1e8 0%, #eef3f7 52%, #dce5ea 100%);
  font-family: "Segoe UI", "PingFang SC", sans-serif;
}

body {
  margin: 0;
}

#app {
  min-height: 100vh;
}

.page-shell {
  max-width: 1240px;
  margin: 0 auto;
  padding: 32px 20px 40px;
}

.hero-card,
.panel-card,
.summary-card,
.mapping-card {
  background: rgba(255, 255, 255, 0.78);
  backdrop-filter: blur(18px);
  border: 1px solid rgba(23, 32, 31, 0.08);
  box-shadow: 0 24px 60px rgba(31, 45, 61, 0.08);
}

.hero-card {
  display: flex;
  justify-content: space-between;
  gap: 24px;
  padding: 28px;
  border-radius: 28px;
}

.hero-copy h1 {
  margin: 6px 0 12px;
  font-size: 52px;
  line-height: 1;
  letter-spacing: -0.04em;
}

.hero-copy p {
  max-width: 760px;
  margin: 0;
  font-size: 16px;
  line-height: 1.7;
  color: rgba(23, 32, 31, 0.72);
}

.eyebrow {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: #476b63;
}

.hero-actions {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  flex-wrap: wrap;
}

.summary-grid,
.content-grid {
  display: grid;
  gap: 18px;
  margin-top: 18px;
}

.summary-grid {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.summary-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 18px 20px;
  border-radius: 22px;
}

.summary-card strong {
  font-size: 28px;
  line-height: 1.2;
}

.summary-label {
  font-size: 12px;
  color: rgba(23, 32, 31, 0.56);
}

.content-grid {
  grid-template-columns: 0.95fr 1.25fr;
  align-items: start;
}

.panel-card {
  border-radius: 28px;
  padding: 22px;
}

.panel-head {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
  margin-bottom: 18px;
}

.panel-title {
  font-size: 24px;
  font-weight: 700;
}

.panel-subtitle {
  margin-top: 6px;
  color: rgba(23, 32, 31, 0.58);
}

.form-grid,
.mapping-grid {
  display: grid;
  gap: 14px;
}

.form-grid {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.toggle-box {
  display: flex;
  align-items: center;
  padding: 0 8px;
}

.empty-state {
  padding: 18px;
  border-radius: 20px;
  background: rgba(71, 107, 99, 0.08);
}

.empty-title {
  font-size: 18px;
  font-weight: 700;
}

.mapping-list {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.mapping-card {
  border-radius: 22px;
  padding: 16px;
}

.mapping-head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: center;
}

.mapping-title-wrap {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.mapping-title {
  min-width: 180px;
  font-size: 18px;
  font-weight: 700;
}

.mapping-grid {
  grid-template-columns: 1.3fr 1fr;
  margin-top: 12px;
}

.mapping-meta {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
  color: rgba(23, 32, 31, 0.66);
  word-break: break-all;
}

@media (max-width: 960px) {
  .hero-card,
  .panel-head {
    flex-direction: column;
  }

  .summary-grid,
  .content-grid,
  .form-grid,
  .mapping-grid {
    grid-template-columns: 1fr;
  }

  .hero-copy h1 {
    font-size: 40px;
  }
}
</style>
