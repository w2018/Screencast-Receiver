// 设置全局状态管理（Zustand）
// 所有设置项即时保存到 SQLite settings 表（Rust 侧 set_setting/get_setting）
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import {
  enable as autoStartEnable,
  disable as autoStartDisable,
  isEnabled as autoStartIsEnabled,
} from "@tauri-apps/plugin-autostart";
import { useToastStore } from "./toastStore";

/** 单个快捷键配置 */
export interface ShortcutConfig {
  shortcut: string;
  global: boolean;
}

/** 快捷键动作配置映射 */
export type ShortcutsConfig = Record<string, ShortcutConfig>;

/** 全部设置项的键 */
export const SETTING_KEYS = [
  "autoStart",
  "minimizeToTray",
  "pauseOnMinimize",
  "language",
  "theme",
  "defaultVolume",
  "defaultSpeed",
  "autoFitWindow",
  "hardwareDecode",
  "networkCacheSecs",
  "loopPlayback",
  "dlnaEnabled",
  "deviceName",
  "dlnaIface",
] as const;

export type SettingKey = (typeof SETTING_KEYS)[number];

interface SettingsState {
  /** 是否已从数据库加载 */
  loaded: boolean;

  // 通用设置
  autoStart: boolean;
  minimizeToTray: boolean;
  pauseOnMinimize: boolean;
  language: string;
  theme: string;

  // 播放设置
  defaultVolume: number;
  defaultSpeed: number;
  autoFitWindow: boolean;
  hardwareDecode: boolean;
  networkCacheSecs: number;
  loopPlayback: boolean;

  // 网络/DLNA 设置
  dlnaEnabled: boolean;
  deviceName: string;
  /** 用户指定投屏网卡 IP（空 = 自动选择） */
  dlnaIface: string;

  // 快捷键配置
  shortcuts: ShortcutsConfig;

  /** 从数据库加载全部设置 */
  loadSettings: () => Promise<void>;
  /** 更新并即时保存单个设置 */
  updateSetting: (key: SettingKey, value: string | number | boolean) => Promise<void>;
  /** 更新快捷键配置（立即保存） */
  updateShortcuts: (shortcuts: ShortcutsConfig) => Promise<void>;
  /** 重置快捷键为默认值 */
  resetShortcuts: () => Promise<void>;
}

/** 默认快捷键配置（对应 lib/shortcuts.ts 的 DEFAULT_SHORTCUTS） */
export const DEFAULT_SHORTCUTS: ShortcutsConfig = {
  play_pause: { shortcut: "Space", global: false },
  seek_back: { shortcut: "ArrowLeft", global: false },
  seek_forward: { shortcut: "ArrowRight", global: false },
  volume_up: { shortcut: "ArrowUp", global: false },
  volume_down: { shortcut: "ArrowDown", global: false },
  mute: { shortcut: "M", global: false },
  fullscreen: { shortcut: "F", global: false },
  mirror: { shortcut: "C", global: false },
};

const DEFAULTS = {
  autoStart: false,
  minimizeToTray: true,
  pauseOnMinimize: false,
  language: "zh-CN",
  theme: "dark",
  defaultVolume: 100,
  defaultSpeed: 1.0,
  autoFitWindow: true,
  hardwareDecode: true,
  networkCacheSecs: 60,
  loopPlayback: false,
  dlnaEnabled: true,
  deviceName: "投屏助手",
  dlnaIface: "",
};

