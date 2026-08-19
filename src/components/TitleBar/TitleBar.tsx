// 自定义标题栏（F10）：无系统边框时提供拖拽区 + 窗口控制按钮
import { useState } from "react";
import { Minus, Square, X, MonitorPlay, Pin } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "../../stores/settingsStore";
import { usePlayerStore } from "../../stores/playerStore";
import { useToastStore } from "../../stores/toastStore";

interface TitleBarProps {
  /** 是否可见（3 秒无操作自动隐藏） */
  visible?: boolean;
}

export function TitleBar({ visible = true }: TitleBarProps) {
  const appWindow = getCurrentWindow();
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);

  // 窗口置顶/取消置顶
  const toggleAlwaysOnTop = async () => {
    const next = !alwaysOnTop;
    try {
      await appWindow.setAlwaysOnTop(next);
      setAlwaysOnTop(next);
    } catch (e) {
      useToastStore.getState().show(`置顶切换失败: ${String(e)}`);
      console.error("置顶切换失败:", e);
    }
  };

  // 关闭：开启"关闭到托盘"时仅隐藏窗口（不触发 CloseRequested，
  // 插件不会销毁 mpv 实例，恢复时零重载直接续显）；否则正常退出
  const handleClose = async () => {
    if (useSettingsStore.getState().minimizeToTray) {
      await appWindow.hide();
      if (useSettingsStore.getState().pauseOnMinimize) {
        const ps = usePlayerStore.getState();
        if (ps.status === "playing") {
          ps.togglePlayPause();
        }
      }
    } else {
      await appWindow.close();
    }
  };

  return (
    <div
      className={`titlebar ${visible ? "" : "hidden"}`}
      data-tauri-drag-region
    >
      <div className="titlebar-left" data-tauri-drag-region>
        <MonitorPlay size={16} className="titlebar-logo" />
        <span className="titlebar-title" data-tauri-drag-region>
          投屏助手
        </span>
      </div>
      <div className="titlebar-btns">
        {/* 窗口置顶 */}
        <button
          className={`tb-btn ${alwaysOnTop ? "active" : ""}`}
          title={alwaysOnTop ? "取消置顶" : "窗口置顶"}
          onClick={toggleAlwaysOnTop}
        >
          <Pin size={14} />
        </button>
        <button
          className="tb-btn"
          title="最小化"
          onClick={() => appWindow.minimize()}
        >
          <Minus size={14} />
        </button>
        <button
          className="tb-btn"
          title="最大化/还原"
          onClick={() => appWindow.toggleMaximize()}
        >
          <Square size={12} />
        </button>
        <button className="tb-btn tb-close" title="关闭" onClick={handleClose}>
          <X size={15} />
        </button>
      </div>
    </div>
  );
}
