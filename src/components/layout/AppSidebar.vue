<script setup lang="ts">
import type { NavItem, RuntimeSnapshot, TabKey } from "../../types/app";

defineProps<{
  currentTab: TabKey;
  isDarkMode: boolean;
  isMini: boolean;
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
    :mini="isMini"
    :breakpoint="500"
    :width="188"
    class="sidebar"
    @update:model-value="$emit('update:modelValue', $event)"
  >
    <q-list padding class="q-mt-sm">
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
        <div class="status-dot tooltip-target">
          <q-tooltip
            v-if="isMini"
            anchor="center right"
            self="center left"
            :offset="[10, 0]"
          >
            {{ runtime.isReady ? "系统就绪" : "待处理" }}
          </q-tooltip>
        </div>
        <div v-show="!isMini">
          <strong>{{ runtime.isSyncing ? "同步中" : runtime.isReady ? "系统就绪" : "待处理" }}</strong>
          <div>{{ runtime.isSyncing ? "正在与远端保持同步..." : (runtime.lastSummary || runtime.readinessDetail) }}</div>
        </div>
      </div>
    </div>
  </q-drawer>
</template>
