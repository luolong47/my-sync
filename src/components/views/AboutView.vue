<template>
  <div class="view-shell">
    <div class="view-header">
      <div>
        <div class="eyebrow">
          关于
        </div>
      </div>
    </div>
    <q-card flat class="panel-card about-card theme-surface">
      <q-icon name="sym_r_sync" size="72px" color="primary" />
      <h2>My Sync</h2>
      <p>用于将零散配置文件映射到 WebDAV 的桌面同步工具。</p>
      <p>当前已支持日志查看、冲突处理、后台轮询同步、文件监听与便携版构建。</p>
      <p class="version-text">版本 v{{ version }}</p>
    </q-card>
  </div>
</template>

<script setup lang="ts">
import pkg from '../../../package.json'

const version = pkg.version
</script>

<style scoped>
.version-text {
  margin-top: 1rem;
  font-size: 0.85rem;
  color: var(--q-secondary);
}
</style>
