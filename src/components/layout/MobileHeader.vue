<script setup lang="ts">
defineProps<{
  isDarkMode: boolean;
}>();

defineEmits<{
  toggleDrawer: [];
  toggleTheme: [];
}>();
</script>

<template>
  <q-header class="mobile-header">
    <q-toolbar class="mobile-toolbar">
      <q-btn
        flat
        round
        color="white"
        icon="sym_r_menu"
        @click="$emit('toggleDrawer')"
      />
      <img src="/logo.png" style="width: 28px; height: 28px; border-radius: 4px; border: 1px solid rgba(255,255,255,0.2)" class="q-ml-sm">
      <q-toolbar-title class="mobile-title">
        <div class="brand-title">
          My Sync
        </div>
        <div class="brand-subtitle">
          极简配置同步
        </div>
      </q-toolbar-title>
      <q-btn
        flat
        round
        color="white"
        :icon="isDarkMode ? 'sym_r_light_mode' : 'sym_r_dark_mode'"
        @click="$emit('toggleTheme')"
      />
    </q-toolbar>
  </q-header>
</template>
