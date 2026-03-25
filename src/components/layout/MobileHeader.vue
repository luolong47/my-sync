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
        color="primary"
        icon="sym_r_menu"
        @click="$emit('toggleDrawer')"
      />
      <q-toolbar-title class="mobile-title">
        <div class="brand-title">
          My Sync
        </div>
        <div class="brand-subtitle">
          Sparse Config Sync
        </div>
      </q-toolbar-title>
      <q-btn
        flat
        round
        color="primary"
        :icon="isDarkMode ? 'sym_r_light_mode' : 'sym_r_dark_mode'"
        @click="$emit('toggleTheme')"
      />
    </q-toolbar>
  </q-header>
</template>
