<script setup lang="ts">
import type { FileMapping, MappingRuntime, RuntimeSnapshot, SyncLogEntry, TabKey } from "../../types/app";

defineProps<{
  canSync: boolean;
  formatDateTime: (value: string | null) => string;
  mappings: FileMapping[];
  recentLogs: SyncLogEntry[];
  runtime: RuntimeSnapshot;
  runtimeStatus: (id: string) => MappingRuntime;
  statusTone: (status: string) => string;
  isDarkMode: boolean;
}>();

defineEmits<{
  navigate: [tab: TabKey];
  syncNow: [];
  toggleTheme: [];
}>();
</script>

<template>
  <div class="view-shell">
    <div class="view-header dashboard-header">
      <div class="row items-center q-gutter-x-sm">
        <q-btn
          class="dashboard-sync-btn"
          color="primary"
          unelevated
          round
          icon="sym_r_sync"
          :disable="!canSync || runtime.isSyncing"
          :loading="runtime.isSyncing"
          @click="$emit('syncNow')"
        >
          <q-tooltip>立即同步</q-tooltip>
        </q-btn>
        <div class="eyebrow">
          仪表盘概览
        </div>
      </div>
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
          <div class="panel-title">
            最近日志
          </div>
          <q-btn
            flat
            class="subtle-action"
            label="查看全部"
            @click="$emit('navigate', 'logs')"
          />
        </div>
        <div class="dashboard-panel-body">
          <div v-if="recentLogs.length === 0" class="empty-block">
            还没有同步日志
          </div>
          <q-list v-else separator class="dashboard-list">
            <q-item v-for="entry in recentLogs" :key="entry.id" class="dashboard-log-item">
              <q-item-section avatar>
                <q-icon
                  :name="entry.level === 'error' ? 'sym_r_error' : entry.level === 'warning' ? 'sym_r_warning' : 'sym_r_check_circle'"
                  :color="entry.level === 'error' ? 'negative' : entry.level === 'warning' ? 'warning' : 'positive'"
                />
              </q-item-section>
              <q-item-section class="dashboard-log-main">
                <q-item-label class="dashboard-log-summary">
                  {{ entry.summary }}
                </q-item-label>
                <q-item-label caption class="dashboard-log-detail">
                  {{ entry.detail }}
                </q-item-label>
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
          <div class="panel-title">
            同步项状态
          </div>
          <q-btn
            flat
            class="subtle-action"
            label="前往设置"
            @click="$emit('navigate', 'settings')"
          />
        </div>
        <div class="dashboard-panel-body">
          <q-list v-if="mappings.length > 0" separator class="dashboard-list">
            <q-item v-for="item in mappings" :key="item.id">
              <q-item-section avatar style="min-width: 28px">
                <div class="status-dot" :class="`status-dot--${statusTone(runtimeStatus(item.id).status)}`">
                  <q-tooltip anchor="center right" self="center left" :offset="[10, 0]">
                    {{ runtimeStatus(item.id).status }}
                  </q-tooltip>
                </div>
              </q-item-section>
              <q-item-section>
                <q-item-label>{{ item.name || item.remotePath || item.localPath }}</q-item-label>
                <q-item-label caption>
                  {{ runtimeStatus(item.id).detail }}
                </q-item-label>
              </q-item-section>
            </q-item>
          </q-list>
          <div v-else class="empty-block">
            还没有文件映射
          </div>
        </div>
      </q-card>
    </div>
  </div>
</template>
