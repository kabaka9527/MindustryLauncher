import { Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AlertTriangle,
  Ban,
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
} from "lucide-react";
import type { ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  cancelDownload,
  clearDebugLog,
  deleteInstance,
  deleteRuntime,
  getAppState,
  importRuntime,
  installVersion,
  installRuntime,
  launchVersion,
  listRemoteRuntimes,
  openDebugLogDir,
  migrateInstallRoot,
  openInstallRoot,
  pauseDownload,
  readDebugLog,
  refreshAccelerators,
  refreshVersions,
  resumeDownload,
  scanRuntimes,
  saveInstanceLaunchSettings,
  saveSettings,
  setRuntimeEnabled,
  switchVersion,
} from "./api";
import type {
  AppUiState,
  ChannelVisibility,
  DebugLogSnapshot,
  GameChannel,
  InstalledInstance,
  LaunchSettings,
  RemoteRuntime,
  RemoteVersion,
  RuntimeInfo,
  Settings,
  TaskEvent,
  TaskRecord,
  Theme,
} from "./types";
import alphaSprite from "./assets/mindustry/alpha.png";
import basaltSprite from "./assets/mindustry/basalt1.png";
import coreSprite from "./assets/mindustry/core-shard.png";
import lancerSprite from "./assets/mindustry/lancer.png";
import zenithSprite from "./assets/mindustry/zenith.png";
import { useTheme } from "./hooks/useTheme";

type View = "games" | "versions" | "settings";

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

