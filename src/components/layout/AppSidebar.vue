<script setup lang="ts">
import type { NavItem, RuntimeSnapshot, TabKey } from "../../types/app";

defineProps<{
  currentTab: TabKey;
  isDarkMode: boolean;
  modelValue: boolean;
  navItems: NavItem[];
  runtime: RuntimeSnapshot;
}>();

defineEmits<{
  "update:modelValue": [value: boolean];
  selectTab: [tab: TabKey];
  toggleTheme: [];
}>();
</script>

<template>
  <q-drawer
    :model-value="modelValue"
    show-if-above
    :breakpoint="820"
    :width="188"
    class="sidebar"
    @update:model-value="$emit('update:modelValue', $event)"
  >
    <div class="sidebar-brand">
      <q-icon name="sym_r_sync" size="30px" color="primary" />
      <div>
        <div class="brand-title">
          My Sync
        </div>
        <div class="brand-subtitle">
          Sparse Config Sync
        </div>
      </div>
      <q-space />
      <q-btn
        flat
        round
        color="primary"
        :icon="isDarkMode ? 'sym_r_light_mode' : 'sym_r_dark_mode'"
        @click="$emit('toggleTheme')"
      />
    </div>

    <q-list padding>
      <q-item
        v-for="item in navItems"
        :key="item.key"
        v-ripple
        clickable
        :active="currentTab === item.key"
        class="nav-item"
        @click="$emit('selectTab', item.key)"
      >
        <q-item-section avatar>
          <q-icon :name="item.icon" />
        </q-item-section>
        <q-item-section>{{ item.label }}</q-item-section>
      </q-item>
    </q-list>

    <div class="sidebar-footer">
      <div class="sync-status" :class="runtime.isReady ? 'ready' : 'pending'">
        <div class="status-dot" />
        <div>
          <strong>{{ runtime.isSyncing ? "同步中" : runtime.isReady ? "系统就绪" : "待处理" }}</strong>
          <div>{{ runtime.readinessDetail }}</div>
        </div>
      </div>
    </div>
  </q-drawer>
</template>
