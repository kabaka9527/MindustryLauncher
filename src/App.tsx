import { Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Ban,
  Bug,
  Cpu,
  CheckCircle2,
  Cloud,
  Download,
  FileUp,
  FolderOpen,
  HardDriveDownload,
  Layers,
  Loader2,
  Monitor,
  Moon,
  Pause,
  Play,
  RefreshCcw,
  Save,
  Search,
  Settings as SettingsIcon,
  SlidersHorizontal,
  Sun,
  Trash2,
  X,
  Zap,
} from "lucide-react";
import type { ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  cancelDownload,
  checkLauncherUpdate,
  clearDebugLog,
  deleteInstance,
  deleteRuntime,
  emitFrontendLog,
  getAppState,
  ignoreLauncherVersion,
  importRuntime,
  installVersion,
  installRuntime,
  launchVersion,
  onDebugLogEntry,
  openDebugLogDir,
  migrateInstallRoot,
  openInstallRoot,
  openUrl,
  pauseDownload,
  pingAccelerator,
  readDebugLog,
  refreshAccelerators,
  refreshVersions,
  startupRefreshVersions,
  startupRefreshRuntimes,
  onVersionsRefreshed,
  onRuntimesRefreshed,
  onGameSessionStarted,
  onGameSessionEnded,
  resumeDownload,
  scanRuntimes,
  saveInstanceLaunchSettings,
  saveSettings,
  setRuntimeEnabled,
  switchVersion,
  upgradeInstance,
} from "./api";
import type {
  Accelerator,
  AppUiState,
  ChannelSprite,
  ChannelVisibility,
  DebugLogEntry,
  DebugLogSnapshot,
  GameChannel,
  InstalledInstance,
  LaunchSettings,
  LauncherUpdateInfo,
  PingResult,
  RemoteRuntime,
  RemoteVersion,
  RuntimeInfo,
  Settings,
  TaskEvent,
  TaskRecord,
  Theme,
} from "./types";
import alphaSprite from "./assets/mindustry/alpha.png";
import coreSprite from "./assets/mindustry/core-shard.png";
import lancerSprite from "./assets/mindustry/lancer.png";
import zenithSprite from "./assets/mindustry/zenith.png";
import { useTheme } from "./hooks/useTheme";
import { useWikiIcons } from "./hooks/useWikiIcons";
import TitleBar from "./TitleBar";

type View = "games" | "versions" | "runtimes" | "settings" | "debug";

type RuntimeConflict = {
  key: string;
  label: string;
  items: RuntimeInfo[];
};

type PendingRuntimeConflict = {
  key: string;
  label: string;
  remote: RemoteRuntime;
  locals: RuntimeInfo[];
};

type DeleteConfirmation =
  | {
      kind: "game";
      instance: InstalledInstance;
    }
  | {
      kind: "runtime";
      runtime: RuntimeInfo;
    };

type ToastMessage = {
  id: number;
  text: string;
};

const channelLabels: Record<GameChannel, string> = {
  mindustry: "Mindustry",
  mindustryX: "MindustryX",
  mindustryBE: "Mindustry BE",
  mindustryXBE: "MindustryX BE",
};

const channelVisibilityKeys: Array<{
  key: keyof ChannelVisibility;
  channel: GameChannel;
  label: string;
}> = [
  { key: "mindustry", channel: "mindustry", label: "Mindustry" },
  { key: "mindustryX", channel: "mindustryX", label: "MindustryX" },
  { key: "mindustryBe", channel: "mindustryBE", label: "Mindustry BE" },
  { key: "mindustryXbe", channel: "mindustryXBE", label: "MindustryX BE" },
];

// 版本列表刷新的超时阈值（毫秒）。超过该时间后端仍未回传“versions-refreshed”
// 事件时，前端强制结束加载状态，避免网络超时或接口无响应导致“正在后台刷新”
// 提示一直不消失。
const VERSION_REFRESH_TIMEOUT_MS = 30_000;

function logFrontend(level: string, message: string) {
  try {
    emitFrontendLog(level, message)
  } catch { /* ignore in web/dev */ }
}

