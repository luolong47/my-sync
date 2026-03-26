<script setup lang="ts">
import { computed, ref } from "vue";
import type { AppConfig, FileMapping, RemoteBrowserEntry, TabKey } from "../../types/app";

const props = defineProps<{
  config: AppConfig;
  isLoadingRemote: boolean;
  mappings: FileMapping[];
  remoteEntries: RemoteBrowserEntry[];
  remotePath: string;
  remotePathLabel: string;
  runtimeStatus: (id: string) => { status: string; detail: string };
  statusTone: (status: string) => string;
}>();

const emit = defineEmits<{
  browse: [path: string];
  chooseFile: [mapping: FileMapping];
  createDirectory: [];
  deleteEntry: [entry: RemoteBrowserEntry];
  downloadFile: [entry: RemoteBrowserEntry];
  mapEntry: [entry: RemoteBrowserEntry];
  navigate: [tab: TabKey];
  renameEntry: [entry: RemoteBrowserEntry];
  renameMapping: [mapping: FileMapping];
  removeMapping: [id: string];
  uploadFile: [];
}>();

const onlyMapped = ref(false);
const sortKey = ref<"name" | "modifiedAt">("name");
const sortOrder = ref<"asc" | "desc">("asc");

const mappedByRemotePath = computed(
  () => new Map(props.mappings.map((item) => [item.remotePath, item])),
);

const breadcrumbItems = computed(() => {
  const rootLabel = [props.config.webdav.remoteDir, props.config.webdav.clientId]
    .filter(Boolean)
    .join("/");
  const segments = props.remotePath.split("/").filter(Boolean);
  const items = [{ label: rootLabel || "远端根目录", path: "" }];

  let current = "";
  for (const segment of segments) {
    current = current ? `${current}/${segment}` : segment;
    items.push({ label: segment, path: current });
  }

  return items;
});

const filteredEntries = computed(() => {
  const visible = props.remoteEntries.filter(
    (entry) => !onlyMapped.value || mappedByRemotePath.value.has(entry.path),
  );

  return [...visible].sort((left, right) => {
    if (left.isDir !== right.isDir) {
      return left.isDir ? -1 : 1;
    }

    if (sortKey.value === "modifiedAt") {
      const leftTime = left.modifiedAt ? new Date(left.modifiedAt).getTime() : 0;
      const rightTime = right.modifiedAt ? new Date(right.modifiedAt).getTime() : 0;
      if (leftTime !== rightTime) {
        return sortOrder.value === "asc" ? leftTime - rightTime : rightTime - leftTime;
      }
    }

    const delta = left.name.localeCompare(right.name, "zh-CN");
    return sortOrder.value === "asc" ? delta : -delta;
  });
});

function toggleSort(nextKey: "name" | "modifiedAt") {
  if (sortKey.value === nextKey) {
    sortOrder.value = sortOrder.value === "asc" ? "desc" : "asc";
    return;
  }

  sortKey.value = nextKey;
  sortOrder.value = nextKey === "modifiedAt" ? "desc" : "asc";
}

function formatEntryTime(value: string | null) {
  if (!value) {
    return "未记录";
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "numeric",
    day: "numeric",
  }).format(date);
}

function formatEntrySize(entry: RemoteBrowserEntry) {
  if (entry.isDir) {
    return "—";
  }

  const size = entry.size ?? 0;
  if (size < 1024) {
    return `${size} B`;
  }
  if (size < 1024 * 1024) {
    return `${Math.round((size / 1024) * 10) / 10} KB`;
  }

  return `${Math.round((size / (1024 * 1024)) * 10) / 10} MB`;
}

function handleOpen(entry: RemoteBrowserEntry) {
  if (entry.isDir) {
    emit("browse", entry.path);
  }
}

function entryMapping(entry: RemoteBrowserEntry) {
  return mappedByRemotePath.value.get(entry.path);
}

function handleRemoveMapping(mapping: FileMapping) {
  if (!window.confirm(`确认解除映射“${mapping.name || mapping.remotePath}”？`)) {
    return;
  }

  emit("removeMapping", mapping.id);
}
</script>

