// 播放控制栏：播放/暂停、进度、音量、倍速、全屏、镜像、设置
import { useState } from "react";
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  RotateCcw,
  RotateCw,
  Maximize,
  Minimize,
  Settings,
  FlipHorizontal2,
  FlipVertical2,
} from "lucide-react";
import { usePlayerStore } from "../../stores/playerStore";
import { ProgressBar } from "./ProgressBar";
import { VolumeSlider } from "./VolumeSlider";

// 可选倍速档位
const SPEED_OPTIONS = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

interface ControlBarProps {
  isFullscreen: boolean;
  onToggleFullscreen: () => void;
  onOpenSettings: () => void;
  onToggleMirrorH: () => void;
  onToggleMirrorV: () => void;
  mirrorH: boolean;
  mirrorV: boolean;
}

export function ControlBar({
  isFullscreen,
  onToggleFullscreen,
  onOpenSettings,
  onToggleMirrorH,
  onToggleMirrorV,
  mirrorH,
  mirrorV,
}: ControlBarProps) {
  const status = usePlayerStore((s) => s.status);
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const volume = usePlayerStore((s) => s.volume);
  const muted = usePlayerStore((s) => s.muted);
  const speed = usePlayerStore((s) => s.speed);
  const {
    togglePlayPause,
    seekTo,
    seekBy,
    changeVolume,
    toggleMute,
    changeSpeed,
  } = usePlayerStore();

  const [speedOpen, setSpeedOpen] = useState(false);

  const isPlaying = status === "playing";

  return (
    <div className="control-bar" onClick={(e) => e.stopPropagation()}>
      <ProgressBar position={position} duration={duration} onSeek={seekTo} />

      <div className="controls-row">
        {/* 后退 10 秒 */}
        <button className="ctrl-btn" title="后退 10 秒" onClick={() => seekBy(-10)}>
          <RotateCcw size={18} />
        </button>
        {/* 后退 5 秒 */}
        <button className="ctrl-btn" title="后退 5 秒" onClick={() => seekBy(-5)}>
          <SkipBack size={18} />
        </button>

        {/* 播放/暂停 */}
        <button
          className="ctrl-btn play-btn"
          title="播放/暂停 (Space)"
          onClick={() => togglePlayPause()}
        >
          {isPlaying ? <Pause size={22} /> : <Play size={22} />}
        </button>

        {/* 前进 5 秒 */}
        <button className="ctrl-btn" title="前进 5 秒" onClick={() => seekBy(5)}>
          <SkipForward size={18} />
        </button>
        {/* 前进 10 秒 */}
        <button className="ctrl-btn" title="前进 10 秒" onClick={() => seekBy(10)}>
          <RotateCw size={18} />
        </button>

        {/* 音量 */}
        <VolumeSlider
          volume={volume}
          muted={muted}
          onChange={changeVolume}
          onToggleMute={toggleMute}
        />

        {/* 倍速选择 */}
        <div className="speed-select">
          <button
            className="ctrl-btn speed-btn"
            title="倍速"
            onClick={() => setSpeedOpen((v) => !v)}
          >
            {speed}x
          </button>
          {speedOpen && (
            <div className="speed-menu">
              {SPEED_OPTIONS.map((s) => (
                <button
                  key={s}
                  className={`speed-item ${s === speed ? "active" : ""}`}
                  onClick={() => {
                    changeSpeed(s);
                    setSpeedOpen(false);
                  }}
                >
                  {s}x
                </button>
              ))}
            </div>
          )}
        </div>

        <div className="spacer" />

        {/* 镜像翻转 */}
        <button
          className={`ctrl-btn ${mirrorH ? "active" : ""}`}
          title="水平镜像 (C)"
          onClick={onToggleMirrorH}
        >
          <FlipHorizontal2 size={18} />
        </button>
        <button
          className={`ctrl-btn ${mirrorV ? "active" : ""}`}
          title="垂直镜像"
          onClick={onToggleMirrorV}
        >
          <FlipVertical2 size={18} />
        </button>

        {/* 设置 */}
        <button className="ctrl-btn" title="设置" onClick={onOpenSettings}>
          <Settings size={18} />
        </button>

        {/* 全屏 */}
        <button className="ctrl-btn" title="全屏 (F)" onClick={onToggleFullscreen}>
          {isFullscreen ? <Minimize size={18} /> : <Maximize size={18} />}
        </button>
      </div>
    </div>
  );
}
