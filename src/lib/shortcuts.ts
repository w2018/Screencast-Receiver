// 快捷键管理模块（F6）
// 支持：窗口内快捷键（keydown）+ 全局快捷键（tauri-plugin-global-shortcut）
// 窗口快捷键始终生效；全局快捷键由用户按动作启用（设置面板接入后配置）
import {
  register,
  unregister,
  unregisterAll,
  isRegistered,
} from "@tauri-apps/plugin-global-shortcut";

/** 快捷键动作类型 */
export type ShortcutAction =
  | "play_pause"
  | "seek_back"
  | "seek_forward"
  | "volume_up"
  | "volume_down"
  | "mute"
  | "fullscreen"
  | "mirror";

/** 默认窗口内快捷键（文档 F6 规定） */
export const DEFAULT_SHORTCUTS: Record<ShortcutAction, string> = {
  play_pause: "Space", // 播放/暂停
  seek_back: "ArrowLeft", // 后退 5 秒
  seek_forward: "ArrowRight", // 前进 5 秒
  volume_up: "ArrowUp", // 音量 +5
  volume_down: "ArrowDown", // 音量 -5
  mute: "M", // 静音
  fullscreen: "F", // 全屏
  mirror: "C", // 镜像翻转
};

/** 动作执行器集合 */
export type ActionHandler = { [K in ShortcutAction]: () => void };

/** 已注册的全局快捷键（shortcut → action） */
const globalShortcutMap = new Map<string, ShortcutAction>();

/**
 * 注册全局快捷键（应用不在前台时也生效）
 * @returns true=注册成功；false=被其他应用占用或注册失败
 */
export async function registerGlobalShortcut(
  shortcut: string,
  action: ShortcutAction,
  handler: () => void,
): Promise<boolean> {
  try {
    if (await isRegistered(shortcut)) return false; // 已被占用
    await register(shortcut, (event) => {
      if (event.state === "Pressed") handler();
    });
    globalShortcutMap.set(shortcut, action);
    return true;
  } catch {
    return false; // 解析失败等
  }
}

/** 注销全局快捷键 */
export async function unregisterGlobalShortcut(shortcut: string): Promise<void> {
  try {
    await unregister(shortcut);
  } catch {
    // 忽略未注册的
  }
  globalShortcutMap.delete(shortcut);
}

/** 获取当前已注册的全部全局快捷键 */
export function getRegisteredGlobalShortcuts(): string[] {
  return [...globalShortcutMap.keys()];
}

/** 注销全部全局快捷键 */
export async function unregisterAllGlobals(): Promise<void> {
  try {
    await unregisterAll();
  } catch {
    // 忽略
  }
  globalShortcutMap.clear();
}

/**
 * 处理窗口内键盘事件
 * @returns 命中的动作，未命中返回 null
 */
export function handleWindowKey(e: KeyboardEvent): ShortcutAction | null {
  const key = e.code === "Space" ? "Space" : e.key;
  for (const [action, shortcut] of Object.entries(DEFAULT_SHORTCUTS) as [
    ShortcutAction,
    string,
  ][]) {
    if (shortcut === key) {
      e.preventDefault();
      return action;
    }
  }
  return null;
}