<template>
  <div class="view-shell">
    <div class="view-header">
      <div>
        <div class="eyebrow">
          Files
        </div>
        <h1>远端文件</h1>
      </div>
      <div class="row q-gutter-sm page-actions">
        <q-btn
          unelevated
          color="primary"
          icon="sym_r_create_new_folder"
          label="新建目录"
          @click="emit('createDirectory')"
        />
        <q-btn
          unelevated
          color="primary"
          icon="sym_r_upload_file"
          label="上传文件"
          @click="emit('uploadFile')"
        />
        <q-btn
          flat
          class="subtle-action"
          icon="sym_r_arrow_upward"
          label="返回上级"
          :disable="!remotePath"
          @click="emit('browse', remotePath.split('/').slice(0, -1).join('/'))"
        />
      </div>
    </div>

    <q-card flat class="panel-card theme-surface drive-list-card">
      <div class="drive-list-toolbar">
        <div class="drive-list-heading">
          <div class="panel-title">
            我的云端硬盘
          </div>
          <q-breadcrumbs active-color="primary" class="drive-breadcrumbs">
            <q-breadcrumbs-el
              v-for="item in breadcrumbItems"
              :key="item.path"
              :label="item.label"
              class="drive-breadcrumb"
              @click="emit('browse', item.path)"
            />
          </q-breadcrumbs>
        </div>
        <div class="drive-list-controls">
          <q-toggle
            v-model="onlyMapped"
            color="primary"
            label="只看已映射"
          />
          <q-btn
            flat
            round
            dense
            class="subtle-action"
            icon="sym_r_refresh"
            @click="emit('browse', remotePath)"
          />
          <q-btn
            flat
            round
            dense
            class="subtle-action"
            icon="sym_r_settings"
            @click="emit('navigate', 'settings')"
          />
        </div>
      </div>

      <div class="drive-table-head">
        <button type="button" class="file-sort-button" @click="toggleSort('name')">
          名称
          <q-icon
            v-if="sortKey === 'name'"
            :name="sortOrder === 'asc' ? 'sym_r_arrow_upward' : 'sym_r_arrow_downward'"
          />
        </button>
        <button type="button" class="file-sort-button" @click="toggleSort('modifiedAt')">
          修改日期
          <q-icon
            v-if="sortKey === 'modifiedAt'"
            :name="sortOrder === 'asc' ? 'sym_r_arrow_upward' : 'sym_r_arrow_downward'"
          />
        </button>
        <div>文件大小</div>
        <div class="text-right">
          更多
        </div>
      </div>

      <div v-if="isLoadingRemote" class="empty-block">
        正在读取远端目录...
      </div>
      <div v-else-if="filteredEntries.length === 0" class="empty-block">
        {{ onlyMapped ? "当前目录没有已映射文件" : "当前目录为空" }}
      </div>
      <div v-else class="drive-table-body">
        <div
          v-for="entry in filteredEntries"
          :key="entry.path"
          class="drive-row"
          :class="{ 'drive-row--clickable': entry.isDir }"
          @click="handleOpen(entry)"
        >
          <div class="drive-row__name">
            <q-icon
              class="drive-row__icon"
              :name="entry.isDir ? 'sym_r_folder' : 'sym_r_description'"
              :color="entry.isDir ? 'grey-9' : 'primary'"
            />
            <div class="drive-row__text">
              <div class="drive-row__title">
                <span class="drive-row__title-text">{{ entry.name }}</span>
                <q-chip
                  v-if="entryMapping(entry)"
                  dense
                  class="drive-inline-chip"
                  :class="`status-pill--${statusTone(runtimeStatus(entryMapping(entry)!.id).status)}`"
                >
                  {{ runtimeStatus(entryMapping(entry)!.id).status }}
                </q-chip>
              </div>
              <div v-if="entryMapping(entry)" class="drive-row__meta">
                {{ runtimeStatus(entryMapping(entry)!.id).detail }} · {{ entryMapping(entry)?.localPath }}
              </div>
            </div>
          </div>

          <div class="drive-row__date">
            {{ formatEntryTime(entry.modifiedAt) }}
          </div>

          <div class="drive-row__size">
            {{ formatEntrySize(entry) }}
          </div>

          <div class="drive-row__actions" @click.stop>
            <q-btn
              flat
              round
              dense
              class="subtle-action"
              icon="sym_r_more_vert"
            >
              <q-menu
                anchor="bottom right"
                self="top right"
                class="app-select-menu"
              >
                <q-list dense style="min-width: 190px">
                  <q-item
                    v-if="!entry.isDir && !entryMapping(entry)"
                    v-close-popup
                    clickable
                    @click="emit('mapEntry', entry)"
                  >
                    <q-item-section avatar>
                      <q-icon name="sym_r_link" />
                    </q-item-section>
                    <q-item-section>
                      加入映射
                    </q-item-section>
                  </q-item>
                  <q-item
                    v-if="!entry.isDir && entryMapping(entry)"
                    v-close-popup
                    clickable
                    @click="emit('renameMapping', entryMapping(entry)!)"
                  >
                    <q-item-section avatar>
                      <q-icon name="sym_r_edit" />
                    </q-item-section>
                    <q-item-section>
                      修改映射名称
                    </q-item-section>
                  </q-item>
                  <q-item
                    v-if="!entry.isDir && entryMapping(entry)"
                    v-close-popup
                    clickable
                    @click="emit('chooseFile', entryMapping(entry)!)"
                  >
                    <q-item-section avatar>
                      <q-icon name="sym_r_folder_open" />
                    </q-item-section>
                    <q-item-section>
                      更换本地文件
                    </q-item-section>
                  </q-item>
                  <q-item
                    v-if="!entry.isDir && entryMapping(entry)"
                    v-close-popup
                    clickable
                    @click="handleRemoveMapping(entryMapping(entry)!)"
                  >
                    <q-item-section avatar>
                      <q-icon name="sym_r_link_off" color="negative" />
                    </q-item-section>
                    <q-item-section>
                      解绑映射
                    </q-item-section>
                  </q-item>
                  <q-item
                    v-if="!entry.isDir"
                    v-close-popup
                    clickable
                    @click="emit('downloadFile', entry)"
                  >
                    <q-item-section avatar>
                      <q-icon name="sym_r_download" />
                    </q-item-section>
                    <q-item-section>
                      下载
                    </q-item-section>
                  </q-item>
                  <q-item
                    v-close-popup
                    clickable
                    @click="emit('renameEntry', entry)"
                  >
                    <q-item-section avatar>
                      <q-icon name="sym_r_drive_file_rename_outline" />
                    </q-item-section>
                    <q-item-section>
                      重命名
                    </q-item-section>
                  </q-item>
                  <q-item
                    v-close-popup
                    clickable
                    @click="emit('deleteEntry', entry)"
                  >
                    <q-item-section avatar>
                      <q-icon name="sym_r_delete" color="negative" />
                    </q-item-section>
                    <q-item-section>
                      删除
                    </q-item-section>
                  </q-item>
                </q-list>
              </q-menu>
            </q-btn>
          </div>
        </div>
      </div>
    </q-card>
  </div>
</template>
