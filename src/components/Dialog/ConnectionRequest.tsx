// 投屏请求确认弹窗（允许/拒绝 + 设备信息）
import { Monitor, X } from "lucide-react";
import { usePlayerStore } from "../../stores/playerStore";
import { useToastStore } from "../../stores/toastStore";

export interface CastRequest {
  uri: string;
  clientIp: string;
  userAgent: string;
}

interface ConnectionRequestProps {
  request: CastRequest;
  onClose: () => void;
}

/** 从 User-Agent 简单解析设备名称 */
function parseDeviceName(ua: string): string {
  if (!ua) return "未知设备";
  // 常见格式: "Bili_3.0.2.2_Banana 6.0.1" / "AppName/1.0"
  const first = ua.split(/[\/\s_]/)[0];
  return first || "未知设备";
}

export function ConnectionRequest({ request, onClose }: ConnectionRequestProps) {
  const loadAndPlay = usePlayerStore((s) => s.loadAndPlay);
  const show = useToastStore((s) => s.show);

  const accept = () => {
    // 记录投屏 URL（用于网络错误重连）并播放
    usePlayerStore.setState({ currentUrl: request.uri });
    loadAndPlay(request.uri);
    show("已允许投屏，正在播放");
    onClose();
  };

  const reject = () => {
    usePlayerStore.setState({ currentUrl: null });
    show("已拒绝投屏请求");
    onClose();
  };

  return (
    <div className="modal-overlay">
      <div className="modal">
        <div className="modal-header">
          <Monitor size={18} className="modal-icon" />
          <h3>收到投屏请求</h3>
          <button className="icon-btn" onClick={reject} title="关闭">
            <X size={16} />
          </button>
        </div>
        <p className="modal-desc">以下设备请求投屏到本机：</p>
        <div className="modal-info">
          <div className="modal-info-row">
            <span className="info-label">设备名称</span>
            <span className="info-value">{parseDeviceName(request.userAgent)}</span>
          </div>
          <div className="modal-info-row">
            <span className="info-label">设备 IP</span>
            <span className="info-value">{request.clientIp}</span>
          </div>
          <div className="modal-info-row">
            <span className="info-label">视频地址</span>
            <span className="info-value info-uri">{request.uri}</span>
          </div>
        </div>
        <div className="modal-btns">
          <button className="modal-btn reject" onClick={reject}>
            拒绝
          </button>
          <button className="modal-btn accept" onClick={accept}>
            允许
          </button>
        </div>
      </div>
    </div>
  );
}
