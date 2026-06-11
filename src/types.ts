export type Theme = "system" | "light" | "dark";

export type GameChannel = "mindustry" | "mindustryX" | "mindustryBE" | "mindustryXBE";

export type ChannelVisibility = {
  mindustry: boolean;
  mindustryX: boolean;
  mindustryBe: boolean;
  mindustryXbe: boolean;
};

export type Settings = {
  installRoot: string;
  showBe: boolean;
  githubProxyPrefix?: string | null;
  httpProxy?: string | null;
  selectedAcceleratorId?: string | null;
  channelVisibility: ChannelVisibility;
  runtimePromptDismissed: boolean;
  debugMode: boolean;
};

export type AcceleratorSupports = {
  api: boolean;
  raw: boolean;
  releaseAsset: boolean;
};

export type AcceleratorRule = {
  from: string;
  to: string;
};

export type Accelerator = {
  id: string;
  name: string;
  baseUrl: string;
  rules: AcceleratorRule[];
  supports: AcceleratorSupports;
  healthCheckUrl?: string | null;
  enabledByDefault: boolean;
};

export type AcceleratorList = {
  version: number;
  updatedAt: string;
  sources: Accelerator[];
};

export type ReleaseAsset = {
  name: string;
  size: number;
  downloadUrl: string;
  digest?: string | null;
};

export type RemoteVersion = {
  id: string;
  channel: GameChannel;
  channelLabel: string;
  version: string;
  tag: string;
  name: string;
  prerelease: boolean;
  publishedAt?: string | null;
  assets: ReleaseAsset[];
  selectedAsset?: ReleaseAsset | null;
  installed: boolean;
};

export type InstalledInstance = {
  id: string;
  channel: GameChannel;
  version: string;
  installDir: string;
  dataDir: string;
  jarPath: string;
  runtimeId?: string | null;
  installedAt: string;
  launchSettings: LaunchSettings;
};

export type LaunchSettings = {
  minMemoryMb?: number | null;
  maxMemoryMb?: number | null;
  extraJvmArgs: string;
  gameArgs: string;
};

export type RuntimeSource = "launcher" | "imported" | "scanned" | "system" | "unknown";

export type RuntimeInfo = {
  id: string;
  javaVersion: number;
  version?: string | null;
  os: string;
  arch: string;
  path: string;
  javaPath: string;
  installed: boolean;
  enabled: boolean;
  source: RuntimeSource;
};

export type RemoteRuntime = {
  id: string;
  javaVersion: number;
  version: string;
  os: string;
  arch: string;
  fileName: string;
  sizeLabel: string;
  sizeBytes?: number | null;
  updatedAt: string;
  downloadUrl: string;
  checksum?: string | null;
};

export type LaunchResult = {
  pid: number;
  logPath: string;
};

export type MigrationResult = {
  oldRoot: string;
  newRoot: string;
  copied: boolean;
};

export type DebugLogSnapshot = {
  enabled: boolean;
  logPath: string;
  sessionId?: string | null;
  startedAt?: string | null;
  lineCount: number;
  maxLines: number;
  truncated: boolean;
  content: string;
};

export type AppUiState = {
  settings: Settings;
  accelerators: AcceleratorList;
  versions: RemoteVersion[];
  instances: InstalledInstance[];
  runtimes: RuntimeInfo[];
};

export type TaskEvent =
  | {
      event: "started";
      data: {
        taskId: string;
        label: string;
        totalBytes?: number | null;
        message?: string | null;
      };
    }
  | {
      event: "progress";
      data: {
        taskId: string;
        downloadedBytes: number;
        totalBytes?: number | null;
        bytesPerSecond?: number | null;
        message?: string | null;
      };
    }
  | {
      event: "paused";
      data: {
        taskId: string;
        downloadedBytes: number;
        totalBytes?: number | null;
        message: string;
      };
    }
  | {
      event: "finished";
      data: {
        taskId: string;
        message: string;
      };
    }
  | {
      event: "canceled";
      data: {
        taskId: string;
        message: string;
      };
    }
  | {
      event: "failed";
      data: {
        taskId: string;
        message: string;
      };
    };

export type TaskRecord = {
  id: string;
  label: string;
  downloadedBytes: number;
  totalBytes?: number | null;
  bytesPerSecond?: number | null;
  status: "running" | "paused" | "finished" | "failed" | "canceled";
  message?: string;
};