// 数字/布尔值设置项的解析
function parseValue(key: string, raw: string): string | number | boolean {
  if (key === "language" || key === "theme" || key === "deviceName" || key === "dlnaIface")
    return raw;
  if (raw === "true" || raw === "false") return raw === "true";
  const n = Number(raw);
  if (!Number.isNaN(n) && raw.trim() !== "") return n;
  return raw;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  loaded: false,
  autoStart: DEFAULTS.autoStart,
  minimizeToTray: DEFAULTS.minimizeToTray,
  pauseOnMinimize: DEFAULTS.pauseOnMinimize,
  language: DEFAULTS.language,
  theme: DEFAULTS.theme,
  defaultVolume: DEFAULTS.defaultVolume,
  defaultSpeed: DEFAULTS.defaultSpeed,
  autoFitWindow: DEFAULTS.autoFitWindow,
  hardwareDecode: DEFAULTS.hardwareDecode,
  networkCacheSecs: DEFAULTS.networkCacheSecs,
  loopPlayback: DEFAULTS.loopPlayback,
  dlnaEnabled: DEFAULTS.dlnaEnabled,
  deviceName: DEFAULTS.deviceName,
  dlnaIface: DEFAULTS.dlnaIface,
  shortcuts: { ...DEFAULT_SHORTCUTS },

  loadSettings: async () => {
    try {
      // 读取所有设置项
      const entries = await Promise.all(
        SETTING_KEYS.map(async (key) => {
          const value = await invoke<string | null>("get_setting", { key });
          return { key, value };
        }),
      );
      const next: Record<string, string | number | boolean> = {};
      for (const { key, value } of entries) {
        if (value !== null && value !== undefined) {
          next[key] = parseValue(key, value);
        }
      }
      // 读取快捷键配置（JSON）
      let shortcuts = { ...DEFAULT_SHORTCUTS };
      try {
        const scRaw = await invoke<string | null>("get_setting", {
          key: "shortcuts",
        });
        if (scRaw) shortcuts = { ...shortcuts, ...JSON.parse(scRaw) };
      } catch {
        // 忽略
      }

      // 读取开机自启实际状态（以系统为准）
      let autoStart = false;
      try {
        autoStart = await autoStartIsEnabled();
      } catch {
        autoStart = Boolean(next.autoStart);
      }

      set((s) => ({
        loaded: true,
        autoStart,
        minimizeToTray:
          (next.minimizeToTray as boolean | undefined) ?? s.minimizeToTray,
        pauseOnMinimize:
          (next.pauseOnMinimize as boolean | undefined) ?? s.pauseOnMinimize,
        language: (next.language as string | undefined) ?? s.language,
        theme: (next.theme as string | undefined) ?? s.theme,
        defaultVolume:
          (next.defaultVolume as number | undefined) ?? s.defaultVolume,
        defaultSpeed: (next.defaultSpeed as number | undefined) ?? s.defaultSpeed,
        autoFitWindow:
          (next.autoFitWindow as boolean | undefined) ?? s.autoFitWindow,
        hardwareDecode:
          (next.hardwareDecode as boolean | undefined) ?? s.hardwareDecode,
        networkCacheSecs:
          (next.networkCacheSecs as number | undefined) ?? s.networkCacheSecs,
        loopPlayback:
          (next.loopPlayback as boolean | undefined) ?? s.loopPlayback,
        dlnaEnabled: (next.dlnaEnabled as boolean | undefined) ?? s.dlnaEnabled,
        deviceName: (next.deviceName as string | undefined) ?? s.deviceName,
        shortcuts,
      }));
    } catch (e) {
      console.error("加载设置失败", e);
      set({ loaded: true });
    }
  },

  updateSetting: async (key, value) => {
    // 即时更新 store
    set({ [key]: value } as Partial<SettingsState>);

    // 特殊处理：开机自启（写系统注册表）
    if (key === "autoStart") {
      try {
        if (value === true) await autoStartEnable();
        else await autoStartDisable();
      } catch (e) {
        console.error("设置开机自启失败", e);
      }
    }

    // 持久化到 SQLite + 提示
    try {
      await invoke("set_setting", { key, value: String(value) });
      useToastStore.getState().show("设置已保存");
    } catch (e) {
      console.error("保存设置失败", e);
      useToastStore.getState().show("设置保存失败");
    }
  },

  updateShortcuts: async (shortcuts) => {
    set({ shortcuts });
    try {
      await invoke("set_setting", { key: "shortcuts", value: JSON.stringify(shortcuts) });
    } catch (e) {
      console.error("保存快捷键失败", e);
    }
  },

  resetShortcuts: async () => {
    const defs = { ...DEFAULT_SHORTCUTS };
    set({ shortcuts: defs });
    try {
      await invoke("set_setting", { key: "shortcuts", value: JSON.stringify(defs) });
    } catch (e) {
      console.error("重置快捷键失败", e);
    }
  },
}));

/** 获取设置默认值 */
export function getSettingDefault(key: SettingKey): string | number | boolean {
  return DEFAULTS[key];
}
