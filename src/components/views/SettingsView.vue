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
}>();

defineEmits<{
  addMapping: [];
  chooseFile: [mapping: AppConfig["mappings"][number]];
  clearBinding: [id: string];
  configUpdated: [];
  removeMapping: [id: string];
}>();

const boundMappings = computed(() =>
  config.value.mappings.filter((item) => item.bindingStatus === "bound"),
);

const pendingMappings = computed(() =>
  config.value.mappings.filter((item) => item.bindingStatus !== "bound"),
);

function bindingSummary(item: AppConfig["mappings"][number]) {
  if (item.bindingStatus === "bound" && item.localPath.trim()) {
    return item.localPath;
  }

  if (item.pathTemplate.trim()) {
    return `候选模板：${item.pathTemplate}`;
  }

  return "当前设备尚未绑定本地路径";
}
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
            单文件大小限制为 1MB。程序启动时会自动同步一次。开启文件监听后，会在检测到变更并静默 {{ config.sync.debounceDelaySecs || 15 }} 秒后批量触发同步；你也可以通过“立即同步”或新增同步项主动触发。
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
            共享同步项
          </div>
          <div class="panel-subtitle">
            定义跨设备共享的逻辑配置项，拖拽本地文件可快速新增
          </div>
        </div>
        <div class="row q-gutter-sm mapping-toolbar">
          <q-btn
            color="primary"
            unelevated
            icon="sym_r_add"
            label="新增同步项"
            @click="$emit('addMapping')"
          />
        </div>
      </div>
      <div v-if="config.mappings.length === 0" class="empty-block">
        拖拽文件到这里，或点击“新增同步项”
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
              placeholder="同步项名称"
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
              v-model="item.remotePath"
              dense
              outlined
              label="远端逻辑路径"
              class="compact-field"
              @update:model-value="$emit('configUpdated')"
            />
            <q-input
              :model-value="fullRemotePath(item.remotePath)"
              dense
              outlined
              readonly
              label="远端完整路径"
              class="compact-field"
            />
            <q-input
              v-model="item.pathTemplate"
              dense
              outlined
              label="路径模板"
              class="compact-field"
              @update:model-value="$emit('configUpdated')"
            />
          </div>
        </q-card>
      </div>
    </q-card>

    <div class="settings-grid q-mt-lg">
      <q-card flat class="panel-card theme-surface">
        <div class="panel-head">
          <div>
            <div class="panel-title">
              当前设备已绑定
            </div>
            <div class="panel-subtitle">
              这些同步项已经确认了本机落点，会参与真实同步
            </div>
          </div>
        </div>
        <div v-if="boundMappings.length === 0" class="empty-block">
          当前设备还没有已确认的本地绑定
        </div>
        <div v-else class="mapping-list">
          <q-card
            v-for="item in boundMappings"
            :key="item.id"
            flat
            class="mapping-card theme-surface"
          >
            <div class="mapping-head">
              <div>
                <div class="panel-title">
                  {{ item.name || item.remotePath }}
                </div>
                <div class="panel-subtitle">
                  {{ bindingSummary(item) }}
                </div>
              </div>
              <div class="row items-center q-gutter-sm">
                <q-chip dense class="status-pill status-pill--positive">
                  已绑定
                </q-chip>
                <q-btn
                  flat
                  dense
                  icon="sym_r_folder_open"
                  label="更换路径"
                  @click="$emit('chooseFile', item)"
                />
                <q-btn
                  flat
                  dense
                  color="negative"
                  icon="sym_r_link_off"
                  label="清除绑定"
                  @click="$emit('clearBinding', item.id)"
                />
              </div>
            </div>
            <div class="mapping-grid">
              <q-input
                :model-value="item.localPath"
                dense
                outlined
                readonly
                label="当前设备本地路径"
                class="compact-field"
              />
              <q-input
                :model-value="fullRemotePath(item.remotePath)"
                dense
                outlined
                readonly
                label="共享远端路径"
                class="compact-field"
              />
            </div>
          </q-card>
        </div>
      </q-card>

      <q-card flat class="panel-card theme-surface">
        <div class="panel-head">
          <div>
            <div class="panel-title">
              待绑定同步项
            </div>
            <div class="panel-subtitle">
              这些同步项已存在于共享配置，但当前设备还未确认本地路径
            </div>
          </div>
        </div>
        <div v-if="pendingMappings.length === 0" class="empty-block">
          当前没有待绑定同步项
        </div>
        <div v-else class="mapping-list">
          <q-card
            v-for="item in pendingMappings"
            :key="item.id"
            flat
            class="mapping-card theme-surface"
          >
            <div class="mapping-head">
              <div>
                <div class="panel-title">
                  {{ item.name || item.remotePath }}
                </div>
                <div class="panel-subtitle">
                  {{ bindingSummary(item) }}
                </div>
              </div>
              <q-chip dense class="status-pill status-pill--warning">
                待绑定
              </q-chip>
            </div>
            <div class="mapping-grid">
              <q-input
                :model-value="fullRemotePath(item.remotePath)"
                dense
                outlined
                readonly
                label="共享远端路径"
                class="compact-field"
              />
              <q-input
                :model-value="item.pathTemplate || '未提供模板'"
                dense
                outlined
                readonly
                label="候选模板"
                class="compact-field"
              />
            </div>
            <div class="row q-gutter-sm q-mt-md">
              <q-btn
                color="primary"
                unelevated
                icon="sym_r_folder_open"
                label="绑定本地文件"
                @click="$emit('chooseFile', item)"
              />
            </div>
          </q-card>
        </div>
      </q-card>
    </div>
  </div>
</template>
