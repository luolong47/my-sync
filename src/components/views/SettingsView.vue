<script setup lang="ts">
import { computed } from "vue";
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
  isDarkMode: boolean;
}>();

defineEmits<{
  addMapping: [];
  chooseFile: [mapping?: AppConfig["mappings"][number]];
  clearBinding: [id: string];
  configUpdated: [];
  removeMapping: [id: string];
  toggleTheme: [];
}>();

const sortedMappings = computed(() => {
  return [...config.value.mappings].sort((a, b) => {
    const aBound = a.bindingStatus === "bound" ? 0 : 1;
    const bBound = b.bindingStatus === "bound" ? 0 : 1;
    return aBound - bBound;
  });
});
</script>

<template>
  <div class="view-shell">
    <div class="view-header">
      <div class="row no-wrap items-center full-width">
        <div class="col">
          <div class="eyebrow">
            系统设置
          </div>
        </div>
        <div class="row items-center q-gutter-x-sm">
          <q-chip dense class="status-pill" :class="`status-pill--${isAutoSaving ? 'primary' : 'neutral'}`">
            {{ isAutoSaving ? "自动保存中" : "已自动保存" }}
          </q-chip>
        </div>
      </div>
    </div>

    <!-- 基础配置网格 -->
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
            @update:model-value="$emit('configUpdated')"
          />
          <q-input
            v-model="config.webdav.spaceId"
            outlined
            label="同步空间"
            @update:model-value="$emit('configUpdated')"
          />
          <q-input
            v-model="config.webdav.deviceId"
            outlined
            label="设备 ID"
            readonly
          />
          <q-input
            v-model="config.webdav.deviceName"
            outlined
            label="设备名称"
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
          <div class="row items-center justify-between no-wrap">
            <div class="text-weight-medium">
              开机自启动
            </div>
            <q-toggle v-model="config.sync.launchOnBoot" color="primary" @update:model-value="$emit('configUpdated')" />
          </div>
          <div class="row items-center justify-between no-wrap">
            <div class="text-weight-medium">
              启用文件系统监听
            </div>
            <q-toggle v-model="config.sync.fsWatchEnabled" color="primary" @update:model-value="$emit('configUpdated')" />
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
        </div>
      </q-card>
    </div>

    <!-- 核心同步项映射管理 -->
    <q-card flat class="panel-card q-mt-lg theme-surface mapping-drop-zone" :class="{ 'mapping-drop-zone--active': isMappingDropActive }">
      <div class="panel-head q-mb-md">
        <div class="panel-title">
          同步映射管理
        </div>
        <q-btn
          color="primary"
          unelevated
          round
          icon="sym_r_add"
          @click="$emit('chooseFile')"
        >
          <q-tooltip>新增同步项</q-tooltip>
        </q-btn>
      </div>

      <div v-if="config.mappings.length === 0" class="empty-block q-my-xl">
        <q-icon name="sym_r_cloud_off" size="48px" class="q-mb-md opacity-40" />
        <div>目前还没有任何同步记录，点击“新增同步项”开始</div>
      </div>

      <div v-else class="mapping-table-container">
        <!-- 表头 -->
        <div class="row no-wrap text-caption text-weight-bold text-faint q-px-lg q-pb-xs text-uppercase" style="letter-spacing: 0.05em;">
          <div class="col-3 q-pr-md">
            名称
          </div>
          <div class="col-7 q-pr-md" style="padding-left: 12px;">
            目录
          </div>
          <div class="col-2 text-right">
            操作
          </div>
        </div>

        <q-card flat class="panel-card theme-surface q-pa-none overflow-hidden">
          <div
            v-for="(item, index) in sortedMappings"
            :key="item.id"
            class="mapping-table-row row no-wrap items-center q-pa-sm"
            :class="{ 'border-top': index > 0, 'bg-unbound': item.bindingStatus !== 'bound' }"
          >
            <!-- 1. 名称栏 -->
            <div class="col-3 q-pr-md row no-wrap items-center">
              <div class="status-dot q-mr-sm" :class="`status-dot--${statusTone(runtimeStatus(item.id).status)}`">
                <q-tooltip anchor="center right" self="center left" :offset="[10, 0]">
                  {{ runtimeStatus(item.id).status || 'idle' }}
                </q-tooltip>
              </div>
              <div class="col overflow-hidden">
                <q-input
                  v-model="item.name"
                  dense
                  borderless
                  placeholder="未命名"
                  class="full-width text-weight-bold field-no-padding mapping-table-name-input"
                  @update:model-value="$emit('configUpdated')"
                />
              </div>
            </div>

            <!-- 2. 目录栏 -->
            <div class="col-7 q-pr-md">
              <q-input
                v-model="item.remotePath"
                dense
                outlined
                placeholder="远端路径"
                class="full-width dense-cell-input"
                @update:model-value="$emit('configUpdated')"
              >
                <q-tooltip
                  class="bg-dark text-white text-body2 shadow-4"
                  anchor="top middle"
                  self="bottom middle"
                  :offset="[0, 4]"
                >
                  <div v-if="item.bindingStatus === 'bound'" style="font-family: inherit;">
                    {{ item.localPath }}
                  </div>
                  <div v-else class="text-warning">
                    尚未绑定落点
                  </div>
                </q-tooltip>
              </q-input>
            </div>

            <!-- 3. 操作栏 -->
            <div class="col-2 row items-center justify-end no-wrap">
              <div class="row no-wrap q-gutter-x-xs">
                <q-btn
                  v-if="item.bindingStatus !== 'bound'"
                  flat
                  round
                  dense
                  color="primary"
                  size="sm"
                  icon="sym_r_add_link"
                  @click="$emit('chooseFile', item)"
                >
                  <q-tooltip>绑定</q-tooltip>
                </q-btn>
                <q-btn
                  v-if="item.bindingStatus === 'bound'"
                  flat
                  round
                  dense
                  color="primary"
                  size="sm"
                  icon="sym_r_edit_square"
                  @click="$emit('chooseFile', item)"
                >
                  <q-tooltip>更换</q-tooltip>
                </q-btn>
                <q-btn
                  v-if="item.bindingStatus === 'bound'"
                  flat
                  round
                  dense
                  color="negative"
                  size="sm"
                  icon="sym_r_link_off"
                  @click="$emit('clearBinding', item.id)"
                >
                  <q-tooltip>解绑</q-tooltip>
                </q-btn>
                <q-btn
                  flat
                  round
                  dense
                  class="text-red-5"
                  size="sm"
                  icon="sym_r_delete_forever"
                  @click="$emit('removeMapping', item.id)"
                >
                  <q-tooltip>删除</q-tooltip>
                </q-btn>
              </div>
            </div>
          </div>
        </q-card>
      </div>
    </q-card>
  </div>
</template>
