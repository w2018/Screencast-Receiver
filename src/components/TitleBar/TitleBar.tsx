// 自定义标题栏（F10）：无系统边框时提供拖拽区 + 窗口控制按钮
import { Minus, Square, X, MonitorPlay } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface TitleBarProps {
  /** 是否可见（3 秒无操作自动隐藏） */
  visible?: boolean;
}

export function TitleBar({ visible = true }: TitleBarProps) {
  const appWindow = getCurrentWindow();

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
        <button
          className="tb-btn tb-close"
          title="关闭"
          onClick={() => appWindow.close()}
        >
          <X size={15} />
        </button>
      </div>
    </div>
  );
}
