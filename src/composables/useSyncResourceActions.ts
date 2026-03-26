import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Ref } from "vue";
import type { RemoteBrowserEntry, RuntimeSnapshot, SyncLogEntry } from "../types/app";
import { buildRenameTargetPath, buildUploadTargetPath, notify } from "../utils/appState";

type RefreshRemoteFiles = (path?: string) => Promise<void>;
type RefreshLogs = () => Promise<void>;

type ResourceActionContext = {
  loadLogs: RefreshLogs;
  loadRemoteFiles: RefreshRemoteFiles;
  logs: Ref<SyncLogEntry[]>;
  remoteEntries: Ref<RemoteBrowserEntry[]>;
  remotePath: Ref<string>;
  runtime: Ref<RuntimeSnapshot>;
  validateLocalFilePath: (localPath: string) => Promise<void>;
};

// eslint-disable-next-line max-lines-per-function
export function useSyncResourceActions(context: ResourceActionContext) {
  async function clearLogs() {
    try {
      await invoke("clear_sync_logs");
      context.logs.value = [];
      notify("positive", "日志已清空");
    } catch (error) {
      notify("negative", `清空日志失败：${String(error)}`);
    }
  }

  async function exportLogs() {
    try {
      const target = await save({
        title: "导出同步日志",
        defaultPath: "my-sync-logs.json",
      });
      if (!target) {
        return;
      }

      await invoke("export_sync_logs", { path: target });
      notify("positive", "日志已导出");
    } catch (error) {
      notify("negative", `导出日志失败：${String(error)}`);
    }
  }

  async function resolveConflict(mappingId: string, strategy: "local" | "remote") {
    try {
      context.runtime.value = await invoke<RuntimeSnapshot>("resolve_conflict", {
        mappingId,
        strategy,
      });
      await Promise.all([context.loadLogs(), context.loadRemoteFiles(context.remotePath.value)]);
      notify("positive", "冲突已处理");
    } catch (error) {
      notify("negative", `处理冲突失败：${String(error)}`);
    }
  }

  async function downloadRemoteFile(entry: RemoteBrowserEntry) {
    try {
      const target = await save({
        title: `下载 ${entry.name}`,
        defaultPath: entry.name,
      });
      if (!target) {
        return;
      }

      await invoke("download_remote_file", {
        remotePath: entry.path,
        savePath: target,
      });
      notify("positive", "远端文件已下载");
    } catch (error) {
      notify("negative", `下载远端文件失败：${String(error)}`);
    }
  }

  async function uploadLocalFile() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: "选择要上传到当前远端目录的文件",
      });
      if (typeof selected !== "string") {
        return;
      }

      await context.validateLocalFilePath(selected);
      const targetPath = buildUploadTargetPath(context.remotePath.value, selected);
      let overwrite = false;
      if (context.remoteEntries.value.some((item) => item.path === targetPath)) {
        overwrite = window.confirm(`远端已存在同名文件“${targetPath.split("/").pop()}”，是否覆盖？`);
        if (!overwrite) {
          return;
        }
      }

      await invoke("upload_local_file", {
        localPath: selected,
        remoteDirPath: context.remotePath.value || null,
        overwrite,
      });
      await context.loadRemoteFiles(context.remotePath.value);
      notify("positive", "文件已上传到远端");
    } catch (error) {
      notify("negative", `上传文件失败：${String(error)}`);
    }
  }

  async function deleteRemoteEntry(entry: RemoteBrowserEntry) {
    try {
      const confirmed = window.confirm(
        `确认删除远端${entry.isDir ? "目录" : "文件"}“${entry.name}”？`,
      );
      if (!confirmed) {
        return;
      }

      await invoke("delete_remote_file", {
        remotePath: entry.path,
      });
      await context.loadRemoteFiles(context.remotePath.value);
      notify("positive", "远端项目已删除");
    } catch (error) {
      notify("negative", `删除远端项目失败：${String(error)}`);
    }
  }

  async function renameRemoteEntry(entry: RemoteBrowserEntry) {
    try {
      const newName = window.prompt("输入新的名称", entry.name)?.trim();
      if (!newName || newName === entry.name) {
        return;
      }

      const targetPath = buildRenameTargetPath(entry, newName);
      let overwrite = false;
      if (context.remoteEntries.value.some((item) => item.path === targetPath && item.path !== entry.path)) {
        overwrite = window.confirm(`远端已存在同名项目“${newName}”，是否覆盖？`);
        if (!overwrite) {
          return;
        }
      }

      await invoke("rename_remote_file", {
        remotePath: entry.path,
        newName,
        overwrite,
      });
      await context.loadRemoteFiles(context.remotePath.value);
      notify("positive", "远端项目已重命名");
    } catch (error) {
      notify("negative", `重命名远端项目失败：${String(error)}`);
    }
  }

  async function createRemoteDirectory() {
    try {
      const name = window.prompt("输入新目录名称")?.trim();
      if (!name) {
        return;
      }

      await invoke("create_remote_directory", {
        remoteDirPath: context.remotePath.value || null,
        name,
      });
      await context.loadRemoteFiles(context.remotePath.value);
      notify("positive", "远端目录已创建");
    } catch (error) {
      notify("negative", `创建远端目录失败：${String(error)}`);
    }
  }

  return {
    clearLogs,
    createRemoteDirectory,
    deleteRemoteEntry,
    downloadRemoteFile,
    exportLogs,
    renameRemoteEntry,
    resolveConflict,
    uploadLocalFile,
  };
}
