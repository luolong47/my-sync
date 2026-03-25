import { Notify } from "quasar";
import type {
  AppConfig,
  FileMapping,
  MappingRuntime,
  RemoteBrowserEntry,
  RuntimeSnapshot,
} from "../types/app";

export function createDefaultConfig(): AppConfig {
  return {
    webdav: {
      baseUrl: "",
      username: "",
      password: "",
      remoteDir: "my-sync",
      clientId: "",
      syncIntervalSecs: 30,
      autoSync: true,
    },
    sync: {
      defaultConflictStrategy: "manual",
      fsWatchEnabled: true,
      debounceDelaySecs: 15,
      launchOnBoot: false,
    },
    mappings: [],
  };
}

export function createDefaultRuntime(): RuntimeSnapshot {
  return {
    isSyncing: false,
    isReady: false,
    readinessDetail: "尚未初始化",
    conflictCount: 0,
    errorCount: 0,
    lastRunAt: null,
    lastSummary: "尚未同步",
    mappings: [],
  };
}

export function createEmptyRuntimeStatus(id: string): MappingRuntime {
  return {
    mappingId: id,
    status: "idle",
    detail: "尚未同步",
    lastSyncAt: null,
    lastSyncedHash: null,
    lastConflictPath: null,
  };
}

export function notify(type: "positive" | "negative" | "warning", message: string) {
  const iconByType = {
    positive: "sym_r_check_circle",
    negative: "sym_r_error",
    warning: "sym_r_warning",
  };

  Notify.create({
    type,
    message,
    icon: iconByType[type],
    position: "bottom-right",
    classes: "app-notify",
  });
}

export function formatDateTime(value: string | null) {
  if (!value) {
    return "尚未执行";
  }

  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(parsed);
}

export function inferRemotePathFromLocal(localPath: string) {
  const normalized = localPath.replace(/\\/g, "/");
  const segments = normalized.split("/").filter(Boolean);
  if (segments.length >= 3) {
    const usersIndex = segments.findIndex((item) => item.toLowerCase() === "users");
    if (usersIndex >= 0 && segments.length > usersIndex + 2) {
      return segments.slice(usersIndex + 2).join("/");
    }
  }

  return segments.slice(-2).join("/") || segments[segments.length - 1] || "config";
}

export function cloneConfig(source: AppConfig): AppConfig {
  return {
    webdav: { ...source.webdav },
    sync: { ...source.sync },
    mappings: source.mappings.map((item) => ({ ...item })),
  };
}

export function createMapping(): FileMapping {
  return {
    id: crypto.randomUUID(),
    name: "",
    localPath: "",
    remotePath: "",
  };
}

export function createMappingFromPath(localPath: string): FileMapping {
  const normalized = localPath.replace(/\\/g, "/");
  const filename = normalized.split("/").pop() || "config";
  return {
    id: crypto.randomUUID(),
    name: filename,
    localPath,
    remotePath: inferRemotePathFromLocal(localPath),
  };
}

export function fullRemotePath(rootPath: string, path: string) {
  const relativePath = path.trim().replace(/^\/+/, "");
  if (!relativePath) {
    return rootPath || "未生成";
  }

  return rootPath ? `${rootPath}/${relativePath}` : relativePath;
}

export function statusTone(status: string) {
  switch (status) {
    case "synced":
    case "resolved":
      return "positive";
    case "pushed":
      return "primary";
    case "pulled":
      return "neutral";
    case "conflict":
      return "warning";
    case "error":
      return "danger";
    default:
      return "neutral";
  }
}

export function buildUploadTargetPath(remotePath: string, selected: string) {
  const filename = selected.replace(/\\/g, "/").split("/").pop() || "file";
  return remotePath ? `${remotePath.replace(/\/$/, "")}/${filename}` : filename;
}

export function buildRenameTargetPath(entry: RemoteBrowserEntry, newName: string) {
  const parentPrefix = entry.path.includes("/") ? entry.path.split("/").slice(0, -1).join("/") : "";
  return parentPrefix ? `${parentPrefix}/${newName}` : newName;
}
