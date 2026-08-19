// 视频挂载区域
// 说明：MPV 通过 wid 嵌入到整个窗口，此 div 作为前端交互覆盖层。
// 视频画面由 MPV 直接绘制，此区域负责拦截鼠标事件（双击全屏等）。
import { useRef } from "react";
import { RotateCcw } from "lucide-react";
import { usePlayerStore } from "../../stores/playerStore";
import { seekAbsolute, play } from "../../lib/mpv";

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
  // 单击延迟 250ms 等双击：单击切换播放/暂停，双击全屏互不干扰
  const clickTimerRef = useRef<number | null>(null);
  const status = usePlayerStore((s) => s.status);
  const buffering = usePlayerStore((s) => s.buffering);
  const loading = usePlayerStore((s) => s.loading);
  const error = usePlayerStore((s) => s.error);
  const filename = usePlayerStore((s) => s.filename);
  // 播放结束（eof 后 keep-open 停在结尾）→ 从头重播
  const replay = async () => {
    try {
      await seekAbsolute(0);
      await play();
      usePlayerStore.setState({ status: "playing", position: 0, error: null });
    } catch (e) {
      usePlayerStore.setState({ error: `重新播放失败: ${String(e)}` });
    }
  };
  // 单击：切换播放/暂停（控制栏、覆盖层按钮区域不触发）
  const handleClick = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest(".control-bar, .video-overlay")) return;
    if (clickTimerRef.current !== null) window.clearTimeout(clickTimerRef.current);
    clickTimerRef.current = window.setTimeout(() => {
      clickTimerRef.current = null;
      const ps = usePlayerStore.getState();
      if (ps.status === "playing" || ps.status === "paused") ps.togglePlayPause();
    }, 250);
  };
  const handleDoubleClick = (e: React.MouseEvent) => {
    if (clickTimerRef.current !== null) {
      window.clearTimeout(clickTimerRef.current);
      clickTimerRef.current = null;
    }
    // 双击非控制栏区域才触发全屏
    if ((e.target as HTMLElement).closest(".control-bar")) return;
    onDoubleClick?.();
  };

  return (
    <div
      ref={ref}
      className="video-surface"
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
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

      {/* 播放结束提示（有内容且已播完） */}
      {!error && status === "idle" && !!filename && (
        <div className="video-overlay ended-overlay">
          <span className="ended-text">已结束，是否重新播放？</span>
          <button className="replay-btn" onClick={replay}>
            <RotateCcw size={20} />
            重新播放
          </button>
        </div>
      )}

      {/* 空闲提示 */}
      {status === "idle" && !loading && !filename && (
        <div className="video-overlay">
          <span className="idle-text">投屏助手 —— 等待接收播放</span>
        </div>
      )}

      {/* 控制栏等子内容 */}
      {children}
    </div>
  );
}
