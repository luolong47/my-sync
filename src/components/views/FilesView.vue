<script setup lang="ts">
import type { AppConfig, RemoteBrowserEntry } from "../../types/app";

defineProps<{
  config: AppConfig;
  isLoadingRemote: boolean;
  remoteEntries: RemoteBrowserEntry[];
  remotePath: string;
  remotePathLabel: string;
}>();

defineEmits<{
  browse: [path: string];
  deleteEntry: [entry: RemoteBrowserEntry];
  downloadFile: [entry: RemoteBrowserEntry];
  renameEntry: [entry: RemoteBrowserEntry];
  uploadFile: [];
}>();
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
          icon="sym_r_upload_file"
          label="上传文件"
          @click="$emit('uploadFile')"
        />
        <q-btn
          flat
          class="subtle-action"
          icon="sym_r_arrow_upward"
          label="返回上级"
          :disable="!remotePath"
          @click="$emit('browse', remotePath.split('/').slice(0, -1).join('/'))"
        />
        <q-btn
          flat
          class="subtle-action"
          icon="sym_r_refresh"
          label="刷新"
          @click="$emit('browse', remotePath)"
        />
      </div>
    </div>

    <div class="content-grid">
      <q-card flat class="panel-card theme-surface">
        <div class="panel-title">
          远端根路径
        </div>
        <div class="q-mt-md remote-root-box">
          <div><strong>根目录</strong> {{ config.webdav.remoteDir }}</div>
          <div><strong>客户端 ID</strong> {{ config.webdav.clientId }}</div>
          <div><strong>当前路径</strong> {{ remotePathLabel || "未配置" }}</div>
        </div>
      </q-card>

      <q-card flat class="panel-card theme-surface">
        <div class="panel-title">
          映射提示
        </div>
        <div class="q-mt-md remote-root-box">
          <div>当前文件菜单是 WebDAV 远端浏览视图。</div>
          <div>文件映射编辑入口已保留在“设置”页。</div>
          <div>支持远端下载、删除、重命名和目录导航。</div>
        </div>
      </q-card>
    </div>

    <q-card flat class="panel-card theme-surface">
      <div v-if="isLoadingRemote" class="empty-block">
        正在读取远端目录...
      </div>
      <div v-else-if="remoteEntries.length === 0" class="empty-block">
        当前目录为空
      </div>
      <q-list v-else separator>
        <q-item
          v-for="entry in remoteEntries"
          :key="entry.path"
          clickable
          @click="entry.isDir ? $emit('browse', entry.path) : undefined"
        >
          <q-item-section avatar>
            <q-icon
              :name="entry.isDir ? 'sym_r_folder' : 'sym_r_description'"
              :color="entry.isDir ? 'primary' : 'secondary'"
            />
          </q-item-section>
          <q-item-section>
            <q-item-label>{{ entry.name }}</q-item-label>
            <q-item-label caption>
              {{ entry.path }}
            </q-item-label>
          </q-item-section>
          <q-item-section side top>
            <div class="remote-entry-meta">
              <span>{{ entry.isDir ? "目录" : `${entry.size ?? 0} B` }}</span>
              <span v-if="entry.modifiedAt">{{ entry.modifiedAt }}</span>
              <q-btn
                v-if="!entry.isDir"
                flat
                dense
                size="sm"
                class="subtle-action compact-action"
                icon="sym_r_download"
                label="下载"
                @click.stop="$emit('downloadFile', entry)"
              />
              <q-btn
                flat
                dense
                size="sm"
                class="subtle-action compact-action"
                icon="sym_r_drive_file_rename_outline"
                label="重命名"
                @click.stop="$emit('renameEntry', entry)"
              />
              <q-btn
                flat
                dense
                size="sm"
                class="danger-action compact-action"
                icon="sym_r_delete"
                label="删除"
                @click.stop="$emit('deleteEntry', entry)"
              />
            </div>
          </q-item-section>
        </q-item>
      </q-list>
    </q-card>
  </div>
</template>
