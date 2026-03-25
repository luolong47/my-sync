<script setup lang="ts">
import type { AppConfig, MappingRuntime } from "../../types/app";

const config = defineModel<AppConfig>("config", {
  required: true,
});

defineProps<{
  fullRemotePath: (path: string) => string;
  isAutoSaving: boolean;
  isMappingDropActive: boolean;
  remoteRootPath: string;
  runtimeStatus: (id: string) => MappingRuntime;
  statusTone: (status: string) => string;
}>();

defineEmits<{
  addMapping: [];
  chooseFile: [mapping: AppConfig["mappings"][number]];
  configUpdated: [];
  removeMapping: [id: string];
}>();
</script>

<template>
  <div class="view-shell">
    <div class="view-header">
      <div>
        <div class="eyebrow">
          Settings
        </div>
        <h1>设置</h1>
      </div>
      <q-chip dense class="status-pill" :class="`status-pill--${isAutoSaving ? 'primary' : 'neutral'}`">
        {{ isAutoSaving ? "自动保存中" : "已自动保存" }}
      </q-chip>
    </div>

    <div class="settings-grid">
      <q-card flat class="panel-card theme-surface">
        <div class="panel-title">
          WebDAV 设置
        </div>
        <div class="form-grid q-mt-md">
          <q-input
            v-model="config.webdav.baseUrl"
            outlined
            label="WebDAV 地址"
            placeholder="https://..."
            @update:model-value="$emit('configUpdated')"
          />
          <q-input
            v-model="config.webdav.remoteDir"
            outlined
            label="远端同步根目录"
            readonly
          />
          <q-input
            v-model="config.webdav.clientId"
            outlined
            label="客户端 ID"
            readonly
          />
          <q-input
            v-model="config.webdav.username"
            outlined
            label="用户名"
            @update:model-value="$emit('configUpdated')"
          />
          <q-input
            v-model="config.webdav.password"
            outlined
            type="password"
            label="密码 / Token"
            @update:model-value="$emit('configUpdated')"
          />
          <q-input
            v-model.number="config.webdav.syncIntervalSecs"
            outlined
            type="number"
            min="10"
            label="轮询间隔（秒）"
            @update:model-value="$emit('configUpdated')"
          />
          <div class="toggle-box">
            <q-toggle
              v-model="config.webdav.autoSync"
              label="启用后台自动同步"
              color="primary"
              @update:model-value="$emit('configUpdated')"
            />
          </div>
          <q-banner rounded class="info-banner full-span">
            远端实际同步根路径为：{{ remoteRootPath || "未生成" }}
          </q-banner>
        </div>
      </q-card>

      <q-card flat class="panel-card theme-surface">
        <div class="panel-title">
          同步策略
        </div>
        <div class="q-mt-md q-gutter-y-md">
          <q-select
            v-model="config.sync.defaultConflictStrategy"
            outlined
            emit-value
            map-options
            label="默认冲突解决方案"
            :options="[
              { label: '手动处理', value: 'manual' },
              { label: '以本地为准', value: 'local' },
              { label: '以远端为准', value: 'remote' },
            ]"
            @update:model-value="$emit('configUpdated')"
          />
          <div class="row items-center justify-between q-gutter-md">
            <div class="col">
              <div class="text-weight-medium">
                开机自启动
              </div>
              <div class="text-caption caption-soft">
                登录系统后自动启动应用，适合需要长期驻留托盘的场景
              </div>
            </div>
            <q-toggle
              v-model="config.sync.launchOnBoot"
              color="primary"
              @update:model-value="$emit('configUpdated')"
            />
          </div>
          <div class="row items-center justify-between q-gutter-md">
            <div class="col">
              <div class="text-weight-medium">
                启用文件系统监听
              </div>
              <div class="text-caption caption-soft">
                开启后优先使用本地文件变更事件，替代纯轮询触发同步
              </div>
            </div>
            <q-toggle
              v-model="config.sync.fsWatchEnabled"
              color="primary"
              @update:model-value="$emit('configUpdated')"
            />
          </div>
          <q-input
            v-model.number="config.sync.debounceDelaySecs"
            outlined
            type="number"
            min="3"
            max="60"
            label="监听防抖延时（秒）"
            @update:model-value="$emit('configUpdated')"
          />
          <q-banner rounded class="info-banner">
            单文件大小限制为 1MB。当前“系统就绪”会检查 WebDAV 基本配置、本地路径是否存在，以及是否有待处理冲突。开启文件监听后，会在检测到变更并静默 {{ config.sync.debounceDelaySecs || 15 }} 秒后批量触发同步。
          </q-banner>
        </div>
      </q-card>
    </div>

    <q-card
      flat
      class="panel-card q-mt-lg theme-surface mapping-drop-zone"
      :class="{ 'mapping-drop-zone--active': isMappingDropActive }"
    >
      <div class="panel-head">
        <div>
          <div class="panel-title">
            文件映射
          </div>
          <div class="panel-subtitle">
            支持拖拽本地文件到此区域快速添加映射
          </div>
        </div>
        <div class="row q-gutter-sm mapping-toolbar">
          <q-btn
            color="primary"
            unelevated
            icon="sym_r_add"
            label="新增映射"
            @click="$emit('addMapping')"
          />
        </div>
      </div>
      <div v-if="config.mappings.length === 0" class="empty-block">
        拖拽文件到这里，或点击“新增映射”
      </div>
      <div v-else class="mapping-list">
        <q-card
          v-for="item in config.mappings"
          :key="item.id"
          flat
          class="mapping-card theme-surface"
        >
          <div class="mapping-head">
            <q-input
              v-model="item.name"
              dense
              borderless
              placeholder="映射名称"
              class="mapping-title"
              @update:model-value="$emit('configUpdated')"
            />
            <div class="row items-center q-gutter-sm">
              <q-chip dense class="status-pill" :class="`status-pill--${statusTone(runtimeStatus(item.id).status)}`">
                {{ runtimeStatus(item.id).status }}
              </q-chip>
              <q-btn
                round
                flat
                dense
                class="danger-action"
                icon="sym_r_delete"
                @click="$emit('removeMapping', item.id)"
              />
            </div>
          </div>
          <div class="mapping-grid">
            <q-input
              v-model="item.localPath"
              dense
              outlined
              label="本地文件"
              class="compact-field"
              @update:model-value="$emit('configUpdated')"
            >
              <template #append>
                <q-btn
                  flat
                  dense
                  icon="sym_r_folder_open"
                  @click="$emit('chooseFile', item)"
                />
              </template>
            </q-input>
            <q-input
              :model-value="fullRemotePath(item.remotePath)"
              dense
              outlined
              readonly
              label="远端完整路径"
              class="compact-field"
            />
          </div>
        </q-card>
      </div>
    </q-card>
  </div>
</template>
