<script setup lang="ts">
import { computed, ref } from "vue";
import type { SyncLogEntry } from "../../types/app";

const props = defineProps<{
  formatDateTime: (value: string | null) => string;
  logs: SyncLogEntry[];
}>();

defineEmits<{
  clearLogs: [];
  exportLogs: [];
}>();

const levelFilter = ref("all");
const actionFilter = ref("all");
const keyword = ref("");

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
</script>

<template>
  <div class="view-shell">
    <div class="view-header">
      <div>
        <div class="eyebrow">
          Logs
        </div>
        <h1>同步日志</h1>
      </div>
      <div class="row q-gutter-sm">
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
      </div>
    </div>

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
        还没有日志记录
      </div>
      <div v-else-if="filteredLogs.length === 0" class="empty-block">
        当前筛选条件下没有日志
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
            <q-item-label v-if="entry.httpStatus || entry.targetPath" caption>
              <span v-if="entry.httpStatus">HTTP {{ entry.httpStatus }}</span>
              <span v-if="entry.httpStatus && entry.targetPath"> · </span>
              <span v-if="entry.targetPath">目标 {{ entry.targetPath }}</span>
            </q-item-label>
          </q-item-section>
          <q-item-section side top class="log-time-side">
            {{ formatDateTime(entry.timestamp) }}
          </q-item-section>
        </q-item>
      </q-list>
    </q-card>
  </div>
</template>
