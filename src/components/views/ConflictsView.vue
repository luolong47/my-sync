<script setup lang="ts">
import type { FileMapping, MappingRuntime } from "../../types/app";

defineProps<{
  conflictItems: FileMapping[];
  runtimeStatus: (id: string) => MappingRuntime;
}>();

defineEmits<{
  resolve: [mappingId: string, strategy: "local" | "remote"];
}>();
</script>

<template>
  <div class="view-shell">
    <div class="view-header">
      <div>
        <div class="eyebrow">
          冲突处理
        </div>
      </div>
    </div>

    <div v-if="conflictItems.length === 0" class="empty-card panel-card theme-surface">
      当前没有待处理冲突
    </div>
    <div v-else class="conflict-list">
      <q-card
        v-for="item in conflictItems"
        :key="item.id"
        flat
        class="panel-card conflict-card theme-surface"
      >
        <div class="panel-head">
          <div>
            <div class="panel-title">
              {{ item.name || item.remotePath }}
            </div>
            <div class="panel-subtitle">
              {{ runtimeStatus(item.id).detail }}
            </div>
          </div>
          <q-chip class="status-pill status-pill--warning">
            conflict
          </q-chip>
        </div>
        <div class="conflict-meta">
          <div>本地：{{ item.localPath }}</div>
          <div>远端：{{ item.remotePath }}</div>
          <div v-if="runtimeStatus(item.id).lastConflictPath">
            冲突副本：{{ runtimeStatus(item.id).lastConflictPath }}
          </div>
        </div>
        <div class="row q-gutter-sm q-mt-md">
          <q-btn
            color="primary"
            unelevated
            label="以本地为准"
            @click="$emit('resolve', item.id, 'local')"
          />
          <q-btn
            outline
            color="primary"
            label="以远端为准"
            @click="$emit('resolve', item.id, 'remote')"
          />
        </div>
      </q-card>
    </div>
  </div>
</template>