export default function App() {
  if (getCurrentWindow().label === "debug-log") {
    return <DebugLogWindow />;
  }

  const { theme, setTheme, isDark } = useTheme();

  const [view, setView] = useState<View>("games");
  const [state, setState] = useState<AppUiState | null>(null);
  const [draft, setDraft] = useState<Settings | null>(null);
  const [tasks, setTasks] = useState<Record<string, TaskRecord>>({});
  const [busy, setBusy] = useState<string | null>("load");
  const [notice, setNotice] = useState<string>("正在读取本地状态");
  const [startupAcceleratorsRefreshDone, setStartupAcceleratorsRefreshDone] = useState(false);
  const [startupRefreshDone, setStartupRefreshDone] = useState(false);
  const [remoteRuntimes, setRemoteRuntimes] = useState<RemoteRuntime[]>([]);
  const [selectedRuntimeId, setSelectedRuntimeId] = useState("");
  const [runtimeCatalogLoaded, setRuntimeCatalogLoaded] = useState(false);
  const [runtimeGuideOpen, setRuntimeGuideOpen] = useState(false);
  const [editingInstanceId, setEditingInstanceId] = useState<string | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState<DeleteConfirmation | null>(null);
  const [toastMessage, setToastMessage] = useState<ToastMessage | null>(null);
  const toastTimerRef = useRef<number | null>(null);
  const [instanceSettingsDraft, setInstanceSettingsDraft] = useState<{
    runtimeId: string;
    launchSettings: LaunchSettings;
  } | null>(null);

  const reload = useCallback(async () => {
    const next = await getAppState();
    setState(next);
    setDraft(next.settings);
    setNotice("状态已同步");
    return next;
  }, []);

  const loadRuntimeCatalog = useCallback(async (force = false) => {
    if (runtimeCatalogLoaded && !force) {
      return;
    }
    setBusy((current) => current ?? "runtimeCatalog");
    try {
      const runtimes = await listRemoteRuntimes();
      setRemoteRuntimes(runtimes);
      setSelectedRuntimeId((current) => {
        if (current && runtimes.some((runtime) => runtime.id === current)) {
          return current;
        }
        return (
          runtimes.find((runtime) => runtime.javaVersion === 17)?.id ??
          runtimes[0]?.id ??
          ""
        );
      });
      setRuntimeCatalogLoaded(true);
    } catch (error) {
      setNotice(toMessage(error));
    } finally {
      setBusy((current) => (current === "runtimeCatalog" ? null : current));
    }
  }, [runtimeCatalogLoaded]);

  useEffect(() => {
    reload()
      .catch((error) => setNotice(toMessage(error)))
      .finally(() => setBusy(null));
  }, [reload]);

  useEffect(() => {
    return () => {
      if (toastTimerRef.current !== null) {
        window.clearTimeout(toastTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (state && !runtimeCatalogLoaded) {
      void loadRuntimeCatalog();
    }
  }, [loadRuntimeCatalog, runtimeCatalogLoaded, state]);

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
          : "管理安装目录、加速源与运行时环境",
    );
  }, [view]);

  useEffect(() => {
    if (
      state &&
      state.instances.length > 0 &&
      state.runtimes.length === 0 &&
      !state.settings.runtimePromptDismissed
    ) {
      setRuntimeGuideOpen(true);
    }
  }, [state]);

  useEffect(() => {
    if (view !== "versions" || !state || startupRefreshDone || state.versions.length > 0) {
      return;
    }
    setStartupRefreshDone(true);
    setBusy("startupRefresh");
    setNotice("正在获取默认版本列表");
    refreshAccelerators()
      .then((accelerators) => {
        setState((current) => (current ? { ...current, accelerators } : current));
        return refreshVersions();
      })
      .then((versions) => {
        setState((current) => (current ? { ...current, versions } : current));
        setNotice("默认版本列表已加载");
      })
      .catch((error) => setNotice(toMessage(error)))
      .finally(() => setBusy(null));
  }, [startupRefreshDone, state, view]);

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
            bytesPerSecond: undefined,
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
      }, 3200);
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

  async function onRefreshVersions() {
    await runWithBusy("versions", async () => {
      const accelerators = await refreshAccelerators();
      const versions = await refreshVersions();
      setState((current) => (current ? { ...current, accelerators, versions } : current));
    }, "版本列表已刷新");
  }

  async function onRefreshAccelerators() {
    await runWithBusy("accelerators", async () => {
      const accelerators = await refreshAccelerators();
      setState((current) => (current ? { ...current, accelerators } : current));
    }, "GitHub 加速列表已刷新");
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
    const previousDebugMode = state?.settings.debugMode;
    await runWithBusy("settings", async () => {
      const saved = await applySettings(nextDraft, false);
      if (previousDebugMode !== undefined && previousDebugMode !== saved.debugMode) {
        setNotice(
          saved.debugMode
            ? "调试模式已保存，重启启动器后会打开独立日志窗口"
            : "调试模式已关闭，重启启动器后停止打开日志窗口",
        );
      } else {
        setNotice("设置已保存");
      }
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
      const result = await launchVersion(instance.id);
      setNotice(`已启动 PID ${result.pid}`);
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
    view === "games" ? "游戏" : view === "versions" ? "版本列表" : "启动器设置";
  const pageNotice =
    view === "games"
      ? installedInstances.length > 0
        ? `已安装 ${installedInstances.length} 个游戏版本`
        : "暂无已安装游戏，打开版本列表选择安装"
      : notice;

  return (
    <div className="app-shell">
      <div className="animated-backdrop" aria-hidden="true">
        <img className="drift drift-a" src={coreSprite} alt="" />
        <img className="drift drift-b" src={zenithSprite} alt="" />
        <img className="drift drift-c" src={lancerSprite} alt="" />
        <img className="drift drift-d" src={alphaSprite} alt="" />
        <img className="tile-texture" src={basaltSprite} alt="" />
      </div>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark"><img src={coreSprite} alt="" /></div>
          <div>
            <strong>Mindustry</strong>
            <span>Launcher</span>
          </div>
        </div>
        <button
          className={view === "games" ? "nav-button active" : "nav-button"}
          onClick={() => {
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
            setView("versions");
            setNotice("选择版本进行切换");
          }}
          title="版本列表"
        >
          <Layers size={18} />
          <span>版本列表</span>
        </button>
        <button
          className={view === "settings" ? "nav-button active" : "nav-button"}
          onClick={() => {
            setView("settings");
            setNotice("管理安装目录、加速源与运行时环境");
          }}
          title="设置"
        >
          <SettingsIcon size={18} />
          <span>设置</span>
        </button>
        <div className="sidebar-footer">
          <span>{state?.instances.length ?? 0} 个实例</span>
          <span>{state?.runtimes.length ?? 0} 个运行时</span>
        </div>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <h1>{pageTitle}</h1>
            <p>{pageNotice}</p>
          </div>
          <div className="top-actions">
            <span className="theme-indicator" title={theme === "system" ? "主题：跟随系统" : theme === "dark" ? "主题：黑夜模式" : "主题：白天模式"}>
              {isDark ? <Moon size={13} /> : <Sun size={13} />}
            </span>
            {view === "games" && (
              <IconButton
                title="打开版本列表"
                label="版本列表"
                onClick={() => setView("versions")}
              >
                <Layers size={17} />
              </IconButton>
            )}
            {view === "versions" && (
              <IconButton
                title="刷新版本列表"
                label="刷新"
                busy={busy === "versions" || busy === "startupRefresh"}
                onClick={onRefreshVersions}
              >
                <RefreshCcw size={17} />
              </IconButton>
            )}
          </div>
        </header>

        {view === "games" ? (
          <section className="content-grid">
            <div className="version-list">
              {installedInstances.length === 0 ? (
                <div className="empty-state game-empty">
                  <div className="empty-visual">
                    <Play size={34} />
                  </div>
                  <div className="empty-copy">
                    <strong>暂无已安装游戏</strong>
                    <span>打开版本列表，选择一个版本切换到本地。</span>
                  </div>
                  <button onClick={() => setView("versions")}>
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
                        <span className={`channel-badge ${instance.channel}`}>
                          {channelLabels[instance.channel]}
                        </span>
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

            <aside className="side-panel">
              <section className="panel-section">
                <div className="section-title">
                  <span>任务</span>
                  <span>{taskList.length}</span>
                </div>
                {taskList.length === 0 ? (
                  <p className="muted">暂无任务</p>
                ) : (
                  taskList.map((task) => (
                    <TaskItem
                      key={task.id}
                      task={task}
                      onPause={onPauseTask}
                      onResume={onResumeTask}
                      onCancel={onCancelTask}
                    />
                  ))
                )}
              </section>
              <section className="panel-section">
                <div className="section-title">
                  <span>运行时</span>
                  <span>{(state?.runtimes ?? []).filter((runtime) => runtime.enabled).length}</span>
                </div>
                {runtimeConflicts.length > 0 && (
                  <RuntimeConflictNotice conflicts={runtimeConflicts} compact />
                )}
                {pendingRuntimeConflicts.length > 0 && (
                  <PendingRuntimeConflictNotice conflicts={pendingRuntimeConflicts} compact />
                )}
                <RuntimePanel
                  runtimes={remoteRuntimeOptions}
                  allRemoteRuntimes={remoteRuntimes}
                  installed={state?.runtimes ?? []}
                  runtimeConflicts={runtimeConflicts}
                  pendingRuntimeConflicts={pendingRuntimeConflicts}
                  busy={busy}
                  onRefresh={() => loadRuntimeCatalog(true)}
                  onInstall={installRemoteRuntime}
                  onReinstall={onReinstallRuntime}
                />
              </section>
            </aside>
          </section>
        ) : view === "versions" ? (
          <section className="version-browser">
            <div className="version-list version-list-full">
              <ChannelStrip
                settings={draft ?? state?.settings ?? null}
                busy={busy === "channelFilter"}
                onToggle={onToggleVersionChannel}
              />
              {visibleVersions.length === 0 ? (
                <div className="empty-state">
                  <RefreshCcw size={28} />
                  <span>暂无版本数据</span>
                  <button onClick={onRefreshVersions}>刷新版本</button>
                </div>
              ) : (
                visibleVersions.map((version) => {
                  const instance = instancesById.get(version.id);
                  return (
                    <article className="version-row" key={version.id}>
                      <div className="version-main">
                        <span className={`channel-badge ${version.channel}`}>
                          {channelLabels[version.channel]}
                        </span>
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
                        {instance ? (
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
            onRefreshRuntimeCatalog={() => loadRuntimeCatalog(true)}
            onRefreshAccelerators={onRefreshAccelerators}
            onToggleRuntimeEnabled={onToggleRuntimeEnabled}
            onReinstallRuntime={onReinstallRuntime}
            onDeleteRuntime={onDeleteRuntime}
            remoteRuntimes={remoteRuntimes}
            runtimeConflicts={runtimeConflicts}
            onPickInstallRoot={onPickInstallRoot}
            onOpenInstallRoot={() => {
              void runWithBusy("openRoot", openInstallRoot);
            }}
          />
        )}
        {runtimeGuideOpen && state && (
          <RuntimeGuide
            runtimes={remoteRuntimeOptions}
            selectedRuntime={selectedRemoteRuntime}
            busy={busy}
            onSelect={setSelectedRuntimeId}
            onDownload={() => onInstallSelectedRuntime()}
            onGoImport={() => {
              setRuntimeGuideOpen(false);
              setView("settings");
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
      </main>
      {toastMessage && (
        <div className="toast-layer" role="status" aria-live="polite">
          <div className="toast-message" key={toastMessage.id}>
            <Loader2 className="spin" size={17} />
            <span>{toastMessage.text}</span>
          </div>
        </div>
      )}
    </div>
  );
}

function DebugLogWindow() {
  const [snapshot, setSnapshot] = useState<DebugLogSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [busy, setBusy] = useState<"clear" | "dir" | null>(null);
  const logRef = useRef<HTMLPreElement | null>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await readDebugLog();
      setSnapshot(next);
      setError(null);
    } catch (error) {
      setError(toMessage(error));
    }
  }, []);

  useEffect(() => {
    document.body.classList.add("debug-log-body");
    let alive = true;
    async function refreshIfAlive() {
      try {
        const next = await readDebugLog();
        if (alive) {
          setSnapshot(next);
          setError(null);
        }
      } catch (error) {
        if (alive) {
          setError(toMessage(error));
        }
      }
    }
    void refreshIfAlive();
    const timer = window.setInterval(refreshIfAlive, 900);
    return () => {
      alive = false;
      window.clearInterval(timer);
      document.body.classList.remove("debug-log-body");
    };
  }, []);

  useEffect(() => {
    if (autoScroll && logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [autoScroll, snapshot?.content, error]);

  async function onClearLog() {
    const confirmed = window.confirm("清空当前调试日志？旧归档文件不会删除。");
    if (!confirmed) {
      return;
    }
    setBusy("clear");
    try {
      const next = await clearDebugLog();
      setSnapshot(next);
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
    : snapshot?.content.trim()
      ? snapshot.content
      : "暂无日志";

  return (
    <main className="debug-log-window">
      <section className="debug-log-panel">
        <div className="debug-log-titlebar">
          <div>
            <h1>调试控制台</h1>
            <span>{snapshot?.logPath || "等待日志路径"}</span>
          </div>
          <div className="debug-log-actions">
            <button onClick={refresh} title="刷新日志">
              <RefreshCcw size={15} />
              <span>刷新</span>
            </button>
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
          <span>
            <b>状态</b>
            {snapshot?.enabled ? "已启用" : "未启用"}
          </span>
          <span>
            <b>会话</b>
            {snapshot?.sessionId ?? "-"}
          </span>
          <span>
            <b>启动时间</b>
            {formatDebugTime(snapshot?.startedAt)}
          </span>
          <span>
            <b>行数</b>
            {snapshot
              ? snapshot.truncated
                ? `${snapshot.lineCount}，显示最近 ${snapshot.maxLines}`
                : `${snapshot.lineCount}`
              : "-"}
          </span>
        </div>

        <pre ref={logRef} className={error ? "debug-log-content error" : "debug-log-content"}>
          {content}
        </pre>
      </section>
    </main>
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
  remoteRuntimes: RemoteRuntime[];
  runtimeConflicts: RuntimeConflict[];
  onPickInstallRoot: () => void;
  onOpenInstallRoot: () => void;
}) {
  const { state, draft } = props;
  if (!draft) {
    return <div className="empty-state">正在加载</div>;
  }

  return (
    <section className="settings-layout">
      <div className="settings-panel">
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

        <RuntimeSettings
            runtimes={state?.runtimes ?? []}
            remoteRuntimes={props.remoteRuntimes}
            conflicts={props.runtimeConflicts}
            busy={props.busy}
            onImport={props.onImportRuntime}
            onScan={props.onScanRuntimes}
            onRefreshCatalog={props.onRefreshRuntimeCatalog}
            onToggle={props.onToggleRuntimeEnabled}
            onReinstall={props.onReinstallRuntime}
            onDelete={props.onDeleteRuntime}
          />

        <button className="save-button" onClick={props.onSave} disabled={props.busy === "settings"}>
          {props.busy === "settings" ? <Loader2 className="spin" size={17} /> : <Save size={17} />}
          <span>保存设置</span>
        </button>
      </div>
    </section>
  );
}

function RuntimeSettings(props: {
  runtimes: RuntimeInfo[];
  remoteRuntimes: RemoteRuntime[];
  conflicts: RuntimeConflict[];
  busy: string | null;
  onImport: () => void;
  onScan: () => void;
  onRefreshCatalog: () => void;
  onToggle: (runtime: RuntimeInfo, enabled: boolean) => void;
  onReinstall: (runtime: RuntimeInfo) => void;
  onDelete: (runtime: RuntimeInfo) => void;
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
        <label>运行时管理</label>
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
        </div>
      </div>

      {props.conflicts.length > 0 && (
        <RuntimeConflictNotice conflicts={props.conflicts} />
      )}

      {launcherRuntimes.length === 0 && otherRuntimes.length === 0 ? (
        <p className="muted">暂无已安装的运行时，使用上方的导入或检索来添加。</p>
      ) : (
        <div className="runtime-sections">
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
        </div>
      )}
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

function ChannelStrip(props: {
  settings: Settings | null;
  busy: boolean;
  onToggle: (key: keyof ChannelVisibility, value: boolean) => void;
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
          return (
            <button
              key={item.key}
              className={active ? "channel-toggle active" : "channel-toggle"}
              disabled={props.busy}
              onClick={() => props.onToggle(item.key, !checked)}
            >
              <span className={`channel-dot ${item.channel}`} />
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
}) {
  const enabledRuntimes = props.runtimes.filter((runtime) => runtime.enabled);
  return (
    <div className="modal-layer" role="dialog" aria-modal="true">
      <div className="instance-settings-modal">
        <button className="modal-close" title="关闭" onClick={props.onClose}>
          <X size={17} />
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
              <option value="">自动选择</option>
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
        <button className="modal-close" title="关闭" onClick={props.onClose}>
          <X size={17} />
        </button>
        <div className="guide-visual">
          <img src={coreSprite} alt="" />
          <img src={alphaSprite} alt="" />
        </div>
        <h2>准备 Java 运行时</h2>
        <p>当前没有可用运行时，启动游戏前需要下载 JRE 或在设置页导入本地 Java。</p>
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
            <span>去设置导入</span>
          </button>
          <button className="ghost-button" onClick={props.onDismiss}>
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
          <X size={17} />
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
}: {
  task: TaskRecord;
  onPause: (taskId: string) => void;
  onResume: (taskId: string) => void;
  onCancel: (taskId: string) => void;
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
        {isActive && (
          <div className="task-controls">
            {task.status === "paused" ? (
              <button title="继续下载" onClick={() => onResume(task.id)}>
                <Play size={13} />
              </button>
            ) : (
              <button title="暂停下载" onClick={() => onPause(task.id)}>
                <Pause size={13} />
              </button>
            )}
            <button className="danger" title="取消下载" onClick={() => onCancel(task.id)}>
              <X size={13} />
            </button>
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
  if (task.bytesPerSecond && task.bytesPerSecond > 0) {
    parts.push(`速度 ${formatSpeed(task.bytesPerSecond)}`);
  }
  if (!task.totalBytes && task.downloadedBytes > 0) {
    parts.push("总大小未返回");
  }
  return parts.join(" · ");
}

function formatSpeed(bytesPerSecond: number) {
  return `${formatBytes(bytesPerSecond)}/s`;
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
