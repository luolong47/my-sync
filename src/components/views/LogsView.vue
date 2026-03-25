<script setup lang="ts">
import type { SyncLogEntry } from "../../types/app";

defineProps<{
  formatDateTime: (value: string | null) => string;
  logs: SyncLogEntry[];
}>();

defineEmits<{
  clearLogs: [];
  exportLogs: [];
}>();
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
      <div v-if="logs.length === 0" class="empty-block">
        还没有日志记录
      </div>
      <q-list v-else separator>
        <q-item v-for="entry in logs" :key="entry.id" class="log-item">
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
