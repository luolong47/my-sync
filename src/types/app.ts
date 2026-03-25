export type ConflictStrategy = "manual" | "local" | "remote";

export type TabKey = "dashboard" | "files" | "logs" | "conflicts" | "settings" | "about";

export type FileMapping = {
  id: string;
  name: string;
  localPath: string;
  remotePath: string;
};

export type WebDavSettings = {
  baseUrl: string;
  username: string;
  password: string;
  remoteDir: string;
  clientId: string;
  syncIntervalSecs: number;
  autoSync: boolean;
};

export type SyncSettings = {
  defaultConflictStrategy: ConflictStrategy;
  fsWatchEnabled: boolean;
  debounceDelaySecs: number;
  launchOnBoot: boolean;
};

export type AppConfig = {
  webdav: WebDavSettings;
  sync: SyncSettings;
  mappings: FileMapping[];
};

export type MappingRuntime = {
  mappingId: string;
  status: string;
  detail: string;
  lastSyncAt: string | null;
  lastSyncedHash: string | null;
  lastConflictPath: string | null;
};

export type RuntimeSnapshot = {
  isSyncing: boolean;
  isReady: boolean;
  readinessDetail: string;
  conflictCount: number;
  errorCount: number;
  lastRunAt: string | null;
  lastSummary: string;
  mappings: MappingRuntime[];
};

export type SyncLogEntry = {
  id: string;
  timestamp: string;
  level: string;
  action: string;
  summary: string;
  detail: string;
  httpStatus: number | null;
  mappingId: string | null;
  mappingName: string | null;
  localPath: string | null;
  remotePath: string | null;
  targetPath: string | null;
};

export type RemoteBrowserEntry = {
  name: string;
  path: string;
  isDir: boolean;
  size: number | null;
  modifiedAt: string | null;
};

export type AppSnapshot = {
  config: AppConfig;
  runtime: RuntimeSnapshot;
};

export type NavItem = {
  key: TabKey;
  label: string;
  icon: string;
};
