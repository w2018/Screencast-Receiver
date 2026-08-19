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
  getVideoFilter,
  loadFile,
  seekAbsolute,
  play,
  getTimePos,
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
  // 待恢复的播放位置（托盘恢复时 set，file-loaded 后执行 seek）
  const pendingSeekRef = useRef(0);
  // 隐藏到托盘瞬间保存的位置（实例销毁/事件归零前抓取）
  const lastPosRef = useRef(0);

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
  // 使用 mpv 原生滤镜（hflip/vflip），比 lavfi 包裹语法更可靠
  const buildFilter = useCallback((h: boolean, v: boolean) => {
    const filters: string[] = [];
    if (h) filters.push("hflip");
    if (v) filters.push("vflip");
    return filters.length > 0 ? filters.join(",") : null;
  }, []);

  // 应用镜像滤镜并读回验证（诊断）
  const applyMirror = useCallback(
    (h: boolean, v: boolean) => {
      const filter = buildFilter(h, v);
      setVideoFilter(filter)
        .then(async () => {
          try {
            const readback = await getVideoFilter();
            flog(
              `[MIRROR] 已设置滤镜 ${filter ?? "无"}，mpv 实际值: ${readback ?? "空"}`,
            );
          } catch (e) {
            flog(`[MIRROR] 读回 vf 失败: ${String(e)}`);
          }
        })
        .catch((e) => flog(`[MIRROR] 设置滤镜失败: ${String(e)}`));
    },
    [buildFilter],
  );

  // ===== MPV 初始化 + 事件监听 =====
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenTray: (() => void) | undefined;
    let unlistenTrayHidden: (() => void) | undefined;
    let unlistenTrayResized: (() => void) | undefined;
    let unlistenDlna: (() => void) | undefined;
    let saveTimer: number | undefined;
    let disposed = false;

    // 重新初始化 MPV 并恢复播放（从托盘恢复窗口时调用）
    // 实例隐藏时会被销毁，恢复时需重建实例 + 防盗链头 + 续播原位置
    const reinitAndPlay = async () => {
      try {
        await initMpv(); // 幂等：实例已存在则跳过
        flog("MPV 已重新初始化（托盘恢复）");
        const st = usePlayerStore.getState();
        const url = st.currentUrl || st.filename;
        if (!url) return;
        usePlayerStore.setState({ allowAutoRetry: true });
        // 恢复防盗链请求头（B 站等 CDN 校验 Referer，缺少会 403）
        try {
          await invoke("setup_stream_headers_command", { uri: url });
        } catch (e) {
          flog(`恢复请求头失败: ${String(e)}`);
        }
        // 静默续播：优先用隐藏瞬间保存的位置（实例销毁/事件归零后 store 可能已丢失），
        // 位置存入 pendingSeek，待 file-loaded 后 seek（立即 seek 会因文件未加载而无效）
        const resume = lastPosRef.current > 5 ? lastPosRef.current : st.position > 5 ? st.position : 0;
        pendingSeekRef.current = resume;
        setStatus("loading");
        await loadFile(url);
        flog(`已恢复播放: ${url} @ ${resume}s`);
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
              // 清除加载/缓冲标志（刷新/恢复后可能残留，导致转圈不消失）
              setBuffering(false);
              usePlayerStore.setState({ loading: false });
              // 托盘恢复续播：mpv 就绪后 seek 到暂停前位置
              if (pendingSeekRef.current > 0) {
                const target = pendingSeekRef.current;
                pendingSeekRef.current = 0;
                seekAbsolute(target)
                  .then(() => {
                    setPosition(target);
                    flog(`[F8] 已续播到 ${target}s`);
                  })
                  .catch((e) => flog(`[F8] 续播 seek 失败: ${String(e)}`));
              }
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
                // 刷新等手动操作时禁用自动重连，避免长时间转圈（直接提示）
                if (!usePlayerStore.getState().allowAutoRetry) {
                  setError("加载失败，视频源可能已失效，请停止后重新投屏");
                  setBuffering(false);
                  retryRef.current = { count: 0, url: "" };
                  flog("[F11] 手动刷新模式，不自动重连");
                  break;
                }
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
                const loop = useSettingsStore.getState().loopPlayback;
                if (loop) {
                  // 循环播放：seek(0)+play 重播。
                  // 说明：INITIAL_OPTIONS 的 keep-open=yes 与 loop-file=inf 冲突（keep-open 优先），
                  // 故不用 loop-file，改由事件驱动重播。
                  setPosition(0);
                  seekAbsolute(0)
                    .then(() => play())
                    .catch((e) => flog(`循环重播失败: ${String(e)}`));
                  flog("[F8] 循环播放: 已从头重播");
                } else {
                  setStatus("idle");
                  setPosition(0);
                  // 播放完成，清除进度记忆（F7）
                  const doneUrl = usePlayerStore.getState().filename;
                  if (doneUrl) usePlayerStore.getState().clearProgress(doneUrl);
                  flog("[F7] 播放完成，进度已清除");
                }
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

    // 监听托盘"显示主窗口"事件
    listen("tray-show", async () => {
      flog("收到托盘显示事件");
      const st = usePlayerStore.getState();
      const url = st.currentUrl || st.filename;
      if (!url) {
        flog("[F8] 托盘恢复: 无播放内容");
        return;
      }
      // 标题栏关闭（关闭到托盘）只隐藏窗口、不销毁 mpv 实例：
      // 实例存活时直接恢复显示，无需重载（画面/进度/暂停状态原样回来）
      try {
        const pos = await getTimePos();
        if (pos !== null && Number.isFinite(pos)) {
          flog(`[F8] 托盘恢复: 实例存活(time-pos=${pos}s)，无需重载`);
          return;
        }
        flog("[F8] 托盘恢复: time-pos 无效，走重载续播");
      } catch {
        flog("[F8] 托盘恢复: 实例已销毁，重载续播");
      }
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
    listen("app-hidden", (e) => {
      // Rust 端在隐藏前查询的真值；无效/0 时不覆盖（避免实例销毁后位置归零污染）
      const payloadPos =
        typeof e.payload === "number" && Number.isFinite(e.payload) ? e.payload : 0;
      if (payloadPos > 0) {
        lastPosRef.current = payloadPos;
        flog(`[F8] 隐藏托盘, 已保存位置 ${payloadPos}s`);
      } else {
        const sp = usePlayerStore.getState().position;
        if (sp > 0) lastPosRef.current = sp;
        flog(`[F8] 隐藏托盘, 保存位置 ${sp}s`);
      }
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

    // 最小化时若开启"最小化到托盘时暂停播放"：隐藏到托盘并暂停
    const unlistenResized = appWindow.onResized(async () => {
      if (!useSettingsStore.getState().pauseOnMinimize) return;
      const minimized = await appWindow.isMinimized();
      if (!minimized) return;
      // 隐藏前保存位置（实例销毁可能触发事件归零）
      lastPosRef.current = usePlayerStore.getState().position;
      flog(`[F8] 最小化, 已保存位置 ${lastPosRef.current}s`);
      await appWindow.hide();
      flog("[F8] 最小化 → 已隐藏到托盘");
      const ps = usePlayerStore.getState();
      if (ps.status === "playing") {
        ps.togglePlayPause();
        flog("[F8] 已按设置暂停播放");
      }
    });
    unlistenResized.then((un) => {
      unlistenTrayResized = un;
    });

    return () => {
      disposed = true;
      if (saveTimer) window.clearInterval(saveTimer);
      unlisten?.();
      unlistenTray?.();
      unlistenTrayHidden?.();
      unlistenTrayResized?.();
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
      case "filename": {
        const name = typeof data === "string" ? data : null;
        // 加载了新文件：重置"隐藏前保存的位置"（新视频从头计）
        if (name !== usePlayerStore.getState().filename) lastPosRef.current = 0;
        setFilename(name);
        break;
      }
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
      applyMirror(next, mirrorV);
      return next;
    });
  }, [mirrorV, applyMirror]);

  const toggleMirrorV = useCallback(() => {
    setMirrorV((prev) => {
      const next = !prev;
      applyMirror(mirrorH, next);
      return next;
    });
  }, [mirrorH, applyMirror]);

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
          // 手动重试：重置计数并重新加载（允许自动重连）
          const url = usePlayerStore.getState().filename;
          retryRef.current = { count: 0, url: "" };
          usePlayerStore.setState({ allowAutoRetry: true });
          setError(null);
          setBuffering(false);
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
            onStop={() => usePlayerStore.getState().stop()}
            mirrorH={mirrorH}
            mirrorV={mirrorV}
          />
        )}
      </VideoSurface>

      {/* 设置面板（点击面板外区域或按 Esc 关闭） */}
      {showSettings && (
        <div
          className="settings-overlay"
          onClick={() => setShowSettings(false)}
        >
          <SettingsPage onClose={() => setShowSettings(false)} />
        </div>
      )}

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