export default function App() {
  const { theme, setTheme, isDark } = useTheme();
  const { floatItems, channelSprites } = useWikiIcons();

  const [view, setView] = useState<View>("games");
  const [state, setState] = useState<AppUiState | null>(null);
  const [draft, setDraft] = useState<Settings | null>(null);
  const [tasks, setTasks] = useState<Record<string, TaskRecord>>({});
  const [busy, setBusy] = useState<string | null>("load");
  const [notice, setNotice] = useState<string>("正在读取本地状态");
  const [startupAcceleratorsRefreshDone, setStartupAcceleratorsRefreshDone] = useState(false);
  const [remoteRuntimes, setRemoteRuntimes] = useState<RemoteRuntime[]>([]);
  const [selectedRuntimeId, setSelectedRuntimeId] = useState("");
  const [versionsRefreshedAt, setVersionsRefreshedAt] = useState(0);
  const [runtimesRefreshedAt, setRuntimesRefreshedAt] = useState(0);
  // 版本列表加载状态：true 表示正在后台刷新，用于驱动刷新按钮的加载动画与提示。
  const [versionsRefreshing, setVersionsRefreshing] = useState(false);
  // 请求令牌：每次刷新自增，只有“最新一次”请求的收尾逻辑可以更新界面，
  // 从而解决并发刷新时的状态覆盖冲突（旧请求晚到不会覆盖新请求的结果）。
  const versionsRefreshTokenRef = useRef(0);
  // 是否在加载中（供一次性事件监听闭包读取最新值）。
  const versionsRefreshingRef = useRef(false);
  // 超时计时器，用于超时后强制收尾。
  const versionsRefreshTimerRef = useRef<number | null>(null);
  const [runtimeGuideOpen, setRuntimeGuideOpen] = useState(false);
  const [editingInstanceId, setEditingInstanceId] = useState<string | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState<DeleteConfirmation | null>(null);
  const [pingResults, setPingResults] = useState<Record<string, PingResult>>({});
  // 正在测速的加速源集合，用于在点按“测试”后展示“正在测试”状态。
  const [pinging, setPinging] = useState<Record<string, boolean>>({});
  const [upgradeTargetId, setUpgradeTargetId] = useState<string | null>(null);
  // 升降级结果反馈：仅用于向用户明确展示“成功/失败”的最终状态与原因。
  const [upgradeFeedback, setUpgradeFeedback] = useState<{
    status: "success" | "error";
    title: string;
    detail: string;
  } | null>(null);
  // 防重复启动：同版本游戏已在运行时弹出的明确警告。
  const [launchConflict, setLaunchConflict] = useState<{
    version: string;
    message: string;
  } | null>(null);
  const [toastMessage, setToastMessage] = useState<ToastMessage | null>(null);
  const toastTimerRef = useRef<number | null>(null);
  const [instanceSettingsDraft, setInstanceSettingsDraft] = useState<{
    runtimeId: string;
    launchSettings: LaunchSettings;
  } | null>(null);
  const [updateInfo, setUpdateInfo] = useState<LauncherUpdateInfo | null>(null);
  const [showUpdatePrompt, setShowUpdatePrompt] = useState(false);

  const reload = useCallback(async () => {
    const next = await getAppState();
    setState(next);
    setDraft(next.settings);
    setNotice("状态已同步");
    return next;
  }, []);

  async function refreshRuntimesInBackground() {
    logFrontend("info", "后台运行时列表刷新已触发");
    const cached = await startupRefreshRuntimes();
    setRemoteRuntimes(cached);
    setSelectedRuntimeId((current) => {
      if (current && cached.some((runtime) => runtime.id === current)) return current;
      return cached.find((runtime) => runtime.javaVersion === 17)?.id ?? cached[0]?.id ?? "";
    });
  }

  useEffect(() => {
    reload()
      .catch((error) => setNotice(toMessage(error)))
      .finally(() => setBusy(null));
  }, [reload]);

  // 成功反馈自动消失；失败反馈保留，等待用户主动关闭以阅读具体原因。
  useEffect(() => {
    if (upgradeFeedback?.status === "success") {
      const timer = window.setTimeout(() => setUpgradeFeedback(null), 5000);
      return () => window.clearTimeout(timer);
    }
  }, [upgradeFeedback]);

  useEffect(() => {
    return () => {
      if (toastTimerRef.current !== null) {
        window.clearTimeout(toastTimerRef.current);
      }
      if (versionsRefreshTimerRef.current !== null) {
        window.clearTimeout(versionsRefreshTimerRef.current);
      }
    };
  }, []);

  const [startupUpdateCheckDone, setStartupUpdateCheckDone] = useState(false);
  useEffect(() => {
    if (!state || startupUpdateCheckDone) {
      return;
    }
    setStartupUpdateCheckDone(true);
    checkLauncherUpdate()
      .then((info) => {
        setUpdateInfo(info);
        if (info.hasUpdate) {
          const ignored = state.settings.ignoredVersions ?? [];
          if (!ignored.includes(info.latestVersion)) {
            setShowUpdatePrompt(true);
          }
        } else if (info.errorMessage) {
          setNotice(info.errorMessage);
        }
      })
      .catch(() => {});
  }, [startupUpdateCheckDone, state]);

  const [startupRuntimesRefreshDone, setStartupRuntimesRefreshDone] = useState(false);
  useEffect(() => {
    if (!state || startupRuntimesRefreshDone) return;
    setStartupRuntimesRefreshDone(true);
    refreshRuntimesInBackground().catch((error) => setNotice(toMessage(error)));
  }, [startupRuntimesRefreshDone, state]);

  useEffect(() => {
    const unlisten = onRuntimesRefreshed((runtimes) => {
      setRemoteRuntimes(runtimes);
      setSelectedRuntimeId((current) => {
        if (current && runtimes.some((r) => r.id === current)) return current;
        return runtimes.find((r) => r.javaVersion === 17)?.id ?? runtimes[0]?.id ?? "";
      });
      setRuntimesRefreshedAt(Date.now());
      logFrontend("info", `远端运行时数据到达：${runtimes.length} 个运行时`);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    if (!state || startupAcceleratorsRefreshDone) {
      return;
    }
    setStartupAcceleratorsRefreshDone(true);
    refreshAccelerators()
      .then((accelerators) => {
        setState((current) => (current ? { ...current, accelerators } : current));
      })
      .catch((error) => setNotice(toMessage(error)));
  }, [startupAcceleratorsRefreshDone, state]);

  useEffect(() => {
    setNotice(
      view === "games"
        ? "已安装游戏列表已就绪"
        : view === "versions"
          ? "选择版本进行切换"
          : view === "runtimes"
            ? "管理本地与远端 Java 运行时"
            : view === "debug"
              ? "调试控制台已打开"
              : "管理安装目录、加速源与运行时环境",
    );
  }, [view]);

  useEffect(() => {
    if (
      state &&
      state.runtimes.length === 0 &&
      !state.settings.runtimePromptDismissed
    ) {
      setRuntimeGuideOpen(true);
    }
  }, [state]);

  // Kick off background version refresh right after initial state loads.
  // The command returns cached versions immediately; fresh data arrives
  // via the "versions-refreshed" event listener below.
  const [startupVersionsRefreshDone, setStartupVersionsRefreshDone] = useState(false);
  useEffect(() => {
    if (!state || startupVersionsRefreshDone) {
      return;
    }
    setStartupVersionsRefreshDone(true);
    startupRefreshVersions()
      .then((cached) => {
        setState((current) =>
          current ? { ...current, versions: cached } : current,
        );
      })
      .catch((error) => setNotice(toMessage(error)));
  }, [startupVersionsRefreshDone, state]);

  // 收尾一次版本刷新：仅当 token 与“最新请求”一致时才更新界面，避免
  // 过期/并发请求覆盖当前状态。无论成功或失败都会把加载状态重置为 false。
  function cancelVersionsRefreshTimer() {
    if (versionsRefreshTimerRef.current !== null) {
      window.clearTimeout(versionsRefreshTimerRef.current);
      versionsRefreshTimerRef.current = null;
    }
  }

  function finishVersionsRefresh(token: number, success: boolean, message?: string) {
    if (token !== versionsRefreshTokenRef.current) {
      return;
    }
    cancelVersionsRefreshTimer();
    versionsRefreshingRef.current = false;
    setVersionsRefreshing(false);
    if (message) {
      setNotice(message);
    }
    logFrontend(success ? "info" : "warn", `版本列表刷新结束（${success ? "成功" : "失败"}）`);
  }

  // Listen for fresh version data pushed from the backend.
  useEffect(() => {
    const unlisten = onVersionsRefreshed((versions) => {
      setState((current) => (current ? { ...current, versions } : current));
      setVersionsRefreshedAt(Date.now());
      logFrontend("info", `远端版本数据到达：${versions.length} 个版本`);
      // 若本次事件来自一次用户触发的刷新，则正式收尾并复位加载状态。
      if (versionsRefreshingRef.current) {
        finishVersionsRefresh(
          versionsRefreshTokenRef.current,
          true,
          `版本列表已刷新（${versions.length} 个版本）`,
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // 游戏会话实时同步：后端在进程启动/退出时推送最新实例状态，
  // 前端就地更新，无需重启启动器即可看到运行时长与运行态变化。
  useEffect(() => {
    const startUnlisten = onGameSessionStarted((instance) => {
      setState((current) =>
        current
          ? {
              ...current,
              instances: current.instances.map((item) =>
                item.id === instance.id ? instance : item,
              ),
            }
          : current,
      );
    });
    const endUnlisten = onGameSessionEnded((instance) => {
      setState((current) =>
        current
          ? {
              ...current,
              instances: current.instances.map((item) =>
                item.id === instance.id ? instance : item,
              ),
            }
          : current,
      );
    });
    return () => {
      startUnlisten.then((fn) => fn());
      endUnlisten.then((fn) => fn());
    };
  }, []);

  const instancesById = useMemo(() => {
    const map = new Map<string, InstalledInstance>();
    for (const instance of state?.instances ?? []) {
      map.set(instance.id, instance);
    }
    return map;
  }, [state?.instances]);

  const installedInstances = useMemo(
    () =>
      [...(state?.instances ?? [])].sort(
        (a, b) => new Date(b.installedAt).getTime() - new Date(a.installedAt).getTime(),
      ),
    [state?.instances],
  );

  // 总游戏时长：汇总所有已安装实例的累计游玩时长（秒）。
  const totalPlaySeconds = useMemo(
    () =>
      (state?.instances ?? []).reduce(
        (sum, instance) => sum + (instance.totalPlaySeconds ?? 0),
        0,
      ),
    [state?.instances],
  );

  // 最近游玩时间：取所有实例中“最近一次启动”的最大值。
  const lastPlayedAt = useMemo(() => {
    const times = (state?.instances ?? [])
      .map((instance) => instance.lastLaunchedAt)
      .filter((value): value is string => Boolean(value));
    if (times.length === 0) {
      return null;
    }
    return times.reduce((a, b) => (new Date(a) >= new Date(b) ? a : b));
  }, [state?.instances]);

  const visibleVersions = useMemo(() => {
    const settings = draft ?? state?.settings;
    if (!settings || !state) {
      return [];
    }
    return state.versions.filter((version) => isChannelVisible(version.channel, settings));
  }, [draft, state]);

  const runtimeConflicts = useMemo(
    () => getRuntimeConflicts(state?.runtimes ?? []),
    [state?.runtimes],
  );

  const remoteRuntimeOptions = useMemo(
    () => filterRemoteRuntimes(remoteRuntimes, state?.runtimes ?? []),
    [remoteRuntimes, state?.runtimes],
  );

  const selectedRemoteRuntime = useMemo(
    () =>
      remoteRuntimeOptions.find((runtime) => runtime.id === selectedRuntimeId) ??
      remoteRuntimeOptions[0] ??
      null,
    [remoteRuntimeOptions, selectedRuntimeId],
  );

  useEffect(() => {
    if (
      remoteRuntimeOptions.length > 0 &&
      !remoteRuntimeOptions.some((runtime) => runtime.id === selectedRuntimeId)
    ) {
      setSelectedRuntimeId(remoteRuntimeOptions[0].id);
    }
  }, [remoteRuntimeOptions, selectedRuntimeId]);

  const handleTaskEvent = useCallback((message: TaskEvent) => {
    const taskId = message.data.taskId;
    if (message.event === "failed") {
      logFrontend("error", `[下载] ${taskId}: ${(message.data as Record<string, unknown>).message ?? ""}`);
    }
    setTasks((current) => {
      if (message.event === "started") {
        return {
          ...current,
          [taskId]: {
            id: taskId,
            label: message.data.label,
            downloadedBytes: 0,
            totalBytes: message.data.totalBytes,
            bytesPerSecond: undefined,
            status: "running",
            message: message.data.message ?? "连接中",
          },
        };
      }
      const existing = current[taskId] ?? {
        id: taskId,
        label: taskId,
        downloadedBytes: 0,
        status: "running" as const,
      };
      if (message.event === "progress") {
        return {
          ...current,
          [taskId]: {
            ...existing,
            downloadedBytes: message.data.downloadedBytes,
            totalBytes: message.data.totalBytes ?? existing.totalBytes,
            bytesPerSecond: message.data.bytesPerSecond,
            status: "running",
            message: message.data.message ?? existing.message,
          },
        };
      }
      if (message.event === "paused") {
        return {
          ...current,
          [taskId]: {
            ...existing,
            downloadedBytes: message.data.downloadedBytes,
            totalBytes: message.data.totalBytes ?? existing.totalBytes,
            status: "paused",
            message: message.data.message,
          },
        };
      }
      if (message.event === "finished") {
        return {
          ...current,
          [taskId]: {
            ...existing,
            downloadedBytes: existing.totalBytes ?? existing.downloadedBytes,
            status: "finished",
            message: message.data.message,
          },
        };
      }
      if (message.event === "canceled") {
        return {
          ...current,
          [taskId]: {
            ...existing,
            bytesPerSecond: undefined,
            status: "canceled",
            message: message.data.message,
          },
        };
      }
      return {
        ...current,
        [taskId]: {
          ...existing,
          status: "failed",
          message: message.data.message,
        },
      };
    });
    if (message.event === "finished" || message.event === "canceled") {
      window.setTimeout(() => {
        setTasks((current) => {
          if (current[taskId]?.status !== "finished" && current[taskId]?.status !== "canceled") {
            return current;
          }
          const next = { ...current };
          delete next[taskId];
          return next;
        });
      }, 5000);
    }
  }, []);

  function showDownloadToast() {
    if (toastTimerRef.current !== null) {
      window.clearTimeout(toastTimerRef.current);
    }
    setToastMessage({ id: Date.now(), text: "正在下载，请稍候…" });
    toastTimerRef.current = window.setTimeout(() => {
      setToastMessage(null);
      toastTimerRef.current = null;
    }, 2600);
  }

  async function runWithBusy<T>(key: string, action: () => Promise<T>, done?: string) {
    setBusy(key);
    try {
      const result = await action();
      if (done) {
        setNotice(done);
      }
      return result;
    } catch (error) {
      const message = toMessage(error);
      setNotice(message);
      return undefined;
    } finally {
      setBusy(null);
    }
  }

  async function onPauseTask(taskId: string) {
    try {
      await pauseDownload(taskId);
      setNotice("下载已暂停");
    } catch (error) {
      setNotice(toMessage(error));
    }
  }

  async function onResumeTask(taskId: string) {
    try {
      await resumeDownload(taskId);
      setNotice("继续下载");
    } catch (error) {
      setNotice(toMessage(error));
    }
  }

  async function onCancelTask(taskId: string) {
    try {
      await cancelDownload(taskId);
      setNotice("正在取消下载");
    } catch (error) {
      setNotice(toMessage(error));
    }
  }

  function onDismissTask(taskId: string) {
    setTasks((current) => {
      const next = { ...current };
      delete next[taskId];
      return next;
    });
  }

  async function onRefreshVersions() {
    logFrontend("info", "用户触发了版本列表刷新");
    // 递增请求令牌：本次刷新标记为“最新请求”，过期请求的收尾将被忽略。
    const token = (versionsRefreshTokenRef.current += 1);
    versionsRefreshingRef.current = true;
    setVersionsRefreshing(true);
    setNotice("版本列表正在后台刷新");
    // 启动超时保护：后端在超时阈值内仍未回传数据则强制结束加载状态，
    // 防止网络超时或接口无响应导致提示卡死。
    cancelVersionsRefreshTimer();
    versionsRefreshTimerRef.current = window.setTimeout(() => {
      logFrontend("warn", "版本列表刷新超时，强制结束加载状态");
      finishVersionsRefresh(token, false, "版本列表刷新超时，请检查网络后重试");
    }, VERSION_REFRESH_TIMEOUT_MS);

    try {
      // startup_refresh_versions 立即返回缓存数据，真正的刷新结果通过
      // "versions-refreshed" 事件异步送达（由上面的监听器收尾）。
      const cached = await startupRefreshVersions();
      logFrontend("info", `版本列表刷新响应：缓存 ${cached.length} 个版本`);
      setState((current) => (current ? { ...current, versions: cached } : current));
      refreshAccelerators()
        .then((accelerators) => {
          logFrontend("info", `加速源刷新响应：${accelerators.sources.length} 个加速源`);
          setState((current) => (current ? { ...current, accelerators } : current));
        })
        .catch((error) =>
          logFrontend("error", `加速源刷新失败：${toMessage(error)}`),
        );
    } catch (error) {
      // 缓存读取或命令调用本身失败：立即收尾并复位加载状态，不依赖事件。
      const message = toMessage(error);
      logFrontend("error", `版本列表刷新失败：${message}`);
      finishVersionsRefresh(token, false, message);
    }
  }

  async function onRefreshAccelerators() {
    logFrontend("info", "用户触发了加速源刷新");
    await runWithBusy("accelerators", async () => {
      const accelerators = await refreshAccelerators();
      setState((current) => (current ? { ...current, accelerators } : current));
    }, "GitHub 加速列表已刷新");
  }

  async function onPingAccelerator(source: Accelerator) {
    setPinging((prev) => ({ ...prev, [source.id]: true }));
    try {
      const result = await pingAccelerator(source);
      setPingResults((prev) => ({ ...prev, [source.id]: result }));
    } catch (error) {
      setPingResults((prev) => ({
        ...prev,
        [source.id]: { sourceId: source.id, latencyMs: null, error: toMessage(error) },
      }));
    } finally {
      setPinging((prev) => ({ ...prev, [source.id]: false }));
    }
  }

  async function onSelectUpgradeTarget(instanceId: string) {
    const instance = instancesById.get(instanceId);
    if (!instance) return;
    if (instance.runningPid) {
      setNotice("游戏正在运行，请先关闭");
      return;
    }
    setUpgradeTargetId(instanceId);
    setEditingInstanceId(null);
    setView("versions");
    setNotice(`选择 ${instance.version} 的升降级目标版本`);
  }

  async function onUpgradeToVersion(targetVersion: RemoteVersion) {
    if (!upgradeTargetId) return;
    const targetId = upgradeTargetId;
    // 升降级必须基于一个已存在的实例；若标识失效则直接拒绝，绝不回退到新建/安装流程。
    if (!instancesById.get(targetId)) {
      setUpgradeTargetId(null);
      setNotice("待升降级的游戏实例不存在，请刷新后重试");
      return;
    }
    const source = instancesById.get(targetId)!;
    const direction = compareVersionDirection(source.version, targetVersion.version);
    const channel = new Channel<TaskEvent>();
    channel.onmessage = handleTaskEvent;
    // 用 upgradeTo:{id} 占用 busy，使按钮据此禁用并显示进度，避免并发重复触发。
    setUpgradeFeedback(null);
    setBusy(`upgradeTo:${targetVersion.id}`);
    try {
      await upgradeInstance(targetId, targetVersion, channel);
      setUpgradeTargetId(null);
      await reload();
      const label = targetVersion.name || targetVersion.tag;
      const dirText =
        direction === "upgrade" ? "升级" : direction === "downgrade" ? "降级" : "切换";
      const message = `${dirText}成功：已切换至 ${label}`;
      setNotice(message);
      // 成功状态反馈：明确方向、目标版本，并告知存档已保留（无损回滚）。
      setUpgradeFeedback({
        status: "success",
        title: `${dirText}成功`,
        detail: `已成功${dirText}至 ${label}（${targetVersion.version}），游戏存档已保留。`,
      });
    } catch (error) {
      // 失败状态反馈：给出具体错误原因，便于用户针对性处理。
      const detail = describeUpgradeError(error);
      setNotice(`升降级失败：${detail}`);
      setUpgradeFeedback({ status: "error", title: "升降级失败", detail });
    } finally {
      setBusy(null);
    }
  }

  async function applySettings(nextDraft: Settings, refreshAfterSave = false) {
    const saved = await saveSettings(nextDraft);
    setDraft(saved);
    setState((current) => (current ? { ...current, settings: saved } : current));
    if (refreshAfterSave) {
      const versions = await refreshVersions();
      setState((current) => (current ? { ...current, versions, settings: saved } : current));
    }
    return saved;
  }

  async function onSaveSettings(nextDraft = draft) {
    if (!nextDraft) {
      return;
    }
    await runWithBusy("settings", async () => {
      const saved = await applySettings(nextDraft, false);
      if (saved.debugMode) {
        logFrontend("info", "调试模式已即时开启");
      }
      setNotice(saved.debugMode ? "调试模式已开启，日志窗口同步打开" : "设置已保存");
    });
  }

  async function onToggleVersionChannel(key: keyof ChannelVisibility, _value: boolean) {
    const base = draft ?? state?.settings;
    if (!base) {
      return;
    }
    const isBeChannel = key === "mindustryBe" || key === "mindustryXbe";
    // 单选模式：只激活点击的 channel，关闭其他 channel
    const next: Settings = {
      ...base,
      showBe: isBeChannel ? true : base.showBe,
      channelVisibility: {
        mindustry: false,
        mindustryX: false,
        mindustryBe: base.showBe ? false : base.channelVisibility.mindustryBe,
        mindustryXbe: base.showBe ? false : base.channelVisibility.mindustryXbe,
        [key]: true,
      },
    };
    setDraft(next);
    await runWithBusy("channelFilter", async () => {
      await applySettings(next, false);
    }, "已切换至" + (channelVisibilityKeys.find((k) => k.key === key)?.label ?? key));
  }

  async function onInstall(version: RemoteVersion) {
    const channel = new Channel<TaskEvent>();
    channel.onmessage = handleTaskEvent;
    await runWithBusy(`install:${version.id}`, async () => {
      const instance = await installVersion(version, channel);
      await reload();
      setNotice(`${instance.version} 已安装`);
    });
  }

  async function onSwitchVersion(version: RemoteVersion) {
    if (busy === `install:${version.id}` || busy === `switch:${version.id}`) {
      showDownloadToast();
      setNotice("正在下载，请稍候…");
      return;
    }
    const sameChannelInstances = (state?.instances ?? []).filter(
      (instance) => instance.channel === version.channel && instance.id !== version.id,
    );
    if (sameChannelInstances.length > 0) {
      const confirmed = window.confirm(
        `切换到 ${version.name || version.tag} 会移除 ${sameChannelInstances
          .map((instance) => instance.version)
          .join("、")} 的游戏文件和隔离数据。\n\n确认切换？`,
      );
      if (!confirmed) {
        setNotice("已取消版本切换");
        return;
      }
    }
    showDownloadToast();
    setNotice("正在下载，请稍候…");
    const channel = new Channel<TaskEvent>();
    channel.onmessage = handleTaskEvent;
    await runWithBusy(`switch:${version.id}`, async () => {
      const instance = await switchVersion(version, channel);
      await reload();
      setNotice(`已切换到 ${instance.version}`);
    });
  }

  async function installRemoteRuntime(runtime: RemoteRuntime, actionLabel = "下载") {
    if (!runtime) {
      setNotice("暂无可下载的运行时");
      return;
    }
    const conflicts = getConflictingLocalRuntimes(runtime, state?.runtimes ?? []);
    if (conflicts.length > 0) {
      const confirmed = window.confirm(
        `本地已有 ${formatRuntimeName(runtime)} 相关运行时，继续${actionLabel}后可能出现运行时冲突。\n\n可在设置页禁用其中一个运行时来消除冲突。`,
      );
      if (!confirmed) {
        setNotice("已取消运行时操作");
        return;
      }
    }
    const channel = new Channel<TaskEvent>();
    channel.onmessage = handleTaskEvent;
    await runWithBusy(`runtime:${runtime.id}`, async () => {
      await installRuntime(runtime, channel);
      const next = await reload();
      const conflicts = getRuntimeConflicts(next.runtimes);
      if (conflicts.length > 0) {
        setNotice(`JRE ${runtime.javaVersion} 已准备，但发现 ${conflicts.length} 组运行时冲突`);
      } else {
        setNotice(`JRE ${runtime.javaVersion} 已准备`);
      }
    });
  }

  async function onInstallSelectedRuntime(runtime = selectedRemoteRuntime) {
    if (!runtime) {
      setNotice("暂无可下载的运行时");
      return;
    }
    await installRemoteRuntime(runtime);
  }

  async function onReinstallRuntime(runtime: RuntimeInfo) {
    const remote = findRemoteForLocalRuntime(runtime, remoteRuntimes);
    if (!remote) {
      setNotice(`没有找到可重新下载的 ${formatRuntimeName(runtime)} 远端包`);
      return;
    }
    await installRemoteRuntime(remote, "重新下载");
  }

  async function onToggleRuntimeEnabled(runtime: RuntimeInfo, enabled: boolean) {
    await runWithBusy(`runtimeToggle:${runtime.id}`, async () => {
      const runtimes = await setRuntimeEnabled(runtime.id, enabled);
      setState((current) => (current ? { ...current, runtimes } : current));
      const conflicts = getRuntimeConflicts(runtimes);
      if (conflicts.length > 0) {
        setNotice(`运行时已${enabled ? "启用" : "禁用"}，当前仍有 ${conflicts.length} 组冲突`);
      } else {
        setNotice(`运行时已${enabled ? "启用" : "禁用"}`);
      }
    });
  }

  async function onDeleteRuntime(runtime: RuntimeInfo) {
    if (!canDeleteRuntime(runtime)) {
      setNotice("只能删除启动器下载的运行时");
      return;
    }
    setDeleteConfirmation({ kind: "runtime", runtime });
  }

  async function onImportRuntime() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择 Java / JRE 根目录",
    });
    if (typeof selected !== "string") {
      return;
    }
    await runWithBusy("importRuntime", async () => {
      const runtime = await importRuntime(selected);
      const next = await reload();
      setRuntimeGuideOpen(false);
      const conflicts = getRuntimeConflicts(next.runtimes);
      setNotice(
        conflicts.length > 0
          ? `已导入 JRE ${runtime.javaVersion}，发现 ${conflicts.length} 组运行时冲突`
          : `已导入 JRE ${runtime.javaVersion}`,
      );
    });
  }

  async function onScanRuntimes() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择要检索 Java 运行时的目录",
    });
    if (typeof selected !== "string") {
      return;
    }
    await runWithBusy("scanRuntime", async () => {
      const found = await scanRuntimes(selected);
      const next = await reload();
      setRuntimeGuideOpen(false);
      const conflicts = getRuntimeConflicts(next.runtimes);
      setNotice(
        conflicts.length > 0
          ? `检索到 ${found.length} 个可用运行时，发现 ${conflicts.length} 组冲突`
          : `检索到 ${found.length} 个可用运行时`,
      );
    });
  }

  async function onDismissRuntimeGuide() {
    const base = draft ?? state?.settings;
    if (!base) {
      setRuntimeGuideOpen(false);
      return;
    }
    const next = { ...base, runtimePromptDismissed: true };
    setRuntimeGuideOpen(false);
    await runWithBusy("dismissRuntimeGuide", async () => {
      await applySettings(next, false);
    }, "运行时提示已关闭");
  }

  async function onLaunch(instance: InstalledInstance) {
    await runWithBusy(`launch:${instance.id}`, async () => {
      try {
        const result = await launchVersion(instance.id);
        // 立即在前端标记运行态，配合后端 game-session-started/ended 实时同步时长。
        setState((current) =>
          current
            ? {
                ...current,
                instances: current.instances.map((item) =>
                  item.id === instance.id
                    ? {
                        ...item,
                        runningPid: result.pid || item.runningPid,
                        runningSince: item.runningSince ?? new Date().toISOString(),
                      }
                    : item,
                ),
              }
            : current,
        );
        setNotice(`已启动 PID ${result.pid}`);
      } catch (error) {
        const message = toMessage(error);
        // 后端基于 PID 存活校验判定“已在运行”时，弹出明确警告而非普通提示。
        if (message.includes("已在运行中") || message.includes("重复启动")) {
          setLaunchConflict({ version: instance.version, message });
        } else {
          throw error;
        }
      }
    });
  }

  async function onDelete(instance: InstalledInstance) {
    setDeleteConfirmation({ kind: "game", instance });
  }

  async function onConfirmDelete() {
    const pending = deleteConfirmation;
    if (!pending) {
      return;
    }
    if (pending.kind === "runtime") {
      await runWithBusy(`runtimeDelete:${pending.runtime.id}`, async () => {
        await deleteRuntime(pending.runtime.id);
        setDeleteConfirmation(null);
        await reload();
      }, `${formatRuntimeName(pending.runtime)} 已删除`);
      return;
    }

    await runWithBusy(`delete:${pending.instance.id}`, async () => {
      const instances = await deleteInstance(pending.instance.id);
      setState((current) => (current ? { ...current, instances } : current));
      setDeleteConfirmation(null);
    }, `${pending.instance.version} 已移除`);
  }

  function onCancelDelete() {
    if (deleteConfirmation?.kind === "runtime") {
      setNotice("已取消删除运行时");
    } else if (deleteConfirmation?.kind === "game") {
      setNotice("已取消删除游戏");
    }
    setDeleteConfirmation(null);
  }

  async function onIgnoreVersion(version: string) {
    try {
      const saved = await ignoreLauncherVersion(version);
      setShowUpdatePrompt(false);
      setDraft(saved);
      setState((current) => (current ? { ...current, settings: saved } : current));
      setNotice("已忽略该版本");
    } catch (error) {
      setNotice(toMessage(error));
    }
  }

  async function handleCheckUpdate() {
    logFrontend("info", "用户触发了启动器更新检查");
    await runWithBusy("checkUpdate", async () => {
      try {
        const info = await checkLauncherUpdate();
        setUpdateInfo(info);
        if (info.hasUpdate) {
          setShowUpdatePrompt(true);
        } else if (info.errorMessage) {
          setNotice(info.errorMessage);
        } else {
          setNotice("已是最新版本");
        }
      } catch (error) {
        setNotice(toMessage(error));
      }
    });
  }

  function onOpenInstanceSettings(instance: InstalledInstance) {
    setEditingInstanceId(instance.id);
    setInstanceSettingsDraft({
      runtimeId: instance.runtimeId ?? "",
      launchSettings: {
        minMemoryMb: instance.launchSettings?.minMemoryMb ?? null,
        maxMemoryMb: instance.launchSettings?.maxMemoryMb ?? null,
        extraJvmArgs: instance.launchSettings?.extraJvmArgs ?? "",
        gameArgs: instance.launchSettings?.gameArgs ?? "",
      },
    });
  }

  async function onSaveInstanceSettings() {
    if (!editingInstanceId || !instanceSettingsDraft) {
      return;
    }
    await runWithBusy(`instanceSettings:${editingInstanceId}`, async () => {
      const instances = await saveInstanceLaunchSettings(
        editingInstanceId,
        instanceSettingsDraft.runtimeId || null,
        instanceSettingsDraft.launchSettings,
      );
      setState((current) => (current ? { ...current, instances } : current));
      setEditingInstanceId(null);
      setInstanceSettingsDraft(null);
    }, "版本启动设置已保存");
  }

  function updateInstanceLaunchSettings(patch: Partial<LaunchSettings>) {
    setInstanceSettingsDraft((current) =>
      current
        ? {
            ...current,
            launchSettings: {
              ...current.launchSettings,
              ...patch,
            },
          }
        : current,
    );
  }

  async function onPickInstallRoot() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择安装根目录",
    });
    if (typeof selected !== "string") {
      return;
    }
    await runWithBusy("migrate", async () => {
      await migrateInstallRoot(selected);
      await reload();
    }, "安装根目录已切换");
  }

  function updateDraft(patch: Partial<Settings>) {
    setDraft((current) => (current ? { ...current, ...patch } : current));
  }

  const taskList = Object.values(tasks).sort((a, b) => a.id.localeCompare(b.id));
  const pendingRuntimeConflicts = getPendingRuntimeConflicts(
    taskList,
    busy,
    remoteRuntimes,
    state?.runtimes ?? [],
  );
  const editingInstance = editingInstanceId ? (instancesById.get(editingInstanceId) ?? null) : null;
  const pageTitle =
    view === "games"
      ? "游戏"
      : view === "versions"
        ? "版本列表"
        : view === "runtimes"
          ? "运行时"
          : view === "debug"
            ? "调试控制台"
            : "启动器设置";
  const pageNotice =
    view === "games"
      ? installedInstances.length > 0
        ? `已安装 ${installedInstances.length} 个游戏版本`
        : "暂无已安装游戏，打开版本列表选择安装"
      : view === "runtimes"
        ? "管理本地与远端 Java 运行时"
        : view === "debug"
          ? "实时日志流"
          : notice;

  return (
    <div className="app-shell">
      <TitleBar />
      <div className="app-backdrop" aria-hidden="true" />
      <div className="wiki-float-layer" aria-hidden="true">
        {floatItems.map((item) => (
          <img
            key={item.key}
            className="wiki-float-item"
            src={item.url}
            alt=""
            style={item.style}
          />
        ))}
      </div>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <img className="brand-core" src={coreSprite} alt="" />
            <img className="brand-unit" src={alphaSprite} alt="" />
          </div>
          <div>
            <strong>Mindustry</strong>
            <span>Launcher</span>
          </div>
        </div>
        <button
          className={view === "games" ? "nav-button active" : "nav-button"}
          onClick={() => {
            setUpgradeTargetId(null);
            setView("games");
            setNotice("已安装游戏列表已就绪");
          }}
          title="游戏"
        >
          <Play size={18} />
          <span>游戏</span>
        </button>
        <button
          className={view === "versions" ? "nav-button active" : "nav-button"}
          onClick={() => {
            setUpgradeTargetId(null);
            setView("versions");
            setNotice("选择版本进行切换");
          }}
          title="版本列表"
        >
          <Layers size={18} />
          <span>版本列表</span>
        </button>
        <button
          className={view === "runtimes" ? "nav-button active" : "nav-button"}
          onClick={() => {
            setUpgradeTargetId(null);
            setView("runtimes");
            setNotice("管理本地与远端 Java 运行时");
          }}
          title="运行时"
        >
          <Cpu size={18} />
          <span>运行时</span>
        </button>
        <button
          className={view === "settings" ? "nav-button active" : "nav-button"}
          onClick={() => {
            setUpgradeTargetId(null);
            setView("settings");
            setNotice("管理安装目录与加速源");
          }}
          title="设置"
        >
          <SettingsIcon size={18} />
          <span>设置</span>
        </button>
        {state?.settings.debugMode && (
          <button
            className={view === "debug" ? "nav-button active" : "nav-button"}
            onClick={() => {
              setUpgradeTargetId(null);
              setView("debug");
              setNotice("调试控制台已打开");
            }}
            title="调试"
          >
            <Bug size={18} />
            <span>调试</span>
          </button>
        )}
        <div className="sidebar-footer">
          <span>{state?.instances.length ?? 0} 个实例</span>
          <span>{state?.runtimes.length ?? 0} 个运行时</span>
        </div>
      </aside>

      <main className="workspace">
        {upgradeFeedback && (
          <div
            className={`upgrade-feedback ${upgradeFeedback.status}`}
            role={upgradeFeedback.status === "error" ? "alert" : "status"}
            aria-live={upgradeFeedback.status === "error" ? "assertive" : "polite"}
          >
            {upgradeFeedback.status === "success" ? (
              <CheckCircle2 size={18} className="uf-icon" />
            ) : (
              <AlertTriangle size={18} className="uf-icon" />
            )}
            <div className="uf-text">
              <strong className="uf-title">{upgradeFeedback.title}</strong>
              <span className="uf-detail">{upgradeFeedback.detail}</span>
            </div>
            <button
              className="uf-close"
              title="关闭"
              onClick={() => setUpgradeFeedback(null)}
            >
              <X size={16} />
            </button>
          </div>
        )}
        <header className="topbar">
          <div>
            <h1>{pageTitle}</h1>
            <p>{pageNotice}</p>
          </div>
          <div className="top-actions">
            {view === "games" && (
              <IconButton
                title="打开版本列表"
                label="版本列表"
                onClick={() => { setUpgradeTargetId(null); setView("versions"); }}
              >
                <Layers size={17} />
              </IconButton>
            )}
            {view === "versions" && (
              <>
                {versionsRefreshedAt > 0 && (
                  <span className="refresh-timestamp">上次刷新 {formatRefreshTime(versionsRefreshedAt)}</span>
                )}
                <IconButton
                  title="刷新版本列表"
                  label="刷新"
                  busy={versionsRefreshing}
                  onClick={onRefreshVersions}
                >
                  <RefreshCcw size={17} />
                </IconButton>
              </>
            )}
          </div>
        </header>

        {view === "games" ? (
          <section className="content-grid">
            <div className="playtime-summary">
              <div className="playtime-stat">
                <span>总游戏时长</span>
                <strong>{formatPlaytime(totalPlaySeconds)}</strong>
              </div>
              <div className="playtime-stat">
                <span>已安装版本</span>
                <strong>{installedInstances.length}</strong>
              </div>
              <div className="playtime-stat">
                <span>最近游玩</span>
                <strong>{lastPlayedAt ? formatDate(lastPlayedAt) : "暂无记录"}</strong>
              </div>
            </div>
            <div className="version-list">
              {installedInstances.length === 0 ? (
                <div className="empty-state game-empty">
                  <div className="empty-visual mindustry-visual">
                    <img className="empty-core" src={coreSprite} alt="" />
                    <img className="empty-alpha" src={alphaSprite} alt="" />
                    <img className="empty-zenith" src={zenithSprite} alt="" />
                  </div>
                  <div className="empty-copy">
                    <strong>暂无已安装游戏</strong>
                    <span>打开版本列表，选择一个版本切换到本地。</span>
                  </div>
                  <button onClick={() => { setUpgradeTargetId(null); setView("versions"); }}>
                    <Layers size={17} />
                    <span>打开版本列表</span>
                  </button>
                </div>
              ) : (
                installedInstances.map((instance) => {
                  const runtime = (state?.runtimes ?? []).find(
                    (item) => item.id === instance.runtimeId,
                  );
                  return (
                    <article className="version-row game-row" key={instance.id}>
                      <div className="version-main">
                        <ChannelBadge channel={instance.channel} sprites={channelSprites} />
                          <div>
                            <h2>{instance.version}</h2>
                            <div className="version-meta">
                              <span>安装于 {formatDate(instance.installedAt)}</span>
                              <span>
                                {runtime
                                  ? `${formatRuntimeName(runtime)} · ${runtimeSourceLabel(runtime)}`
                                  : "自动选择运行时"}
                              </span>
                              <span>版本隔离</span>
                              {instance.runningPid ? (
                                <span className="running-tag">运行中</span>
                              ) : (
                                <span>
                                  {instance.totalPlaySeconds > 0
                                    ? `游玩 ${formatPlaytime(instance.totalPlaySeconds)}`
                                    : "未游玩过"}
                                </span>
                              )}
                            </div>
                          </div>
                      </div>
                      <div className="version-actions">
                        <IconButton
                          title="启动游戏"
                          label="启动"
                          busy={busy === `launch:${instance.id}`}
                          onClick={() => onLaunch(instance)}
                        >
                          <Play size={17} />
                        </IconButton>
                        <button
                          className="icon-only"
                          title="游戏设置"
                          onClick={() => onOpenInstanceSettings(instance)}
                        >
                          <SlidersHorizontal size={17} />
                        </button>
                        <button
                          className="icon-only danger"
                          title="删除游戏"
                          onClick={() => onDelete(instance)}
                        >
                          <Trash2 size={17} />
                        </button>
                      </div>
                    </article>
                  );
                })
              )}
            </div>
          </section>
        ) : view === "versions" ? (
          <section className="version-browser">
            <div className="version-list version-list-full">
              {upgradeTargetId && instancesById.get(upgradeTargetId) && (
                <div className="upgrade-hint">
                  <span>升降级模式：选择 {instancesById.get(upgradeTargetId)!.version} 的目标版本</span>
                  <button className="mini-button" onClick={() => setUpgradeTargetId(null)}>
                    <X size={14} />
                    <span>取消</span>
                  </button>
                </div>
              )}
              <ChannelStrip
                settings={draft ?? state?.settings ?? null}
                busy={busy === "channelFilter"}
                onToggle={onToggleVersionChannel}
                sprites={channelSprites}
              />
              {visibleVersions.length === 0 ? (
                <div className="empty-state">
                  <RefreshCcw size={28} />
                  <span>暂无版本数据</span>
                  <button onClick={onRefreshVersions} disabled={versionsRefreshing}>
                    刷新版本
                  </button>
                </div>
              ) : (
                visibleVersions.map((version) => {
                  const instance = instancesById.get(version.id);
                  return (
                    <article className="version-row" key={version.id}>
                      <div className="version-main">
                        <ChannelBadge channel={version.channel} sprites={channelSprites} />
                        <div>
                          <h2>{version.name || version.tag}</h2>
                          <div className="version-meta">
                            <span>{version.tag}</span>
                            <span>{formatDate(version.publishedAt)}</span>
                            <span>{formatBytes(version.selectedAsset?.size)}</span>
                          </div>
                        </div>
                      </div>
                      <div className="version-actions">
                        {upgradeTargetId && instancesById.get(upgradeTargetId) ? (
                          <IconButton
                            title="更换为此版本"
                            label="更换版本"
                            busy={busy === `upgradeTo:${version.id}`}
                            onClick={() => onUpgradeToVersion(version)}
                          >
                            <RefreshCcw size={17} />
                          </IconButton>
                        ) : instance ? (
                          <>
                            <IconButton
                              title="启动游戏"
                              label="启动"
                              busy={busy === `launch:${instance.id}`}
                              onClick={() => onLaunch(instance)}
                            >
                              <Play size={17} />
                            </IconButton>
                            <button
                              className="icon-only"
                              title="游戏设置"
                              onClick={() => onOpenInstanceSettings(instance)}
                            >
                              <SlidersHorizontal size={17} />
                            </button>
                            <button
                              className="icon-only danger"
                              title="删除游戏"
                              onClick={() => onDelete(instance)}
                            >
                              <Trash2 size={17} />
                            </button>
                          </>
                        ) : (
                          <IconButton
                            title="下载此版本"
                            label="下载"
                            busy={
                              busy === `install:${version.id}` || busy === `switch:${version.id}`
                            }
                            onClick={() => onSwitchVersion(version)}
                          >
                            <Download size={17} />
                          </IconButton>
                        )}
                      </div>
                    </article>
                  );
                })
              )}
            </div>
          </section>
        ) : view === "runtimes" ? (
          <RuntimesView
            runtimes={state?.runtimes ?? []}
            remoteRuntimes={remoteRuntimes}
            runtimeConflicts={runtimeConflicts}
            busy={busy}
            refreshedAt={runtimesRefreshedAt}
            onImportRuntime={onImportRuntime}
            onScanRuntimes={onScanRuntimes}
            onRefreshRuntimeCatalog={() => { setNotice("运行时列表正在后台刷新"); void refreshRuntimesInBackground(); }}
            onToggleRuntimeEnabled={onToggleRuntimeEnabled}
            onReinstallRuntime={onReinstallRuntime}
            onDeleteRuntime={onDeleteRuntime}
            onInstallRemoteRuntime={installRemoteRuntime}
          />
        ) : (
          <SettingsView
            state={state}
            draft={draft}
            busy={busy}
            theme={theme}
            setTheme={setTheme}
            updateDraft={updateDraft}
            onSave={() => onSaveSettings()}
            onImportRuntime={onImportRuntime}
            onScanRuntimes={onScanRuntimes}
            onRefreshRuntimeCatalog={() => { setNotice("运行时列表正在后台刷新"); void refreshRuntimesInBackground(); }}
            onRefreshAccelerators={onRefreshAccelerators}
            onToggleRuntimeEnabled={onToggleRuntimeEnabled}
            onReinstallRuntime={onReinstallRuntime}
            onDeleteRuntime={onDeleteRuntime}
            onInstallRemoteRuntime={installRemoteRuntime}
            remoteRuntimes={remoteRuntimes}
            runtimeConflicts={runtimeConflicts}
            refreshedAt={runtimesRefreshedAt}
            onPickInstallRoot={onPickInstallRoot}
            onOpenInstallRoot={() => {
              void runWithBusy("openRoot", openInstallRoot);
            }}
            onCheckUpdate={handleCheckUpdate}
            updateInfo={updateInfo}
            pingResults={pingResults}
            pinging={pinging}
            onPingAccelerator={onPingAccelerator}
          />
        )}
        <div className="debug-view" style={{ display: view === "debug" ? "grid" : "none" }}>
          <DebugLogWindow />
        </div>
        {runtimeGuideOpen && state && (
          <RuntimeGuide
            runtimes={remoteRuntimeOptions}
            selectedRuntime={selectedRemoteRuntime}
            busy={busy}
            onSelect={setSelectedRuntimeId}
            onDownload={() => onInstallSelectedRuntime()}
            onGoImport={() => {
              setRuntimeGuideOpen(false);
              setView("runtimes");
            }}
            onDismiss={onDismissRuntimeGuide}
            onClose={() => setRuntimeGuideOpen(false)}
          />
        )}
        {editingInstance && instanceSettingsDraft && (
          <InstanceSettingsModal
            instance={editingInstance}
            runtimes={state?.runtimes ?? []}
            draft={instanceSettingsDraft}
            busy={busy === `instanceSettings:${editingInstance.id}`}
            onRuntimeChange={(runtimeId) =>
              setInstanceSettingsDraft((current) =>
                current ? { ...current, runtimeId } : current,
              )
            }
            onLaunchSettingsChange={updateInstanceLaunchSettings}
            onSave={onSaveInstanceSettings}
            onClose={() => {
              setEditingInstanceId(null);
              setInstanceSettingsDraft(null);
            }}
            onUpgrade={onSelectUpgradeTarget}
          />
        )}
        {deleteConfirmation && (
          <DeleteConfirmModal
            confirmation={deleteConfirmation}
            busy={
              deleteConfirmation.kind === "runtime"
                ? busy === `runtimeDelete:${deleteConfirmation.runtime.id}`
                : busy === `delete:${deleteConfirmation.instance.id}`
            }
            onCancel={onCancelDelete}
            onConfirm={onConfirmDelete}
          />
        )}
        {launchConflict && (
          <LaunchConflictModal
            conflict={launchConflict}
            onClose={() => setLaunchConflict(null)}
          />
        )}
      </main>
      {toastMessage && (
        <div className="toast-layer" role="status" aria-live="polite">
          <div className="toast-message" key={toastMessage.id}>
            <Loader2 className="spin" size={17} />
            <span>{toastMessage.text}</span>
          </div>
        </div>
      )}
      {taskList.length > 0 && (
        <DownloadPanel
          tasks={taskList}
          onPause={onPauseTask}
          onResume={onResumeTask}
          onCancel={onCancelTask}
          onDismiss={onDismissTask}
        />
      )}
      {showUpdatePrompt && updateInfo && (
        <UpdatePromptModal
          info={updateInfo}
          onClose={() => setShowUpdatePrompt(false)}
          onIgnore={onIgnoreVersion}
        />
      )}
    </div>
  );
}

