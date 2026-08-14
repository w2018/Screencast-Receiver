// 投屏助手 —— 主应用组件
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  destroy,
  listenEvents,
  type MpvEvent,
} from "tauri-plugin-libmpv-api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  initMpv,
  setVideoFilter,
  loadFile,
  setVolume as mpvSetVolume,
  setSpeed as mpvSetSpeed,
  setLoop as mpvSetLoop,
} from "./lib/mpv";
import {
  handleWindowKey,
  registerGlobalShortcut,
  unregisterAllGlobals,
  type ActionHandler,
  type ShortcutAction,
} from "./lib/shortcuts";
import { usePlayerStore } from "./stores/playerStore";
import { useSettingsStore } from "./stores/settingsStore";
import { SettingsPage } from "./components/Settings/SettingsPage";
import { VideoSurface } from "./components/Player/VideoSurface";
import { ControlBar } from "./components/Player/ControlBar";
import { TitleBar } from "./components/TitleBar/TitleBar";
import { Toast } from "./components/Toast/Toast";
import { ConnectionRequest, type CastRequest } from "./components/Dialog/ConnectionRequest";
import "./App.css";

// 把关键状态转发到 Rust stdout（tauri dev 终端可见，用于调试）
const flog = (msg: string) => {
  invoke("log_frontend", { msg }).catch(() => {});
};

