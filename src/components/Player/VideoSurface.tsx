// 视频挂载区域
// 说明：MPV 通过 wid 嵌入到整个窗口，此 div 作为前端交互覆盖层。
// 视频画面由 MPV 直接绘制，此区域负责拦截鼠标事件（双击全屏等）。
import { useRef } from "react";
import { usePlayerStore } from "../../stores/playerStore";

interface VideoSurfaceProps {
  /** 双击回调（用于全屏切换） */
  onDoubleClick?: () => void;
  /** 鼠标移动回调（用于显示/隐藏控制栏） */
  onMouseMove?: () => void;
  /** 错误重试回调（F11 网络异常） */
  onRetry?: () => void;
  children?: React.ReactNode;
}

export function VideoSurface({
  onDoubleClick,
  onMouseMove,
  onRetry,
  children,
}: VideoSurfaceProps) {
  const ref = useRef<HTMLDivElement>(null);
  const status = usePlayerStore((s) => s.status);
  const buffering = usePlayerStore((s) => s.buffering);
  const loading = usePlayerStore((s) => s.loading);
  const error = usePlayerStore((s) => s.error);

  return (
    <div
      ref={ref}
      className="video-surface"
      onDoubleClick={(e) => {
        // 双击非控制栏区域才触发全屏
        if ((e.target as HTMLElement).closest(".control-bar")) return;
        onDoubleClick?.();
      }}
      onMouseMove={() => {
        onMouseMove?.();
      }}
    >
      {/* 加载动画 */}
      {(loading || buffering) && (
        <div className="video-overlay">
          <div className="spinner" />
          <span>{buffering ? "缓冲中…" : "加载中…"}</span>
        </div>
      )}

      {/* 错误提示 */}
      {error && (
        <div className="video-overlay error-overlay">
          <span className="error-text">⚠ {error}</span>
          {onRetry && (
            <button className="retry-btn" onClick={onRetry}>
              重试
            </button>
          )}
        </div>
      )}

      {/* 空闲提示 */}
      {status === "idle" && !loading && (
        <div className="video-overlay">
          <span className="idle-text">投屏助手 —— 等待接收播放</span>
        </div>
      )}

      {/* 控制栏等子内容 */}
      {children}
    </div>
  );
}