function DownloadPanel({
  tasks,
  onPause,
  onResume,
  onCancel,
  onDismiss,
}: {
  tasks: TaskRecord[];
  onPause: (taskId: string) => void;
  onResume: (taskId: string) => void;
  onCancel: (taskId: string) => void;
  onDismiss: (taskId: string) => void;
}) {
  return (
    <div className="download-panel-layer">
      <div className="download-panel glass-panel">
        <div className="download-panel-head">
          <Download size={15} />
          <span>下载任务</span>
          <span className="download-panel-count">{tasks.length}</span>
        </div>
        {tasks.map((task) => (
          <TaskItem
            key={task.id}
            task={task}
            onPause={onPause}
            onResume={onResume}
            onCancel={onCancel}
            onDismiss={onDismiss}
          />
        ))}
      </div>
    </div>
  );
}

function DebugLogWindow() {
  const [entries, setEntries] = useState<DebugLogEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [busy, setBusy] = useState<"clear" | "dir" | null>(null);
  const [enabled, setEnabled] = useState(false);
  const logRef = useRef<HTMLPreElement | null>(null);

  useEffect(() => {
    readDebugLog()
      .then((snapshot) => {
        setEnabled(snapshot.enabled);
        if (snapshot.content.trim()) {
          setEntries([{
            level: "FILE",
            message: snapshot.content.trim(),
            timestamp: "",
          }]);
        }
      })
      .catch(() => {});

    const unlisten = onDebugLogEntry((entry) => {
      setEnabled(true);
      setEntries((prev) => {
        if (prev.length === 1 && prev[0].level === "FILE") {
          return [prev[0], { level: "FILE", message: "──── 实时事件 ────", timestamp: "" }, entry];
        }
        return [...prev, entry];
      });
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (autoScroll && logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [autoScroll, entries]);

  async function onClearLog() {
    const confirmed = window.confirm("清空当前调试日志？旧归档文件不会删除。");
    if (!confirmed) {
      return;
    }
    setBusy("clear");
    try {
      await clearDebugLog();
      setEntries([]);
      setError(null);
    } catch (error) {
      setError(toMessage(error));
    } finally {
      setBusy(null);
    }
  }

  async function onOpenLogDir() {
    setBusy("dir");
    try {
      await openDebugLogDir();
    } catch (error) {
      setError(toMessage(error));
    } finally {
      setBusy(null);
    }
  }

  const content = error
    ? error
    : entries.length === 0
      ? "暂无日志"
      : entries.map((e) => {
          if (e.level === "FILE") return e.message;
          const ts = e.timestamp ? new Date(e.timestamp).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" }) + " " : "";
          return `${ts}[${e.level}] ${e.message}`;
        }).join("\n");

  return (
    <section className="debug-log-panel">
      <div className="debug-log-titlebar">
        <div>
          <span>实时日志流 · 共 {entries.length} 条</span>
        </div>
        <div className="debug-log-actions">
          <button
            className={autoScroll ? "active" : ""}
            onClick={() => setAutoScroll((value) => !value)}
            title={autoScroll ? "关闭自动滚动" : "开启自动滚动"}
          >
            <CheckCircle2 size={15} />
            <span>跟随</span>
          </button>
          <button onClick={onOpenLogDir} disabled={busy === "dir"} title="打开日志目录">
            {busy === "dir" ? <Loader2 className="spin" size={15} /> : <FolderOpen size={15} />}
            <span>目录</span>
          </button>
          <button className="danger" onClick={onClearLog} disabled={busy === "clear"} title="清空当前日志">
            {busy === "clear" ? <Loader2 className="spin" size={15} /> : <Trash2 size={15} />}
            <span>清空</span>
          </button>
        </div>
      </div>

      <div className="debug-log-meta">
        <span><b>状态</b>{enabled ? "已启用" : "未启用"}</span>
        <span><b>实时事件</b>流式接收中</span>
        <span><b>条目数</b>{entries.length}</span>
        <span><b>方式</b>实时事件 + 文件归档</span>
      </div>

      <pre ref={logRef} className={error ? "debug-log-content error" : "debug-log-content"}>
        {content}
      </pre>
    </section>
  );
}

function SettingsView(props: {
  state: AppUiState | null;
  draft: Settings | null;
  busy: string | null;
  theme: Theme;
  setTheme: (theme: Theme) => void;
  updateDraft: (patch: Partial<Settings>) => void;
  onSave: () => void;
  onImportRuntime: () => void;
  onScanRuntimes: () => void;
  onRefreshRuntimeCatalog: () => void;
  onRefreshAccelerators: () => void;
  onToggleRuntimeEnabled: (runtime: RuntimeInfo, enabled: boolean) => void;
  onReinstallRuntime: (runtime: RuntimeInfo) => void;
  onDeleteRuntime: (runtime: RuntimeInfo) => void;
  onInstallRemoteRuntime?: (runtime: RemoteRuntime) => void;
  remoteRuntimes: RemoteRuntime[];
  runtimeConflicts: RuntimeConflict[];
  refreshedAt: number;
  onPickInstallRoot: () => void;
  onOpenInstallRoot: () => void;
  onCheckUpdate: () => void;
  updateInfo: LauncherUpdateInfo | null;
  pingResults: Record<string, PingResult>;
  pinging: Record<string, boolean>;
  onPingAccelerator: (source: Accelerator) => void;
}) {
  const { state, draft } = props;
  if (!draft) {
    return <div className="empty-state">正在加载</div>;
  }

  return (
    <section className="settings-layout">
      <div className="settings-panel glass-panel">
        <div className="setting-row">
          <label>主题</label>
          <div className="theme-selector">
            {(["system", "light", "dark"] as const).map((option) => (
              <button
                key={option}
                className={props.theme === option ? "theme-option active" : "theme-option"}
                onClick={() => props.setTheme(option)}
                title={
                  option === "system"
                    ? "跟随系统主题自动切换"
                    : option === "light"
                      ? "始终白天"
                      : "始终黑夜"
                }
              >
                {option === "system" ? (
                  <Monitor size={15} />
                ) : option === "light" ? (
                  <Sun size={15} />
                ) : (
                  <Moon size={15} />
                )}
                <span>
                  {option === "system" ? "跟随系统" : option === "light" ? "始终白天" : "始终黑夜"}
                </span>
              </button>
            ))}
          </div>
        </div>

        <div className="setting-row">
          <label>安装根目录</label>
          <div className="inline-input">
            <input value={draft.installRoot} readOnly />
            <button title="选择目录" onClick={props.onPickInstallRoot}>
              <FolderOpen size={17} />
            </button>
            <button title="打开目录" onClick={props.onOpenInstallRoot}>
              打开
            </button>
          </div>
        </div>

        <div className="setting-row">
          <label>GitHub 加速前缀</label>
          <input
            value={draft.githubProxyPrefix ?? ""}
            placeholder="默认使用远端加速列表"
            onChange={(event) =>
              props.updateDraft({ githubProxyPrefix: event.target.value || null })
            }
          />
        </div>

        <div className="setting-row">
          <label>HTTP 代理</label>
          <input
            value={draft.httpProxy ?? ""}
            placeholder="留空时读取系统代理"
            onChange={(event) => props.updateDraft({ httpProxy: event.target.value || null })}
          />
        </div>

        <div className="setting-row">
          <label className="label-with-icon">
            <span>加速源</span>
            <button
              className="mini-button icon-mini"
              title="刷新加速列表"
              onClick={props.onRefreshAccelerators}
              disabled={props.busy === "accelerators"}
            >
              {props.busy === "accelerators" ? (
                <Loader2 className="spin" size={13} />
              ) : (
                <RefreshCcw size={13} />
              )}
            </button>
          </label>
          <select
            value={draft.selectedAcceleratorId ?? ""}
            onChange={(event) =>
              props.updateDraft({ selectedAcceleratorId: event.target.value || null })
            }
          >
            {(state?.accelerators.sources ?? []).map((source) => (
              <option key={source.id} value={source.id}>
                {source.name}
              </option>
            ))}
          </select>
        </div>

        <div className="setting-row split">
          <label>加速源测速</label>
          <label className="toggle-label">
            <input
              type="checkbox"
              checked={draft.acceleratorPingEnabled}
              onChange={() => props.updateDraft({ acceleratorPingEnabled: !draft.acceleratorPingEnabled })}
            />
            <span className="toggle-slider" />
          </label>
        </div>

        {draft.acceleratorPingEnabled && (
          <div className="accelerator-ping-list">
            {(state?.accelerators.sources ?? []).map((source) => {
              const ping = props.pingResults[source.id];
              const isPinging = props.pinging[source.id];
              return (
                <div key={source.id} className="accelerator-ping-row">
                  <span className="accelerator-ping-name">{source.name}</span>
                  <span className="accelerator-ping-value">
                    {isPinging ? (
                      <span className="pinging">
                        <Loader2 size={12} className="spin" />
                        <span>正在测试…</span>
                      </span>
                    ) : ping ? (
                      ping.latencyMs != null ? (
                        `${ping.latencyMs}ms`
                      ) : ping.error ? (
                        "失败"
                      ) : (
                        ""
                      )
                    ) : (
                      ""
                    )}
                  </span>
                  <button
                    className="mini-button"
                    disabled={isPinging}
                    onClick={() => props.onPingAccelerator(source)}
                  >
                    {isPinging ? (
                      <Loader2 size={13} className="spin" />
                    ) : (
                      <Zap size={13} />
                    )}
                    <span>{isPinging ? "测试中" : "测试"}</span>
                  </button>
                </div>
              );
            })}
          </div>
        )}

        <div className="setting-row split">
          <label>调试模式</label>
          <label className="toggle-label">
            <input
              type="checkbox"
              checked={draft.debugMode}
              onChange={() => props.updateDraft({ debugMode: !draft.debugMode })}
            />
            <span className="toggle-slider" />
          </label>
        </div>

        <div className="setting-row">
          <label>启动器版本</label>
          <div className="version-info">
            <span>当前版本 {props.updateInfo?.currentVersion ?? "未知"}</span>
            {props.updateInfo?.hasUpdate && (
              <span className="update-available">新版本 {props.updateInfo.latestVersion}</span>
            )}
            {props.updateInfo && !props.updateInfo.hasUpdate && !props.updateInfo.errorMessage && (
              <span className="up-to-date">已是最新</span>
            )}
            {props.updateInfo?.errorMessage && (
              <span className="update-error" title={props.updateInfo.errorMessage}>检查失败</span>
            )}
            <button
              className="secondary-button"
              onClick={props.onCheckUpdate}
              disabled={props.busy === "checkUpdate"}
            >
              {props.busy === "checkUpdate" ? (
                <Loader2 className="spin" size={14} />
              ) : (
                <RefreshCcw size={14} />
              )}
              <span>检查更新</span>
            </button>
          </div>
        </div>

        <button className="save-button" onClick={props.onSave} disabled={props.busy === "settings"}>
          {props.busy === "settings" ? <Loader2 className="spin" size={17} /> : <Save size={17} />}
          <span>保存设置</span>
        </button>
      </div>
    </section>
  );
}

/**
 * 运行时管理独立页面
 * 将设置页中的运行时管理模块提取为主内容区视图，
 * 提供本地运行时启用/禁用、重装、删除及远端运行时查看能力。
 */
function RuntimesView(props: {
  runtimes: RuntimeInfo[];
  remoteRuntimes: RemoteRuntime[];
  runtimeConflicts: RuntimeConflict[];
  busy: string | null;
  refreshedAt: number;
  onImportRuntime: () => void;
  onScanRuntimes: () => void;
  onRefreshRuntimeCatalog: () => void;
  onToggleRuntimeEnabled: (runtime: RuntimeInfo, enabled: boolean) => void;
  onReinstallRuntime: (runtime: RuntimeInfo) => void;
  onDeleteRuntime: (runtime: RuntimeInfo) => void;
  onInstallRemoteRuntime?: (runtime: RemoteRuntime) => void;
}) {
  return (
    <section className="runtimes-layout">
      <div className="runtimes-panel glass-panel">
        <RuntimeSettings
          runtimes={props.runtimes}
          remoteRuntimes={props.remoteRuntimes}
          conflicts={props.runtimeConflicts}
          busy={props.busy}
          refreshedAt={props.refreshedAt}
          onImport={props.onImportRuntime}
          onScan={props.onScanRuntimes}
          onRefreshCatalog={props.onRefreshRuntimeCatalog}
          onToggle={props.onToggleRuntimeEnabled}
          onReinstall={props.onReinstallRuntime}
          onDelete={props.onDeleteRuntime}
          onInstall={props.onInstallRemoteRuntime}
        />
      </div>
    </section>
  );
}

function RuntimeSettings(props: {
  runtimes: RuntimeInfo[];
  remoteRuntimes: RemoteRuntime[];
  conflicts: RuntimeConflict[];
  busy: string | null;
  refreshedAt: number;
  onImport: () => void;
  onScan: () => void;
  onRefreshCatalog: () => void;
  onToggle: (runtime: RuntimeInfo, enabled: boolean) => void;
  onReinstall: (runtime: RuntimeInfo) => void;
  onDelete: (runtime: RuntimeInfo) => void;
  onInstall?: (runtime: RemoteRuntime) => void;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  const launcherRuntimes = props.runtimes.filter(
    (rt) => rt.source === "launcher",
  );
  const otherRuntimes = props.runtimes.filter(
    (rt) => rt.source !== "launcher",
  );

  function toggleSection(key: string) {
    setCollapsed((prev) => ({ ...prev, [key]: !prev[key] }));
  }

  return (
    <div className="setting-row">
      <div className="runtime-settings-head">
        <label>运行时</label>
        <div className="runtime-settings-actions">
          <button className="secondary-button" onClick={props.onImport}>
            <FileUp size={16} />
            <span>导入</span>
          </button>
          <button className="secondary-button" onClick={props.onScan}>
            <Search size={16} />
            <span>检索</span>
          </button>
          <button className="secondary-button" onClick={props.onRefreshCatalog}>
            <RefreshCcw size={16} />
            <span>刷新</span>
          </button>
          {props.refreshedAt > 0 && (
            <span className="refresh-timestamp">上次刷新 {formatRefreshTime(props.refreshedAt)}</span>
          )}
        </div>
      </div>

      {props.conflicts.length > 0 && (
        <RuntimeConflictNotice conflicts={props.conflicts} />
      )}

      <div className="runtime-sections">
        {(launcherRuntimes.length > 0 || otherRuntimes.length > 0) ? (
          <>
            <RuntimeSourceSection
              key="launcher"
              label="启动器下载"
              icon={<HardDriveDownload size={15} />}
              count={launcherRuntimes.length}
              collapsed={collapsed["launcher"] ?? false}
              onToggle={() => toggleSection("launcher")}
              emptyLabel="暂无启动器下载的运行时"
            >
              {launcherRuntimes.map((rt) => (
                <RuntimeSettingsCard
                  key={rt.id}
                  runtime={rt}
                  showReinstall={findRemoteForLocalRuntime(rt, props.remoteRuntimes) !== undefined}
                  showDelete
                  conflicted={isRuntimeInConflicts(rt, props.conflicts)}
                  busy={props.busy}
                  onToggle={(enabled) => props.onToggle(rt, enabled)}
                  onReinstall={() => props.onReinstall(rt)}
                  onDelete={() => props.onDelete(rt)}
                />
              ))}
            </RuntimeSourceSection>

            <RuntimeSourceSection
              key="other"
              label="导入 / 检索 / 系统"
              icon={<CheckCircle2 size={15} />}
              count={otherRuntimes.length}
              collapsed={collapsed["other"] ?? false}
              onToggle={() => toggleSection("other")}
              emptyLabel="暂无其他来源的运行时"
            >
              {otherRuntimes.map((rt) => (
                <RuntimeSettingsCard
                  key={rt.id}
                  runtime={rt}
                  showReinstall={false}
                  showDelete={false}
                  conflicted={isRuntimeInConflicts(rt, props.conflicts)}
                  busy={props.busy}
                  onToggle={(enabled) => props.onToggle(rt, enabled)}
                />
              ))}
            </RuntimeSourceSection>
          </>
        ) : (
          <p className="muted" style={{ marginTop: 0 }}>暂无已安装的运行时，使用上方的导入或检索来添加。</p>
        )}

        <RuntimeSourceSection
          key="remote"
          label="远端运行时 (Adoptium JRE)"
          icon={<Cloud size={15} />}
          count={props.remoteRuntimes.length}
          collapsed={collapsed["remote"] ?? false}
          onToggle={() => toggleSection("remote")}
          emptyLabel={props.remoteRuntimes.length === 0 ? "点击刷新按钮加载远端列表" : ""}
        >
          {props.remoteRuntimes.map((rt) => (
            <RuntimeCard
              key={rt.id}
              runtime={rt}
              kind="remote"
              compact
              action={props.onInstall ? (
                <button
                  className="mini-button icon-mini"
                  title="下载运行时"
                  onClick={() => props.onInstall!(rt)}
                  disabled={props.busy === `runtime:${rt.id}`}
                >
                  {props.busy === `runtime:${rt.id}` ? (
                    <Loader2 className="spin" size={13} />
                  ) : (
                    <HardDriveDownload size={13} />
                  )}
                </button>
              ) : undefined}
            />
          ))}
        </RuntimeSourceSection>
      </div>
    </div>
  );
}

function RuntimeSourceSection(props: {
  label: string;
  icon: ReactNode;
  count: number;
  collapsed: boolean;
  onToggle: () => void;
  emptyLabel: string;
  children: ReactNode;
}) {
  return (
    <div className={`runtime-source-section ${props.collapsed ? "collapsed" : ""}`}>
      <button
        className="runtime-source-header"
        onClick={props.onToggle}
        title={props.collapsed ? "展开" : "折叠"}
      >
        <span className="runtime-source-header-left">
          {props.icon}
          <strong>{props.label}</strong>
          <span className="runtime-source-count">{props.count}</span>
        </span>
        <span className={`runtime-source-chevron ${props.collapsed ? "" : "open"}`}>
          ▾
        </span>
      </button>
      {!props.collapsed && (
        <div className="runtime-source-body">
          {props.count === 0 ? (
            <span className="muted">{props.emptyLabel}</span>
          ) : (
            props.children
          )}
        </div>
      )}
    </div>
  );
}

function RuntimeSettingsCard(props: {
  runtime: RuntimeInfo;
  showReinstall: boolean;
  showDelete: boolean;
  conflicted: boolean;
  busy: string | null;
  onToggle: (enabled: boolean) => void;
  onReinstall?: () => void;
  onDelete?: () => void;
}) {
  const rt = props.runtime;
  const detail = [rt.os, rt.arch, rt.version, runtimeSourceLabel(rt)]
    .filter(Boolean)
    .join(" · ");

  return (
    <div
      className={[
        "runtime-card",
        "local",
        "settings-card",
        !rt.enabled ? "disabled" : "",
        props.conflicted ? "conflicted" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      title={rt.javaPath}
    >
      <Cpu size={15} />
      <div className="runtime-card-main">
        <strong>{formatRuntimeName(rt)}</strong>
        <span>{detail}</span>
      </div>
      {!rt.enabled && <span className="runtime-chip">已禁用</span>}
      {props.conflicted && <span className="runtime-chip warn">冲突</span>}
      <div className="runtime-card-actions">
        <button
          className="mini-button"
          onClick={() => props.onToggle(!rt.enabled)}
          disabled={props.busy === `runtimeToggle:${rt.id}`}
        >
          {rt.enabled ? <Ban size={13} /> : <CheckCircle2 size={13} />}
          <span>{rt.enabled ? "禁用" : "启用"}</span>
        </button>
        {props.showReinstall && props.onReinstall && (
          <button
            className="mini-button"
            onClick={props.onReinstall}
            disabled={props.busy?.startsWith("runtime:")}
          >
            <RefreshCcw size={13} />
            <span>重下</span>
          </button>
        )}
        {props.showDelete && props.onDelete && (
          <button
            className="mini-button danger"
            onClick={props.onDelete}
            disabled={props.busy === `runtimeDelete:${rt.id}`}
          >
            <Trash2 size={13} />
            <span>删除</span>
          </button>
        )}
      </div>
    </div>
  );
}

function ChannelBadge(props: {
  channel: GameChannel;
  sprites: Record<GameChannel, ChannelSprite>;
}) {
  const sprite = props.sprites[props.channel];
  return (
    <span className={`channel-badge ${props.channel}`}>
      <img
        src={sprite.url}
        alt=""
        onError={(e) => {
          e.currentTarget.src = sprite.fallback;
        }}
      />
      <span>{channelLabels[props.channel]}</span>
    </span>
  );
}

function ChannelStrip(props: {
  settings: Settings | null;
  busy: boolean;
  onToggle: (key: keyof ChannelVisibility, value: boolean) => void;
  sprites: Record<GameChannel, ChannelSprite>;
}) {
  if (!props.settings) {
    return null;
  }
  return (
    <div className="channel-strip">
      <div className="strip-title">
        <Layers size={16} />
        <span>版本通道</span>
      </div>
      <div className="channel-buttons">
        {channelVisibilityKeys.map((item) => {
          const checked = props.settings?.channelVisibility[item.key] ?? false;
          const active =
            checked &&
            ((item.channel !== "mindustryBE" && item.channel !== "mindustryXBE") ||
              props.settings?.showBe);
          const sprite = props.sprites[item.channel];
          return (
            <button
              key={item.key}
              className={active ? "channel-toggle active" : "channel-toggle"}
              disabled={props.busy}
              onClick={() => props.onToggle(item.key, !checked)}
            >
              <span className={`channel-dot ${item.channel}`}>
                <img
                  src={sprite.url}
                  alt=""
                  onError={(e) => {
                    e.currentTarget.src = sprite.fallback;
                  }}
                />
              </span>
              <span>{item.label}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function RuntimePanel(props: {
  runtimes: RemoteRuntime[];
  allRemoteRuntimes: RemoteRuntime[];
  installed: RuntimeInfo[];
  runtimeConflicts: RuntimeConflict[];
  pendingRuntimeConflicts: PendingRuntimeConflict[];
  busy: string | null;
  onRefresh: () => void;
  onInstall: (runtime: RemoteRuntime) => void;
  onReinstall: (runtime: RuntimeInfo) => void;
}) {
  const enabledLocal = props.installed.filter((runtime) => runtime.enabled);
  const pendingLocalIds = new Set(
    props.pendingRuntimeConflicts.flatMap((conflict) =>
      conflict.locals.map((runtime) => runtime.id),
    ),
  );
  const pendingRemoteKeys = new Set(
    props.pendingRuntimeConflicts.map((conflict) => runtimeKey(conflict.remote)),
  );
  return (
    <div className="runtime-panel">
      <div className="runtime-group">
        <div className="runtime-group-title">
          <CheckCircle2 size={15} />
          <span>本地有</span>
        </div>
        <div className="runtime-local-list">
          {enabledLocal.length === 0 ? (
            <span className="muted">暂无本地运行时</span>
          ) : (
            enabledLocal.map((runtime) => (
              <RuntimeCard
                key={runtime.id}
                kind="local"
                runtime={runtime}
                compact
                conflicted={isRuntimeInConflicts(runtime, props.runtimeConflicts)}
                pendingConflict={pendingLocalIds.has(runtime.id)}
                action={
                  canReinstallRuntime(runtime) &&
                  findRemoteForLocalRuntime(runtime, props.allRemoteRuntimes) && (
                    <button
                      className="mini-button icon-mini"
                      title="重新下载对应远端运行时"
                      onClick={() => props.onReinstall(runtime)}
                      disabled={props.busy?.startsWith("runtime:")}
                    >
                      <RefreshCcw size={13} />
                    </button>
                  )
                }
              />
            ))
          )}
        </div>
      </div>
      <div className="runtime-group">
        <div className="runtime-group-title">
          <Cloud size={15} />
          <span>远端</span>
          <button className="mini-button icon-mini" title="刷新运行时列表" onClick={props.onRefresh}>
            <RefreshCcw size={13} />
          </button>
        </div>
        <div className="runtime-local-list">
          {props.runtimes.length === 0 ? (
            <span className="muted">暂无可下载的新运行时</span>
          ) : (
            props.runtimes.map((runtime) => (
              <RuntimeCard
                key={runtime.id}
                kind="remote"
                runtime={runtime}
                compact
                pendingConflict={pendingRemoteKeys.has(runtimeKey(runtime))}
                action={
                  <button
                    className="mini-button icon-mini"
                    title="下载运行时"
                    onClick={() => props.onInstall(runtime)}
                    disabled={props.busy === `runtime:${runtime.id}`}
                  >
                    {props.busy === `runtime:${runtime.id}` ? (
                      <Loader2 className="spin" size={13} />
                    ) : (
                      <HardDriveDownload size={13} />
                    )}
                  </button>
                }
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function RuntimeCard(props: {
  runtime: RuntimeInfo | RemoteRuntime;
  kind: "local" | "remote";
  compact?: boolean;
  disabled?: boolean;
  conflicted?: boolean;
  pendingConflict?: boolean;
  action?: ReactNode;
}) {
  const runtime = props.runtime;
  const version = "version" in runtime ? runtime.version : null;
  const detail =
    props.kind === "remote"
      ? `${runtime.os}/${runtime.arch}${"sizeLabel" in runtime && runtime.sizeLabel ? ` · ${runtime.sizeLabel}` : ""}`
      : `${runtime.os}/${runtime.arch} · ${"source" in runtime ? runtimeSourceLabel(runtime) : ""}`;
  return (
    <div
      className={[
        "runtime-card",
        props.kind,
        props.compact ? "compact" : "",
        props.disabled ? "disabled" : "",
        props.conflicted ? "conflicted" : "",
        props.pendingConflict ? "pending-conflict" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      title={"javaPath" in runtime ? runtime.javaPath : runtime.fileName}
    >
      <Cpu size={props.compact ? 14 : 16} />
      <div className="runtime-card-main">
        <strong>{formatRuntimeName(runtime)}</strong>
        <span>{version ? `${version} · ${detail}` : detail}</span>
      </div>
      {"source" in runtime && runtime.source === "launcher" && (
        <span className="runtime-chip ok">启动器</span>
      )}
      {props.disabled && <span className="runtime-chip">已禁用</span>}
      {props.conflicted && <span className="runtime-chip warn">冲突</span>}
      {props.pendingConflict && <span className="runtime-chip warn">待冲突</span>}
      {props.action}
    </div>
  );
}

function RuntimeConflictNotice(props: { conflicts: RuntimeConflict[]; compact?: boolean }) {
  return (
    <div className={props.compact ? "runtime-conflict compact" : "runtime-conflict"}>
      <AlertTriangle size={15} />
      <span>
        {props.compact
          ? `${props.conflicts.length} 组运行时冲突`
          : `发现 ${props.conflicts.length} 组运行时冲突：${props.conflicts
              .map((conflict) => conflict.label)
              .join("、")}`}
      </span>
    </div>
  );
}

function PendingRuntimeConflictNotice(props: {
  conflicts: PendingRuntimeConflict[];
  compact?: boolean;
}) {
  return (
    <div className={props.compact ? "runtime-conflict pending compact" : "runtime-conflict pending"}>
      <AlertTriangle size={15} />
      <span>
        {props.compact
          ? `${props.conflicts.length} 个运行时下载将产生冲突`
          : `下载完成后将产生冲突：${props.conflicts
              .map((conflict) => conflict.label)
              .join("、")}`}
      </span>
    </div>
  );
}

function InstanceSettingsModal(props: {
  instance: InstalledInstance;
  runtimes: RuntimeInfo[];
  draft: { runtimeId: string; launchSettings: LaunchSettings };
  busy: boolean;
  onRuntimeChange: (runtimeId: string) => void;
  onLaunchSettingsChange: (patch: Partial<LaunchSettings>) => void;
  onSave: () => void;
  onClose: () => void;
  onUpgrade?: (instanceId: string) => void;
}) {
  const enabledRuntimes = props.runtimes.filter((runtime) => runtime.enabled);
  return (
    <div className="modal-layer" role="dialog" aria-modal="true">
      <div className="instance-settings-modal">
        <button className="modal-close" title="关闭" onClick={props.onClose}>
          <X size={17} strokeWidth={1.25} />
        </button>
        <div>
          <h2>{props.instance.version}</h2>
          <p>{channelLabels[props.instance.channel]}</p>
        </div>
        <div className="settings-form-grid">
          <div className="setting-row">
            <label>运行时</label>
            <select
              value={props.draft.runtimeId}
              onChange={(event) => props.onRuntimeChange(event.target.value)}
            >
              <option value="">
                {(() => {
                  const r = findAutoSelectedRuntime(enabledRuntimes, props.instance.requiredJavaVersion ?? 17);
                  return r
                    ? `自动选择 (${formatRuntimeName(r)} · ${runtimeSourceLabel(r)})`
                    : "自动选择"
                })()}
              </option>
              {enabledRuntimes.map((runtime) => (
                <option key={runtime.id} value={runtime.id}>
                  {formatRuntimeName(runtime)}
                  {runtime.version ? ` · ${runtime.version}` : ""} · {runtimeSourceLabel(runtime)}
                </option>
              ))}
            </select>
          </div>
          <div className="setting-row">
            <label>最小内存 MB</label>
            <input
              type="number"
              min={0}
              value={props.draft.launchSettings.minMemoryMb ?? ""}
              onChange={(event) =>
                props.onLaunchSettingsChange({
                  minMemoryMb: numberOrNull(event.target.value),
                })
              }
            />
          </div>
          <div className="setting-row">
            <label>最大内存 MB</label>
            <input
              type="number"
              min={0}
              value={props.draft.launchSettings.maxMemoryMb ?? ""}
              onChange={(event) =>
                props.onLaunchSettingsChange({
                  maxMemoryMb: numberOrNull(event.target.value),
                })
              }
            />
          </div>
          <div className="setting-row wide">
            <label>JVM 参数</label>
            <input
              value={props.draft.launchSettings.extraJvmArgs}
              onChange={(event) =>
                props.onLaunchSettingsChange({ extraJvmArgs: event.target.value })
              }
            />
          </div>
          <div className="setting-row wide">
            <label>游戏参数</label>
            <input
              value={props.draft.launchSettings.gameArgs}
              onChange={(event) =>
                props.onLaunchSettingsChange({ gameArgs: event.target.value })
              }
            />
          </div>
        </div>
        <div className="modal-actions">
          <button className="secondary-button" onClick={props.onClose}>
            取消
          </button>
          {props.onUpgrade && (
            <button className="secondary-button" onClick={() => props.onUpgrade!(props.instance.id)}>
              <RefreshCcw size={17} />
              <span>升降级</span>
            </button>
          )}
          <button className="save-button" onClick={props.onSave} disabled={props.busy}>
            {props.busy ? <Loader2 className="spin" size={17} /> : <Save size={17} />}
            <span>保存</span>
          </button>
        </div>
      </div>
    </div>
  );
}

function RuntimeGuide(props: {
  runtimes: RemoteRuntime[];
  selectedRuntime: RemoteRuntime | null;
  busy: string | null;
  onSelect: (id: string) => void;
  onDownload: () => void;
  onGoImport: () => void;
  onDismiss: () => void;
  onClose: () => void;
}) {
  return (
    <div className="modal-layer" role="dialog" aria-modal="true">
      <div className="runtime-guide">
        <div className="guide-visual">
          <img src={coreSprite} alt="" />
          <img src={alphaSprite} alt="" />
        </div>
        <h2>准备 Java 运行时</h2>
        <p>当前没有可用运行时，启动游戏前需要下载 JRE 或在运行时管理页导入本地 Java。</p>
        <div className="guide-runtime-select">
          <select
            value={props.selectedRuntime?.id ?? ""}
            onChange={(event) => props.onSelect(event.target.value)}
            disabled={props.runtimes.length === 0}
          >
            {props.runtimes.length === 0 ? (
              <option value="">远端列表未加载</option>
            ) : (
              props.runtimes.map((runtime) => (
                <option key={runtime.id} value={runtime.id}>
                  JRE {runtime.javaVersion} · {runtime.version} · {runtime.sizeLabel}
                </option>
              ))
            )}
          </select>
        </div>
        <div className="guide-actions">
          <button className="action-button" onClick={props.onDownload} disabled={!props.selectedRuntime}>
            {props.busy?.startsWith("runtime:") ? (
              <Loader2 className="spin" size={17} />
            ) : (
              <Download size={17} />
            )}
            <span>下载运行时</span>
          </button>
          <button className="secondary-button" onClick={props.onGoImport}>
            <FileUp size={17} />
            <span>去运行时导入</span>
          </button>
          <button className="ghost-button guide-dismiss" onClick={props.onDismiss}>
            无需理会
          </button>
        </div>
      </div>
    </div>
  );
}

function DeleteConfirmModal(props: {
  confirmation: DeleteConfirmation;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  let title: string;
  let description: string;
  let details: string[];
  if (props.confirmation.kind === "runtime") {
    const runtime = props.confirmation.runtime;
    title = `删除 ${formatRuntimeName(runtime)}`;
    description = "将移除启动器下载的运行时文件，并清空引用它的游戏运行时选择。";
    details = [
      `运行时目录：${runtime.path}`,
      `Java 路径：${runtime.javaPath}`,
      `来源：${runtimeSourceLabel(runtime)}`,
    ];
  } else {
    const instance = props.confirmation.instance;
    title = `删除 ${instance.version}`;
    description = "将移除该游戏的版本文件、实例目录和隔离数据。";
    details = [
      `版本文件：${instance.jarPath}`,
      `实例目录：${instance.installDir}`,
      `隔离数据：${instance.dataDir}`,
    ];
  }

  return (
    <div className="modal-layer" role="dialog" aria-modal="true">
      <div className="delete-confirm-modal">
        <button className="modal-close" title="关闭" onClick={props.onCancel} disabled={props.busy}>
          <X size={17} strokeWidth={1.25} />
        </button>
        <div className="delete-confirm-head">
          <span>
            <AlertTriangle size={20} />
          </span>
          <div>
            <h2>{title}</h2>
            <p>{description}</p>
          </div>
        </div>
        <ul className="delete-confirm-details">
          {details.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
        <p className="delete-confirm-warning">此操作不可恢复。请再次确认后再删除。</p>
        <div className="modal-actions">
          <button className="secondary-button" onClick={props.onCancel} disabled={props.busy}>
            取消
          </button>
          <button className="danger-button" onClick={props.onConfirm} disabled={props.busy}>
            {props.busy ? <Loader2 className="spin" size={17} /> : <Trash2 size={17} />}
            <span>确认删除</span>
          </button>
        </div>
      </div>
    </div>
  );
}

function LaunchConflictModal(props: {
  conflict: { version: string; message: string };
  onClose: () => void;
}) {
  return (
    <div className="modal-layer" role="dialog" aria-modal="true">
      <div className="delete-confirm-modal">
        <div className="delete-confirm-head">
          <span>
            <AlertTriangle size={20} />
          </span>
          <div>
            <h2>无法重复启动 {props.conflict.version}</h2>
            <p>{props.conflict.message}</p>
          </div>
        </div>
        <p className="delete-confirm-warning">
          该版本对应的游戏进程仍在运行，重复启动可能导致存档冲突或资源争用。请先关闭已运行的实例，或在任务管理器中结束对应进程。
        </p>
        <div className="modal-actions">
          <button className="save-button" onClick={props.onClose}>
            知道了
          </button>
        </div>
      </div>
    </div>
  );
}

function UpdatePromptModal(props: {
  info: LauncherUpdateInfo;
  onClose: () => void;
  onIgnore: (version: string) => void;
}) {
  return (
    <div className="modal-layer" role="dialog" aria-modal="true">
      <div className="update-prompt-modal">
        <div className="update-prompt-head">
          <span className="update-prompt-icon">
            <Download size={20} />
          </span>
          <div className="update-prompt-head-text">
            <h2>发现新版本</h2>
            <span className="update-prompt-version">{props.info.latestVersion}</span>
            <span className="update-prompt-current">当前版本 {props.info.currentVersion}</span>
          </div>
        </div>
        {props.info.releaseBody && (
          <div className="update-prompt-body">
            <span className="update-prompt-body-label">更新内容</span>
            <div className="update-prompt-body-content">{props.info.releaseBody}</div>
          </div>
        )}
        <div className="update-prompt-actions">
          <button className="ghost-button" onClick={() => props.onIgnore(props.info.latestVersion)}>
            <Ban size={15} />
            <span>忽略此版本</span>
          </button>
          <button className="ghost-button" onClick={props.onClose}>
            稍后再说
          </button>
          {props.info.releaseUrl && (
            <button
              className="action-button"
              onClick={() => openUrl(props.info.releaseUrl)}
            >
              <Download size={17} />
              <span>前往下载</span>
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function IconButton(props: {
  children: ReactNode;
  label: string;
  title: string;
  busy?: boolean;
  onClick: () => void;
}) {
  return (
    <button className="action-button" onClick={props.onClick} title={props.title} disabled={props.busy}>
      {props.busy ? <Loader2 className="spin" size={17} /> : props.children}
      <span>{props.label}</span>
    </button>
  );
}

function TaskItem({
  task,
  onPause,
  onResume,
  onCancel,
  onDismiss,
}: {
  task: TaskRecord;
  onPause: (taskId: string) => void;
  onResume: (taskId: string) => void;
  onCancel: (taskId: string) => void;
  onDismiss?: (taskId: string) => void;
}) {
  const totalBytes = task.totalBytes ?? 0;
  const hasTotal = totalBytes > 0;
  const isActive = task.status === "running" || task.status === "paused";
  const progress =
    hasTotal
      ? Math.min(100, Math.round((task.downloadedBytes / totalBytes) * 100))
      : task.status === "finished"
        ? 100
        : 0;
  const progressLabel =
    task.status === "failed"
      ? "失败"
      : task.status === "canceled"
        ? "已取消"
        : task.status === "paused"
          ? "已暂停"
          : hasTotal
            ? `${progress}%`
            : task.status === "finished"
              ? "完成"
              : task.downloadedBytes > 0
                ? formatBytes(task.downloadedBytes)
                : "获取大小";
  return (
    <div
      className={`task-item ${task.status} ${!hasTotal && task.status === "running" ? "indeterminate" : ""}`}
    >
      <div className="task-head">
        <span>{task.label}</span>
        <span>{progressLabel}</span>
      </div>
      <div className="task-progress-row">
        {(isActive || task.status === "failed") && (
          <div className="task-controls">
            {task.status === "paused" ? (
              <button title="继续下载" onClick={() => onResume(task.id)}>
                <Play size={13} />
              </button>
            ) : task.status === "running" ? (
              <button title="暂停下载" onClick={() => onPause(task.id)}>
                <Pause size={13} />
              </button>
            ) : null}
            {task.status === "running" || task.status === "paused" ? (
              <button className="danger" title="取消下载" onClick={() => onCancel(task.id)}>
                <X size={13} strokeWidth={1.25} />
              </button>
            ) : null}
            {task.status !== "running" && task.status !== "paused" && onDismiss && (
              <button className="danger" title="关闭" onClick={() => onDismiss(task.id)}>
                <X size={13} strokeWidth={1.25} />
              </button>
            )}
          </div>
        )}
        <div className="progress-track">
          <div
            style={{
              width:
                hasTotal || task.status === "finished" || task.status === "canceled"
                  ? `${progress}%`
                  : undefined,
            }}
          />
        </div>
      </div>
      <div className="task-metrics">
        <span>
          <b>包大小</b>
          {hasTotal ? formatBytes(totalBytes) : task.downloadedBytes > 0 ? "未返回" : "获取中"}
        </span>
        <span>
          <b>下载速度</b>
          {task.bytesPerSecond && task.bytesPerSecond > 0
            ? formatSpeed(task.bytesPerSecond)
            : "-"}
        </span>
        <span>
          <b>下载进度</b>
          {hasTotal
            ? `${formatBytes(task.downloadedBytes)} (${progress}%)`
            : formatBytes(task.downloadedBytes)}
        </span>
      </div>
      <small>
        {task.status === "failed"
          ? task.message ?? "失败"
          : task.status === "finished"
            ? task.message ?? "完成"
            : task.status === "canceled"
              ? task.message ?? "已取消"
              : formatTaskDetail(task)}
      </small>
    </div>
  );
}

function isChannelVisible(channel: GameChannel, settings: Settings) {
  if ((channel === "mindustryBE" || channel === "mindustryXBE") && !settings.showBe) {
    return false;
  }
  const key = channel === "mindustryBE" ? "mindustryBe" : channel === "mindustryXBE" ? "mindustryXbe" : channel;
  return settings.channelVisibility[key as keyof ChannelVisibility];
}

function filterRemoteRuntimes(remotes: RemoteRuntime[], installed: RuntimeInfo[]) {
  return remotes.filter(
    (remote) => !installed.some((local) => localMatchesRemote(local, remote)),
  );
}

function localMatchesRemote(local: RuntimeInfo, remote: RemoteRuntime) {
  if (!local.enabled || runtimeKey(local) !== runtimeKey(remote)) {
    return false;
  }
  if (local.version) {
    return normalizeRuntimeVersion(local.version) === normalizeRuntimeVersion(remote.version);
  }
  return true;
}

function findRemoteForLocalRuntime(local: RuntimeInfo, remotes: RemoteRuntime[]) {
  if (!canReinstallRuntime(local)) {
    return undefined;
  }
  return (
    remotes.find(
      (remote) =>
        runtimeKey(remote) === runtimeKey(local) &&
        local.version &&
        normalizeRuntimeVersion(remote.version) === normalizeRuntimeVersion(local.version),
    ) ?? remotes.find((remote) => runtimeKey(remote) === runtimeKey(local))
  );
}

function canReinstallRuntime(runtime: RuntimeInfo) {
  return runtime.source === "launcher";
}

function canDeleteRuntime(runtime: RuntimeInfo) {
  return runtime.source === "launcher";
}

function getConflictingLocalRuntimes(remote: RemoteRuntime, installed: RuntimeInfo[]) {
  return installed.filter((local) => local.enabled && runtimeKey(local) === runtimeKey(remote));
}

function getRuntimeConflicts(installed: RuntimeInfo[]): RuntimeConflict[] {
  const groups = new Map<string, RuntimeInfo[]>();
  for (const runtime of installed) {
    if (!runtime.enabled) {
      continue;
    }
    const key = runtimeKey(runtime);
    groups.set(key, [...(groups.get(key) ?? []), runtime]);
  }
  return Array.from(groups.entries())
    .filter(([, items]) => items.length > 1)
    .map(([key, items]) => ({
      key,
      label: formatRuntimeName(items[0]),
      items,
    }));
}

function getPendingRuntimeConflicts(
  tasks: TaskRecord[],
  busy: string | null,
  remotes: RemoteRuntime[],
  installed: RuntimeInfo[],
): PendingRuntimeConflict[] {
  const pendingRemotes = new Map<string, RemoteRuntime>();
  const busyRemote = findPendingRemoteRuntime(busy, remotes);
  if (busyRemote) {
    pendingRemotes.set(runtimeKey(busyRemote), busyRemote);
  }
  for (const task of tasks) {
    if (task.status !== "running") {
      continue;
    }
    const remote = findPendingRemoteRuntime(task.id, remotes) ?? findPendingRemoteRuntime(task.label, remotes);
    if (remote) {
      pendingRemotes.set(runtimeKey(remote), remote);
    }
  }
  return Array.from(pendingRemotes.values())
    .map((remote) => {
      const locals = installed.filter((local) => willRuntimeDownloadConflict(local, remote));
      return {
        key: runtimeKey(remote),
        label: formatRuntimeName(remote),
        remote,
        locals,
      };
    })
    .filter((conflict) => conflict.locals.length > 0);
}

function findPendingRemoteRuntime(value: string | null | undefined, remotes: RemoteRuntime[]) {
  if (!value) {
    return undefined;
  }
  const runtimeId = value.startsWith("runtime:") ? value.slice("runtime:".length) : value;
  const exact = remotes.find(
    (remote) => remote.id === runtimeId || baseRuntimeId(remote) === runtimeId,
  );
  if (exact) {
    return exact;
  }
  const labelMatch = value.match(/JRE\s+(\d+)/i);
  if (!labelMatch) {
    return undefined;
  }
  const javaVersion = Number(labelMatch[1]);
  return remotes.find((remote) => remote.javaVersion === javaVersion);
}

function willRuntimeDownloadConflict(local: RuntimeInfo, remote: RemoteRuntime) {
  if (!local.enabled || runtimeKey(local) !== runtimeKey(remote)) {
    return false;
  }
  if (local.source === "launcher" && local.id === baseRuntimeId(remote)) {
    return false;
  }
  return !localMatchesRemote(local, remote);
}

function isRuntimeInConflicts(runtime: RuntimeInfo, conflicts: RuntimeConflict[]) {
  return conflicts.some((conflict) => conflict.items.some((item) => item.id === runtime.id));
}

function runtimeKey(runtime: Pick<RuntimeInfo, "javaVersion" | "os" | "arch">) {
  return `${runtime.javaVersion}|${runtime.os}|${runtime.arch}`;
}

function baseRuntimeId(runtime: Pick<RuntimeInfo, "javaVersion" | "os" | "arch">) {
  return `jre-${runtime.javaVersion}-${runtime.os}-${runtime.arch}`;
}

function normalizeRuntimeVersion(version: string) {
  return version.trim().toLowerCase().replace("+", "_");
}

function formatRuntimeName(runtime: Pick<RuntimeInfo, "javaVersion" | "os" | "arch">) {
  return `JRE ${runtime.javaVersion}`;
}

function findAutoSelectedRuntime(runtimes: RuntimeInfo[], minJavaVersion: number) {
  return runtimes
    .filter((r) => r.enabled && r.javaVersion >= minJavaVersion)
    .sort((a, b) => a.javaVersion - b.javaVersion)[0]
}

function runtimeSourceLabel(runtime: Pick<RuntimeInfo, "source">) {
  if (runtime.source === "launcher") {
    return "启动器";
  }
  if (runtime.source === "imported") {
    return "导入";
  }
  if (runtime.source === "scanned") {
    return "检索";
  }
  if (runtime.source === "system") {
    return "系统";
  }
  return "本地";
}

function numberOrNull(value: string) {
  if (value.trim() === "") {
    return null;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? Math.round(parsed) : null;
}

function formatBytes(value?: number | null) {
  if (!value) {
    return "-";
  }
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function formatTaskBytes(task: TaskRecord) {
  if (task.totalBytes && task.totalBytes > 0) {
    return `已下载 ${formatBytes(task.downloadedBytes)} / ${formatBytes(task.totalBytes)}`;
  }
  if (task.downloadedBytes > 0) {
    return `已下载 ${formatBytes(task.downloadedBytes)}`;
  }
  return "等待响应";
}

function formatTaskDetail(task: TaskRecord) {
  const parts = [task.message ?? "下载中", formatTaskBytes(task)];
  parts.push(`速度 ${task.bytesPerSecond && task.bytesPerSecond > 0 ? formatSpeed(task.bytesPerSecond) : "-"}`);
  if (!task.totalBytes && task.downloadedBytes > 0) {
    parts.push("总大小未返回");
  }
  return parts.join(" · ");
}

function formatSpeed(bytesPerSecond: number) {
  return `${formatBytes(bytesPerSecond)}/s`;
}

function formatRefreshTime(value: number) {
  const d = new Date(value)
  const now = new Date()
  const time = d.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })
  if (d.toDateString() === now.toDateString()) return `今天 ${time}`
  const yesterday = new Date(now)
  yesterday.setDate(yesterday.getDate() - 1)
  if (d.toDateString() === yesterday.toDateString()) return `昨天 ${time}`
  return `${d.getMonth() + 1}/${d.getDate()} ${time}`
}

function formatDate(value?: string | null) {
  if (!value) {
    return "-";
  }
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(value));
}

function formatPlaytime(totalSeconds?: number | null) {
  const seconds = Math.max(0, Math.floor(totalSeconds ?? 0));
  if (seconds < 60) {
    return `${seconds} 秒`;
  }
  const minutes = Math.floor(seconds / 60);
  const remSeconds = seconds % 60;
  if (minutes < 60) {
    return remSeconds > 0 ? `${minutes} 分 ${remSeconds} 秒` : `${minutes} 分钟`;
  }
  const hours = Math.floor(minutes / 60);
  const remMinutes = minutes % 60;
  if (hours < 24) {
    return remMinutes > 0 ? `${hours} 小时 ${remMinutes} 分` : `${hours} 小时`;
  }
  const days = Math.floor(hours / 24);
  const remHours = hours % 24;
  return remHours > 0 ? `${days} 天 ${remHours} 小时` : `${days} 天`;
}

function formatDebugTime(value?: string | null) {
  if (!value) {
    return "-";
  }
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(value));
}

function toMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

// 判断升降级方向：基于当前版本与目标版本的语义化数值比较。
type VersionDirection = "upgrade" | "downgrade" | "unknown";

function extractVersionTuple(value: string): [number, number, number] | null {
  const match = value.match(/(\d+)(?:\.(\d+))?(?:\.(\d+))?/);
  if (!match) return null;
  return [
    Number(match[1] ?? 0),
    Number(match[2] ?? 0),
    Number(match[3] ?? 0),
  ];
}

function compareVersionDirection(
  current: string,
  target: string,
): VersionDirection {
  const a = extractVersionTuple(current);
  const b = extractVersionTuple(target);
  if (!a || !b) return "unknown";
  for (let i = 0; i < 3; i += 1) {
    if (b[i] !== a[i]) return b[i] > a[i] ? "upgrade" : "downgrade";
  }
  return "unknown";
}

// 将后端错误归类为用户可读的具体失败原因，确保失败提示清晰、可操作。
function describeUpgradeError(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  if (raw.includes("目标版本已安装")) {
    return "目标版本已经安装，请先在当前版本管理中卸载该版本后再试。";
  }
  if (raw.includes("游戏正在运行")) {
    return "游戏正在运行中，请先关闭游戏后再执行升降级。";
  }
  if (raw.includes("未找到待升降级的实例") || raw.includes("instance ")) {
    return "未找到对应的游戏实例，可能已被删除，请刷新列表后重试。";
  }
  if (
    raw.includes("缺少待升降级的游戏实例标识") ||
    raw.includes("目标游戏版本缺少唯一标识")
  ) {
    return "升降级请求缺少必要的版本标识，请重新选择目标版本。";
  }
  if (raw.includes("目标版本与当前版本相同")) {
    return "所选目标版本与当前版本相同，无需升降级。";
  }
  if (/网络|下载|request failed|下载已取消|Connect|timeout|timed out|resolve/i.test(raw)) {
    return `网络或下载失败：${raw}。请检查网络连接后重试。`;
  }
  if (/runtime|Java|运行时|JRE/i.test(raw)) {
    return `运行环境准备失败：${raw}，无法继续升降级。`;
  }
  return `升降级失败：${raw}`;
}
