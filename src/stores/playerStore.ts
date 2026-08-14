// 播放器全局状态管理（Zustand）
// 状态来源：MPV 属性事件 + 用户操作
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import {
  loadFile,
  togglePause,
  stop,
  seekAbsolute,
  seekRelative,
  setVolume,
  setSpeed,
  toggleMute,
  getDuration,
  getTimePos,
} from "../lib/mpv";

// 播放进度信息（对应 Rust 侧 ProgressInfo）
interface ProgressInfo {
  url: string;
  title: string | null;
  position: number;
  duration: number | null;
  updatedAt: number;
  completed: boolean;
}

export type PlaybackStatus = "idle" | "loading" | "playing" | "paused" | "error";

interface PlayerState {
  /** 播放状态 */
  status: PlaybackStatus;
  /** 当前播放位置（秒） */
  position: number;
  /** 视频总时长（秒） */
  duration: number;
  /** 音量（0-100） */
  volume: number;
  /** 是否静音 */
  muted: boolean;
  /** 倍速 */
  speed: number;
  /** 当前文件名/URL */
  filename: string | null;
  /** 当前 URL（持久保留，不受 MPV filename 属性被清空影响，用于重连） */
  currentUrl: string | null;
  /** 视频原始宽度 */
  videoWidth: number | null;
  /** 视频原始高度 */
  videoHeight: number | null;
  /** 是否缓冲中 */
  buffering: boolean;
  /** 错误信息 */
  error: string | null;
  /** 是否加载中 */
  loading: boolean;

  // 动作
  setStatus: (status: PlaybackStatus) => void;
  setPosition: (pos: number) => void;
  setDuration: (dur: number) => void;
  setPaused: (paused: boolean) => void;
  setVolume: (vol: number) => void;
  setMuted: (muted: boolean) => void;
  setSpeed: (spd: number) => void;
  setFilename: (name: string | null) => void;
  setVideoSize: (w: number | null, h: number | null) => void;
  setBuffering: (b: boolean) => void;
  setError: (e: string | null) => void;
  setLoading: (l: boolean) => void;

  /** 加载并播放媒体（文件或 URL） */
  loadAndPlay: (url: string) => Promise<void>;
  /** 切换播放/暂停 */
  togglePlayPause: () => Promise<void>;
  /** 停止播放 */
  stop: () => Promise<void>;
  /** 跳转到指定秒 */
  seekTo: (seconds: number) => Promise<void>;
  /** 相对跳转 */
  seekBy: (delta: number) => Promise<void>;
  /** 修改音量并同步到 MPV */
  changeVolume: (vol: number) => Promise<void>;
  /** 切换静音 */
  toggleMute: () => Promise<void>;
  /** 修改倍速并同步到 MPV */
  changeSpeed: (spd: number) => Promise<void>;
  /** 从 MPV 拉取当前进度（用于定时保存/恢复） */
  refreshProgress: () => Promise<void>;
  /** 保存当前播放进度到数据库 */
  saveProgress: () => Promise<void>;
  /** 清除指定 URL 的播放进度（播放完成后调用） */
  clearProgress: (url: string) => Promise<void>;
}

export const usePlayerStore = create<PlayerState>((set, get) => ({
  status: "idle",
  position: 0,
  duration: 0,
  volume: 100,
  muted: false,
  speed: 1.0,
  filename: null,
  currentUrl: null,
  videoWidth: null,
  videoHeight: null,
  buffering: false,
  error: null,
  loading: false,

  setStatus: (status) => set({ status }),
  setPosition: (pos) => set({ position: pos }),
  setDuration: (dur) => set({ duration: dur }),
  setPaused: (paused) =>
    set({ status: paused ? "paused" : "playing", buffering: false }),
  setVolume: (vol) => set({ volume: vol }),
  setMuted: (muted) => set({ muted }),
  setSpeed: (spd) => set({ speed: spd }),
  // 只更新 filename；currentUrl 由 loadAndPlay / dlna-session 单独维护（保持完整 URL）
  setFilename: (filename) => set({ filename }),
  setVideoSize: (w, h) =>
    set((s) => ({
      videoWidth: typeof w === "number" && w > 0 ? w : s.videoWidth,
      videoHeight: typeof h === "number" && h > 0 ? h : s.videoHeight,
    })),
  setBuffering: (buffering) => set({ buffering }),
  setError: (error) => set({ error, status: error ? "error" : get().status }),
  setLoading: (loading) => set({ loading }),

  loadAndPlay: async (url) => {
    // 查询上次播放进度，提示是否恢复（F7 进度记忆）
    let resumePos = 0;
    try {
      const prog = await invoke<ProgressInfo | null>("get_progress", { url });
      if (prog && prog.position > 5 && !prog.completed) {
        const m = Math.floor(prog.position / 60);
        const s = Math.floor(prog.position % 60);
        const timeStr = `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
        if (
          window.confirm(
            `上次播放到 ${timeStr}，是否从上次位置继续播放？`,
          )
        ) {
          resumePos = prog.position;
        }
      }
    } catch {
      // 查询失败静默处理
    }

    set({
      loading: true,
      error: null,
      status: "loading",
      filename: url,
      currentUrl: url,
      position: 0,
    });
    try {
      await loadFile(url);
      if (resumePos > 0) {
        await seekAbsolute(resumePos);
        set({ position: resumePos });
      }
      set({ loading: false });
    } catch (e) {
      set({ loading: false, error: String(e), status: "error" });
    }
  },

  togglePlayPause: async () => {
    try {
      await togglePause();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  stop: async () => {
    try {
      await stop();
      set({
        status: "idle",
        position: 0,
        duration: 0,
        filename: null,
        error: null,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  seekTo: async (seconds) => {
    try {
      await seekAbsolute(seconds);
      set({ position: seconds });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  seekBy: async (delta) => {
    try {
      await seekRelative(delta);
      const cur = get().position;
      set({ position: Math.max(0, cur + delta) });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  changeVolume: async (vol) => {
    const clamped = Math.max(0, Math.min(100, vol));
    try {
      await setVolume(clamped);
      set({ volume: clamped, muted: clamped === 0 ? true : get().muted });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  toggleMute: async () => {
    try {
      await toggleMute();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  changeSpeed: async (spd) => {
    try {
      await setSpeed(spd);
      set({ speed: spd });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  refreshProgress: async () => {
    try {
      const [pos, dur] = await Promise.all([getTimePos(), getDuration()]);
      if (pos !== null) set({ position: pos });
      if (dur !== null) set({ duration: dur });
    } catch {
      // 静默失败（实例未就绪时忽略）
    }
  },

  saveProgress: async () => {
    const { filename, position, duration, status } = get();
    if (!filename || status === "idle" || status === "error") return;
    try {
      await invoke("save_progress", {
        url: filename,
        title: null,
        position,
        duration: duration > 0 ? duration : null,
      });
    } catch {
      // 静默失败
    }
  },

  clearProgress: async (url) => {
    try {
      await invoke("clear_progress", { url });
    } catch {
      // 静默失败
    }
  },
}));
