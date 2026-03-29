<script setup lang="ts">
import { computed, ref, onUnmounted, watch, nextTick } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SyncLogEntry } from "../../types/app";

const props = defineProps<{
  formatDateTime: (value: string | null) => string;
  logs: SyncLogEntry[];
}>();

defineEmits<{
  clearLogs: [];
  exportLogs: [];
}>();

const activeTab = ref("history");
const levelFilter = ref("all");
const actionFilter = ref("all");
const keyword = ref("");

interface LogPayload {
  message: string;
  level: number;
  timestamp: string;
}

// 实时日志状态
const isStreaming = ref(false);
const liveLogs = ref<LogPayload[]>([]);
const liveLogsContainer = ref<HTMLElement | null>(null);
let unlisten: UnlistenFn | null = null;

const levelOptions = [
  { label: "全部级别", value: "all" },
  { label: "成功", value: "info" },
  { label: "警告", value: "warning" },
  { label: "错误", value: "error" },
];

const actionOptions = computed(() => {
  const labels = new Map([
    ["sync-run", "同步流程"],
    ["fs-watch-sync", "文件监听"],
    ["pushed", "推送远端"],
    ["pulled", "拉取本地"],
    ["resolved", "冲突解决"],
    ["conflict", "检测冲突"],
    ["mapping-error", "映射失败"],
    ["resolve-conflict", "处理冲突"],
    ["download", "下载远端"],
    ["upload", "上传本地"],
    ["delete-remote", "删除远端"],
    ["rename-remote", "重命名远端"],
  ]);

  const values = new Set(props.logs.map((entry) => entry.action));
  return [
    { label: "全部动作", value: "all" },
    ...[...values].sort().map((value) => ({
      label: labels.get(value) ?? value,
      value,
    })),
  ];
});

const filteredLogs = computed(() => {
  const search = keyword.value.trim().toLowerCase();
  return props.logs.filter((entry) => {
    if (levelFilter.value !== "all" && entry.level !== levelFilter.value) {
      return false;
    }
    if (actionFilter.value !== "all" && entry.action !== actionFilter.value) {
      return false;
    }
    if (!search) {
      return true;
    }

    const fields = [
      entry.summary,
      entry.detail,
      entry.mappingName,
      entry.remotePath,
      entry.targetPath,
      entry.localPath,
    ]
      .filter(Boolean)
      .join(" ")
      .toLowerCase();

    return fields.includes(search);
  });
});

// 实时日志处理逻辑
const startStreaming = async () => {
  if (unlisten) return;
  isStreaming.value = true;
  unlisten = await listen<LogPayload>("log", (event) => {
    const payload = event.payload;
    liveLogs.value.push(payload);
    
    // 限制条数防止内存溢出
    if (liveLogs.value.length > 500) {
      liveLogs.value.shift();
    }
    
    // 自动滚动到底部
    nextTick(() => {
      if (liveLogsContainer.value) {
        liveLogsContainer.value.scrollTop = liveLogsContainer.value.scrollHeight;
      }
    });
  });
};

const stopStreaming = () => {
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
  isStreaming.value = false;
};

const clearLiveLogs = () => {
  liveLogs.value = [];
};

// 监听标签页切换
watch(activeTab, (newTab) => {
  if (newTab !== "live") {
    stopStreaming();
  }
});

// 离开页面时停止
onUnmounted(() => {
  stopStreaming();
});

const getLevelLabel = (level: number) => {
  switch (level) {
    case 1: return "ERROR";
    case 2: return "WARN";
    case 3: return "INFO";
    case 4: return "DEBUG";
    case 5: return "TRACE";
    default: return "LOG";
  }
};

const getLevelColor = (level: number) => {
  switch (level) {
    case 1: return "text-negative";
    case 2: return "text-warning";
    case 3: return "text-info";
    default: return "text-grey-5";
  }
};
</script>

