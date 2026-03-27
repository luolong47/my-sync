<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import { storeToRefs } from "pinia";
import { useQuasar } from "quasar";
import AppSidebar from "./components/layout/AppSidebar.vue";
import MobileHeader from "./components/layout/MobileHeader.vue";
import AboutView from "./components/views/AboutView.vue";
import ConflictsView from "./components/views/ConflictsView.vue";
import DashboardView from "./components/views/DashboardView.vue";
import FilesView from "./components/views/FilesView.vue";
import LogsView from "./components/views/LogsView.vue";
import SettingsView from "./components/views/SettingsView.vue";
import { navItems } from "./constants/navigation";
import { useSyncAppStore } from "./stores/syncApp";

const $q = useQuasar();
const store = useSyncAppStore();
const {
  canSync,
  config,
  conflictItems,
  currentTab,
  isAutoSaving,
  isBooting,
  isDarkMode,
  isLoadingRemote,
  isMappingDropActive,
  leftDrawerOpen,
  logs,
  recentLogs,
  remoteEntries,
  remotePath,
  remotePathLabel,
  remoteRootPath,
  runtime,
} = storeToRefs(store);

const currentView = computed(() => {
  switch (currentTab.value) {
    case "dashboard":
      return DashboardView;
    case "files":
      return FilesView;
    case "logs":
      return LogsView;
    case "conflicts":
      return ConflictsView;
    case "settings":
      return SettingsView;
    default:
      return AboutView;
  }
});

const currentViewProps = computed(() => {
  switch (currentTab.value) {
    case "dashboard":
      return {
        canSync: canSync.value,
        formatDateTime: store.formatDateTime,
        mappings: config.value.mappings,
        recentLogs: recentLogs.value,
        runtime: runtime.value,
        runtimeStatus: store.runtimeStatus,
        statusTone: store.statusTone,
      };
    case "files":
      return {
        config: config.value,
        isLoadingRemote: isLoadingRemote.value,
        mappings: config.value.mappings,
        remoteEntries: remoteEntries.value,
        remotePath: remotePath.value,
        remotePathLabel: remotePathLabel.value,
        runtimeStatus: store.runtimeStatus,
        statusTone: store.statusTone,
      };
    case "logs":
      return {
        formatDateTime: store.formatDateTime,
        logs: logs.value,
      };
    case "conflicts":
      return {
        conflictItems: conflictItems.value,
        runtimeStatus: store.runtimeStatus,
      };
    case "settings":
      return {
        config: config.value,
        fullRemotePath: store.fullRemotePath,
        isAutoSaving: isAutoSaving.value,
        isMappingDropActive: isMappingDropActive.value,
        remoteRootPath: remoteRootPath.value,
        runtimeStatus: store.runtimeStatus,
        statusTone: store.statusTone,
      };
    default:
      return {};
  }
});

function handleSelectTab(tab: typeof currentTab.value) {
  store.selectTab(tab, $q.screen.lt.md);
}

function handleToggleTheme() {
  store.applyTheme(!isDarkMode.value);
}

onMounted(async () => {
  await store.bootstrap($q.screen.gt.sm);
});

onBeforeUnmount(() => {
  store.teardown();
});
</script>

<template>
  <q-layout view="lHh Lpr lFf">
    <MobileHeader
      v-if="$q.screen.lt.md"
      :is-dark-mode="isDarkMode"
      @toggle-drawer="store.toggleDrawer"
      @toggle-theme="handleToggleTheme"
    />

    <AppSidebar
      v-model="leftDrawerOpen"
      :current-tab="currentTab"
      :is-dark-mode="isDarkMode"
      :nav-items="navItems"
      :runtime="runtime"
      @select-tab="handleSelectTab"
      @toggle-theme="handleToggleTheme"
    />

    <q-page-container>
      <q-page class="page-shell">
        <component
          :is="currentView"
          v-bind="currentViewProps"
          v-model:config="config"
          @add-mapping="store.addMapping"
          @browse="store.loadRemoteFiles"
          @choose-file="store.chooseFile"
          @clear-logs="store.clearLogs"
          @clear-binding="store.clearBinding"
          @config-updated="store.handleConfigUpdated"
          @create-directory="store.createRemoteDirectory"
          @delete-entry="store.deleteRemoteEntry"
          @download-file="store.downloadRemoteFile"
          @export-logs="store.exportLogs"
          @map-entry="store.mapRemoteEntryToLocal"
          @navigate="handleSelectTab"
          @rename-mapping="store.renameMapping"
          @remove-mapping="store.removeMapping"
          @rename-entry="store.renameRemoteEntry"
          @resolve="store.resolveConflict"
          @sync-now="store.syncNow"
          @upload-file="store.uploadLocalFile"
        />

        <q-inner-loading :showing="isBooting">
          <q-spinner-dots size="50px" color="primary" />
        </q-inner-loading>
      </q-page>
    </q-page-container>
  </q-layout>
</template>

<style src="./styles/app-shell.scss" lang="scss"></style>
