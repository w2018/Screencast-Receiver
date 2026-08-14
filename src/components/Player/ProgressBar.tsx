// 播放进度条：显示进度 + 点击/拖拽跳转
import { useCallback, useRef, useState } from "react";

interface ProgressBarProps {
  /** 当前进度（秒） */
  position: number;
  /** 总时长（秒） */
  duration: number;
  /** 跳转回调 */
  onSeek: (seconds: number) => void;
}

export function ProgressBar({ position, duration, onSeek }: ProgressBarProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);
  const [preview, setPreview] = useState<number | null>(null);

  const total = duration > 0 ? duration : 1;
  const ratio = Math.min(1, Math.max(0, position / total));

  const getSecondsFromClientX = useCallback(
    (clientX: number) => {
      const el = ref.current;
      if (!el) return 0;
      const rect = el.getBoundingClientRect();
      const r = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
      return r * total;
    },
    [total],
  );

  const handlePointerDown = (e: React.PointerEvent) => {
    e.preventDefault();
    setDragging(true);
    const sec = getSecondsFromClientX(e.clientX);
    setPreview(sec);
    onSeek(sec);
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!dragging) return;
    const sec = getSecondsFromClientX(e.clientX);
    setPreview(sec);
    onSeek(sec);
  };

  const handlePointerUp = () => {
    setDragging(false);
    setPreview(null);
  };

  const fmt = (s: number) => {
    if (!Number.isFinite(s) || s < 0) return "00:00";
    const m = Math.floor(s / 60);
    const sec = Math.floor(s % 60);
    return `${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  };

  return (
    <div className="progress-wrapper">
      <span className="time-label">{fmt(position)}</span>
      <div
        ref={ref}
        className={`progress-bar ${dragging ? "dragging" : ""}`}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
      >
        <div className="progress-track">
          <div className="progress-fill" style={{ width: `${ratio * 100}%` }} />
        </div>
        <div
          className="progress-thumb"
          style={{ left: `${ratio * 100}%` }}
        />
        {preview !== null && (
          <div className="progress-preview" style={{ left: `${(preview / total) * 100}%` }}>
            <span className="preview-tip">{fmt(preview)}</span>
          </div>
        )}
      </div>
      <span className="time-label">{fmt(duration)}</span>
    </div>
  );
}
