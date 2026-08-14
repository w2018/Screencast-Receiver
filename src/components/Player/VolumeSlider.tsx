// 音量滑块：滑动调节 + 悬停展开
import { useRef, useState } from "react";
import { Volume, Volume1, Volume2, VolumeX } from "lucide-react";

interface VolumeSliderProps {
  volume: number;
  muted: boolean;
  onChange: (volume: number) => void;
  onToggleMute: () => void;
}

export function VolumeSlider({
  volume,
  muted,
  onChange,
  onToggleMute,
}: VolumeSliderProps) {
  const [expanded, setExpanded] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const Icon =
    muted || volume === 0 ? VolumeX : volume < 40 ? Volume : volume < 70 ? Volume1 : Volume2;

  const handleClick = (e: React.MouseEvent) => {
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const r = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    onChange(Math.round(r * 100));
  };

  return (
    <div
      className="volume-slider"
      onMouseEnter={() => setExpanded(true)}
      onMouseLeave={() => setExpanded(false)}
    >
      <button className="ctrl-btn" onClick={onToggleMute} title="静音切换 (M)">
        <Icon size={18} />
      </button>
      <div className={`volume-track-wrap ${expanded ? "expanded" : ""}`}>
        <div ref={ref} className="volume-track" onClick={handleClick}>
          <div
            className="volume-fill"
            style={{ width: `${muted ? 0 : volume}%` }}
          />
        </div>
      </div>
    </div>
  );
}