<template>
  <div class="view-shell">
    <div class="view-header">
      <div class="row items-center q-gutter-md">
        <div class="eyebrow">
          日志
        </div>
        <q-tabs
          v-model="activeTab"
          dense
          class="text-grey"
          active-color="primary"
          indicator-color="primary"
          align="left"
          narrow-indicator
        >
          <q-tab name="history" label="同步历史" />
          <q-tab name="live" label="实时诊断" />
        </q-tabs>
      </div>
      
      <div class="row q-gutter-sm">
        <template v-if="activeTab === 'history'">
          <q-btn
            flat
            class="subtle-action"
            label="导出"
            @click="$emit('exportLogs')"
          />
          <q-btn
            flat
            class="danger-action"
            label="清空"
            @click="$emit('clearLogs')"
          />
        </template>
        <template v-else>
          <q-btn
            flat
            :color="isStreaming ? 'negative' : 'primary'"
            :label="isStreaming ? '停止获取' : '获取日志'"
            @click="isStreaming ? stopStreaming() : startStreaming()"
          />
          <q-btn
            flat
            class="subtle-action"
            label="清屏"
            @click="clearLiveLogs"
          />
        </template>
      </div>
    </div>

    <div v-show="activeTab === 'history'">
      <q-card flat class="panel-card theme-surface">
        <div class="logs-filter-bar q-mb-md">
          <div class="logs-filter-cell logs-filter-cell--narrow">
            <q-select
              v-model="levelFilter"
              outlined
              dense
              emit-value
              map-options
              prefix="级别"
              class="compact-field logs-toolbar-field"
              :options="levelOptions"
              options-dark
              popup-content-class="app-select-menu"
            />
          </div>
          <div class="logs-filter-cell logs-filter-cell--narrow">
            <q-select
              v-model="actionFilter"
              outlined
              dense
              emit-value
              map-options
              prefix="动作"
              class="compact-field logs-toolbar-field"
              :options="actionOptions"
              options-dark
              popup-content-class="app-select-menu"
            />
          </div>
          <div class="logs-filter-cell logs-filter-cell--search">
            <q-input
              v-model="keyword"
              outlined
              dense
              clearable
              prefix="搜索"
              placeholder="搜索映射名、路径、说明"
              class="compact-field logs-toolbar-field"
            />
          </div>
        </div>

        <div v-if="logs.length === 0" class="empty-block">
          还没有同步历史记录
        </div>
        <div v-else-if="filteredLogs.length === 0" class="empty-block">
          当前筛选条件下没有记录
        </div>
        <q-list v-else separator>
          <q-item v-for="entry in filteredLogs" :key="entry.id" class="log-item">
            <q-item-section avatar>
              <q-icon
                :name="entry.level === 'error' ? 'sym_r_error' : entry.level === 'warning' ? 'sym_r_warning' : 'sym_r_check_circle'"
                :color="entry.level === 'error' ? 'negative' : entry.level === 'warning' ? 'warning' : 'positive'"
              />
            </q-item-section>
            <q-item-section>
              <q-item-label>{{ entry.summary }}</q-item-label>
              <q-item-label caption>
                {{ entry.detail }}
              </q-item-label>
              <q-item-label v-if="entry.mappingName || entry.remotePath" caption>
                {{ entry.mappingName || "未命名映射" }} · {{ entry.remotePath || "无远端路径" }}
              </q-item-label>
            </q-item-section>
            <q-item-section side top class="log-time-side">
              {{ formatDateTime(entry.timestamp) }}
            </q-item-section>
          </q-item>
        </q-list>
      </q-card>
    </div>

    <div v-show="activeTab === 'live'" class="full-height column">
      <q-card flat class="panel-card theme-surface col column">
        <div 
          ref="liveLogsContainer"
          class="live-log-console col q-pa-md overflow-auto bg-black text-grey-4"
          style="font-family: 'Cascadia Code', Consolas, monospace; font-size: 12px; line-height: 1.4;"
        >
          <div v-if="liveLogs.length === 0" class="text-grey-7">
            {{ isStreaming ? '正在等待日志输入...' : '点击“获取日志”开始实时监控程序运行状态' }}
          </div>
          <div v-for="(log, index) in liveLogs" :key="index" class="q-mb-xs">
            <span class="text-grey-6">[{{ log.timestamp.split('T')[1].substring(0, 12) }}]</span>
            <span :class="['q-ml-sm text-bold', getLevelColor(log.level)]">{{ getLevelLabel(log.level).padEnd(5) }}</span>
            <span class="q-ml-sm">{{ log.message }}</span>
          </div>
        </div>
      </q-card>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.live-log-console {
  border-radius: 4px;
  min-height: 400px;
  white-space: pre-wrap;
  word-break: break-all;
}

// 模拟终端滚动条
.live-log-console::-webkit-scrollbar {
  width: 8px;
}
.live-log-console::-webkit-scrollbar-thumb {
  background: #333;
  border-radius: 4px;
}
.live-log-console::-webkit-scrollbar-track {
  background: #111;
}
</style>