function App() {
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [mirrorH, setMirrorH] = useState(false);
  const [mirrorV, setMirrorV] = useState(false);
  const [controlsVisible, setControlsVisible] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const [pendingCast, setPendingCast] = useState<CastRequest | null>(null);
  const hideTimer = useRef<number | null>(null);
  // 网络错误重连计数（F11）
  const retryRef = useRef<{ count: number; url: string }>({ count: 0, url: "" });

  // Zustand store 动作
  const setPaused = usePlayerStore((s) => s.setPaused);
  const setPosition = usePlayerStore((s) => s.setPosition);
  const setDuration = usePlayerStore((s) => s.setDuration);
  const setVolume = usePlayerStore((s) => s.setVolume);
  const setMuted = usePlayerStore((s) => s.setMuted);
  const setSpeed = usePlayerStore((s) => s.setSpeed);
  const setFilename = usePlayerStore((s) => s.setFilename);
  const setVideoSize = usePlayerStore((s) => s.setVideoSize);
  const setBuffering = usePlayerStore((s) => s.setBuffering);
  const setError = usePlayerStore((s) => s.setError);
  const setStatus = usePlayerStore((s) => s.setStatus);
  const loadAndPlay = usePlayerStore((s) => s.loadAndPlay);

  const appWindow = getCurrentWindow();

  // ===== 窗口自适应视频分辨率（F3）=====
  const videoSizeRef = useRef<{ w: number; h: number } | null>(null);
  const fittedRef = useRef<string>("");

  /** 将窗口调整为视频原始分辨率（每个新视频只调整一次，且过小视频不调整） */
  const fitWindowToVideo = useCallback(() => {
    if (!useSettingsStore.getState().autoFitWindow) return; // 设置关闭则不调整
    const size = videoSizeRef.current;
    if (!size || size.w < 640 || size.h < 360) return; // 过小视频不调整窗口
    const key = `${size.w}x${size.h}`;
    if (fittedRef.current === key) return; // 同一视频只调整一次
    fittedRef.current = key;
    appWindow
      .setSize(new LogicalSize(size.w, size.h))
      .then(() => flog(`[F3] 窗口自适应: ${key}`))
      .catch((e) => flog(`[F3] 窗口自适应失败: ${String(e)}`));
  }, [appWindow]);

  // ===== 镜像滤镜构建 =====
  const buildFilter = useCallback((h: boolean, v: boolean) => {
    const filters: string[] = [];
    if (h) filters.push("hflip");
    if (v) filters.push("vflip");
    return filters.length > 0 ? `lavfi=[${filters.join(",")}]` : null;
  }, []);

  // ===== MPV 初始化 + 事件监听 =====
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenTray: (() => void) | undefined;
    let unlistenTrayHidden: (() => void) | undefined;
    let unlistenDlna: (() => void) | undefined;
    let saveTimer: number | undefined;
    let disposed = false;

    // 重新初始化 MPV 并恢复播放（从托盘恢复窗口时调用）
    const reinitAndPlay = async () => {
      try {
        await initMpv(); // 幂等：实例已存在则跳过
        flog("MPV 已重新初始化（托盘恢复）");
        const st = usePlayerStore.getState();
        // 恢复之前播放的视频
        if (st.filename && st.status !== "playing") {
          await loadAndPlay(st.filename);
        }
      } catch (e) {
        setError(`MPV 重新初始化失败: ${String(e)}`);
        flog(`MPV reinit 失败: ${String(e)}`);
      }
    };

    (async () => {
      try {
        // 先加载设置，再按设置初始化 MPV
        await useSettingsStore.getState().loadSettings();
        const st = useSettingsStore.getState();
        const label = await initMpv({
          hwdec: st.hardwareDecode ? "d3d11va" : "no",
          "cache-secs": String(st.networkCacheSecs),
        });
        if (disposed) return;
        flog(
          `[F8] 设置已加载: hwdec=${st.hardwareDecode}, cache=${st.networkCacheSecs}s, tray=${st.minimizeToTray}, volume=${st.defaultVolume}, speed=${st.defaultSpeed}`,
        );
        flog(`MPV init 成功 (${label})`);

        unlisten = await listenEvents((event: MpvEvent) => {
          switch (event.event) {
            case "start-file":
              setStatus("loading");
              break;
            case "file-loaded":
              setStatus("playing");
              // 应用播放设置（默认音量/倍速/循环）
              {
                const ps = useSettingsStore.getState();
                mpvSetVolume(ps.defaultVolume).catch(() => {});
                mpvSetSpeed(ps.defaultSpeed).catch(() => {});
                mpvSetLoop(ps.loopPlayback).catch(() => {});
              }
              flog("file-loaded -> 播放成功");
              break;
            case "end-file":
              if (event.reason === "error") {
                // 网络/解码错误：自动重连（最多 3 次，间隔 2s/4s/8s）
                const errUrl =
                  usePlayerStore.getState().currentUrl ||
                  usePlayerStore.getState().filename ||
                  retryRef.current.url;
                if (errUrl && retryRef.current.url === errUrl) {
                  retryRef.current.count += 1;
                } else {
                  retryRef.current = { count: 1, url: errUrl ?? "" };
                }
                const cnt = retryRef.current.count;
                flog(`end-file error=${event.error}, 重连 ${cnt}/3`);
                if (cnt <= 3 && errUrl) {
                  const delay = [2000, 4000, 8000][cnt - 1] ?? 8000;
                  setError(`网络异常，正在自动重连 (${cnt}/3)…`);
                  setTimeout(() => {
                    if (disposed) return;
                    flog(`[F11] 自动重连 (${cnt}/3): ${errUrl}`);
                    loadFile(errUrl).catch((e) =>
                      flog(`[F11] 重连失败: ${String(e)}`),
                    );
                  }, delay);
                } else {
                  setError("网络异常，请检查网络连接后重试");
                  flog("[F11] 重连次数已用尽");
                  retryRef.current = { count: 0, url: "" };
                }
              } else if (event.reason === "eof") {
                setStatus("idle");
                setPosition(0);
                // 播放完成，清除进度记忆（F7）
                const doneUrl = usePlayerStore.getState().filename;
                if (doneUrl) usePlayerStore.getState().clearProgress(doneUrl);
                flog("[F7] 播放完成，进度已清除");
              }
              break;
            case "video-reconfig":
              flog("video-reconfig");
              break;
            case "property-change":
              handlePropertyChange(event);
              break;
            case "log-message":
              // 完整打印 MPV 日志到终端（诊断 vo/hwdec 渲染问题）
              if (event.level === "error" || event.level === "warn") {
                setError(event.text);
                flog(`[mpv-${event.level}] ${event.text}`);
              } else if (event.level === "info" || event.level === "v") {
                flog(`[mpv-${event.level}] ${event.text}`);
              }
              break;
          }
        });

        // 每 5 秒自动保存播放进度（F7）
        saveTimer = window.setInterval(() => {
          usePlayerStore.getState().saveProgress();
        }, 5000);
      } catch (e) {
        setError(`MPV 初始化失败: ${String(e)}`);
        flog(`MPV init 失败: ${String(e)}`);
      }
    })();

    // 监听托盘"显示主窗口"事件 → 重新初始化 MPV
    listen("tray-show", () => {
      flog("收到托盘显示事件");
      reinitAndPlay();
    }).then((un) => {
      unlistenTray = un;
    });

    // DLNA 投屏请求：弹出允许/拒绝确认框（窗口在托盘时也会被唤醒）
    listen("dlna-request", async (e) => {
      const p = e.payload as {
        uri: string;
        clientIp: string;
        userAgent: string;
      };
      flog(`[DLNA] 投屏请求: ${p.uri} from ${p.clientIp}`);
      // 窗口可能从托盘恢复（隐藏时 MPV 实例可能被销毁），确保 MPV 已初始化
      try {
        const st = useSettingsStore.getState();
        await initMpv({
          hwdec: st.hardwareDecode ? "d3d11va" : "no",
          "cache-secs": String(st.networkCacheSecs),
        });
      } catch (e) {
        flog(`[DLNA] MPV 预初始化失败: ${String(e)}`);
      }
      setPendingCast({
        uri: String(p.uri ?? ""),
        clientIp: String(p.clientIp ?? "未知"),
        userAgent: String(p.userAgent ?? ""),
      });
    }).then((un) => {
      unlistenDlna = un;
    });

    // 窗口隐藏到托盘：按设置暂停播放
    listen("app-hidden", () => {
      if (useSettingsStore.getState().pauseOnMinimize) {
        const ps = usePlayerStore.getState();
        if (ps.status === "playing") {
          ps.togglePlayPause();
          flog("[F8] 已按设置暂停播放");
        }
      }
    }).then((un) => {
      unlistenTrayHidden = un;
    });

    return () => {
      disposed = true;
      if (saveTimer) window.clearInterval(saveTimer);
      unlisten?.();
      unlistenTray?.();
      unlistenTrayHidden?.();
      unlistenDlna?.();
      destroy().catch(() => {});
    };
  }, []);

  // ===== 属性变化处理 =====
  const handlePropertyChange = useCallback((event: Extract<MpvEvent, { event: "property-change" }>) => {
    const { name, data } = event;
    switch (name) {
      case "pause":
        setPaused(Boolean(data));
        break;
      case "time-pos": {
        const v = Number(data);
        if (Number.isFinite(v)) setPosition(v);
        break;
      }
      case "duration": {
        const v = Number(data);
        if (Number.isFinite(v)) setDuration(v);
        break;
      }
      case "volume": {
        const v = Number(data);
        if (Number.isFinite(v)) setVolume(v);
        break;
      }
      case "mute":
        setMuted(Boolean(data));
        break;
      case "speed": {
        const v = Number(data);
        if (Number.isFinite(v) && v > 0) setSpeed(v);
        break;
      }
      case "width": {
        const v = Number(data);
        if (Number.isFinite(v) && v > 0) {
          setVideoSize(v, null);
          videoSizeRef.current = { w: v, h: videoSizeRef.current?.h ?? 0 };
          fitWindowToVideo();
        }
        break;
      }
      case "height": {
        const v = Number(data);
        if (Number.isFinite(v) && v > 0) {
          setVideoSize(null, v);
          videoSizeRef.current = { w: videoSizeRef.current?.w ?? 0, h: v };
          fitWindowToVideo();
        }
        break;
      }
      case "filename":
        setFilename(typeof data === "string" ? data : null);
        break;
      case "cache-buffering-state": {
        const v = Number(data);
        setBuffering(v === 1);
        break;
      }
    }
  }, [fitWindowToVideo]);

  // ===== 全屏切换 =====
  const toggleFullscreen = useCallback(async () => {
    const fs = await appWindow.isFullscreen();
    await appWindow.setFullscreen(!fs);
    setIsFullscreen(!fs);
  }, [appWindow]);

  // ===== 控制栏自动隐藏（全屏时）=====
  const handleMouseMove = useCallback(() => {
    setControlsVisible(true);
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => {
      if (usePlayerStore.getState().status !== "idle") {
        setControlsVisible(false);
      }
    }, 3000);
  }, []);

  // ===== 镜像切换 =====
  const toggleMirrorH = useCallback(() => {
    setMirrorH((prev) => {
      const next = !prev;
      setVideoFilter(buildFilter(next, mirrorV)).catch(() => {});
      return next;
    });
  }, [mirrorV, buildFilter]);

  const toggleMirrorV = useCallback(() => {
    setMirrorV((prev) => {
      const next = !prev;
      setVideoFilter(buildFilter(mirrorH, next)).catch(() => {});
      return next;
    });
  }, [mirrorH, buildFilter]);

  // ===== 快捷键动作映射（F6）=====
  const actionHandlers = useMemo<ActionHandler>(
    () => ({
      play_pause: () => usePlayerStore.getState().togglePlayPause(),
      seek_back: () => usePlayerStore.getState().seekBy(-5),
      seek_forward: () => usePlayerStore.getState().seekBy(5),
      volume_up: () =>
        usePlayerStore.getState().changeVolume(usePlayerStore.getState().volume + 5),
      volume_down: () =>
        usePlayerStore.getState().changeVolume(usePlayerStore.getState().volume - 5),
      mute: () => usePlayerStore.getState().toggleMute(),
      fullscreen: () => toggleFullscreen(),
      mirror: () => toggleMirrorH(),
    }),
    [toggleFullscreen, toggleMirrorH],
  );

  // ===== 窗口内快捷键（默认配置驱动）=====
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Esc 退出全屏（独立处理）
      if (e.key === "Escape" && isFullscreen) {
        appWindow.setFullscreen(false);
        setIsFullscreen(false);
        return;
      }
      const action = handleWindowKey(e);
      if (action) actionHandlers[action]();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isFullscreen, appWindow, actionHandlers]);

  // ===== 主题应用（F10：深色/浅色）=====
  const theme = useSettingsStore((s) => s.theme);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  // ===== 全局快捷键同步（F6/F8：按设置注册全局快捷键）=====
  const shortcuts = useSettingsStore((s) => s.shortcuts);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      await unregisterAllGlobals();
      for (const [action, cfg] of Object.entries(shortcuts)) {
        if (cancelled) return;
        if (cfg.global) {
          const ok = await registerGlobalShortcut(
            cfg.shortcut,
            action as ShortcutAction,
            () => {
              actionHandlers[action as keyof ActionHandler]?.();
            },
          );
          flog(`[F6] 全局快捷键 ${cfg.shortcut}(${action}): ${ok ? "已注册" : "冲突"}`);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [shortcuts, actionHandlers]);

  return (
    <div className="app-shell">
      <TitleBar visible={controlsVisible} />
      <VideoSurface
        onDoubleClick={toggleFullscreen}
        onMouseMove={handleMouseMove}
        onRetry={() => {
          // 手动重试：重置计数并重新加载
          const url = usePlayerStore.getState().filename;
          retryRef.current = { count: 0, url: "" };
          setError(null);
          if (url) {
            flog("[F11] 手动重试");
            loadFile(url).catch((e) => flog(`[F11] 重试失败: ${String(e)}`));
          }
        }}
      >
        {controlsVisible && (
          <ControlBar
            isFullscreen={isFullscreen}
            onToggleFullscreen={toggleFullscreen}
            onOpenSettings={() => setShowSettings(true)}
            onToggleMirrorH={toggleMirrorH}
            onToggleMirrorV={toggleMirrorV}
            mirrorH={mirrorH}
            mirrorV={mirrorV}
          />
        )}
      </VideoSurface>

      {/* 设置面板 */}
      {showSettings && <SettingsPage onClose={() => setShowSettings(false)} />}

      {/* 投屏请求确认弹窗 */}
      {pendingCast && (
        <ConnectionRequest
          request={pendingCast}
          onClose={() => setPendingCast(null)}
        />
      )}

      {/* 全局通知 */}
      <Toast />
    </div>
  );
}

export default App;
