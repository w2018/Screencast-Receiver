// 网络视频播放弹窗：URL 直接播放 + 二维码扫码远程播放 + 播放历史
// 远程播放：内置 HTTP 服务 → 手机扫码打开网页输入 URL → 后端 emit remote-play-request → 此处自动播放
// 播放历史：localStorage 持久化，点击编辑框弹出，支持单条删除
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Globe, X, Copy, Play, Clock, Trash2 } from "lucide-react";
import QRCode from "qrcode";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { usePlayerStore } from "../../stores/playerStore";
import { useToastStore } from "../../stores/toastStore";
import { loadFile } from "../../lib/mpv";

const HISTORY_KEY = "remote_play_history";
const HISTORY_MAX = 20;

interface RemoteInfo {
  ip: string;
  port: number;
  url: string;
}

interface RemotePlayDialogProps {
  open: boolean;
  onClose: () => void;
}

/** 读取播放历史（按时间倒序，最新在前） */
function loadHistory(): string[] {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    return Array.isArray(arr) ? arr.filter((u) => typeof u === "string") : [];
  } catch {
    return [];
  }
}

/** 记录播放历史：去重置顶，最多保留 HISTORY_MAX 条 */
function saveHistory(url: string): string[] {
  const list = loadHistory();
  const next = [url, ...list.filter((u) => u !== url)].slice(0, HISTORY_MAX);
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
  } catch {
    // 存储失败静默
  }
  return next;
}

/** 播放网络视频（扩展逻辑，不修改现有播放核心） */
async function playRemote(url: string) {
  usePlayerStore.setState({
    loading: true,
    error: null,
    status: "loading",
    filename: url,
    currentUrl: url,
    position: 0,
    buffering: false,
    allowAutoRetry: true,
  });
  try {
    // 与 DLNA 投屏一致：恢复防盗链请求头（B 站等 CDN 需要）
    await invoke("setup_stream_headers_command", { uri: url }).catch(() => {});
    await loadFile(url);
    usePlayerStore.setState({ loading: false });
  } catch (e) {
    usePlayerStore.setState({ loading: false, error: String(e), status: "error" });
  }
}

export function RemotePlayDialog({ open, onClose }: RemotePlayDialogProps) {
  const show = useToastStore((s) => s.show);
  const [inputUrl, setInputUrl] = useState("");
  const [info, setInfo] = useState<RemoteInfo | null>(null);
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [history, setHistory] = useState<string[]>([]);
  const [historyOpen, setHistoryOpen] = useState(false);

  // 打开时加载历史 + 启动远程服务 + 生成二维码
  useEffect(() => {
    if (!open) return;
    let alive = true;
    setErr(null);
    setHistory(loadHistory());
    (async () => {
      try {
        const r = await invoke<RemoteInfo>("start_remote_play_server");
        if (!alive) return;
        setInfo(r);
        const qr = await QRCode.toDataURL(r.url, { width: 150, margin: 1 });
        if (alive) setQrDataUrl(qr);
      } catch (e) {
        if (alive) setErr(`远程服务启动失败: ${String(e)}`);
      }
    })();
    return () => {
      alive = false;
    };
  }, [open]);

  // 监听手机端提交的播放请求 → 自动播放并记录
  useEffect(() => {
    if (!open) return;
    let un: (() => void) | undefined;
    listen<string>("remote-play-request", (e) => {
      const url = e.payload;
      if (!url) return;
      playRemote(url);
      setHistory(saveHistory(url));
      show(`已收到远程播放请求，正在播放`);
      onClose();
    }).then((fn) => {
      un = fn;
    });
    return () => {
      un?.();
    };
  }, [open, onClose, show]);

  if (!open) return null;

  /** 本地播放并记录历史 */
  const handlePlay = (url: string) => {
    playRemote(url);
    setHistory(saveHistory(url));
    show("正在播放");
    setHistoryOpen(false);
  };

  /** 删除一条历史 */
  const removeHistory = (url: string) => {
    const next = history.filter((u) => u !== url);
    setHistory(next);
    try {
      localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
    } catch {
      // 忽略
    }
  };

  const copyLink = async () => {
    if (!info) return;
    try {
      await writeText(info.url);
      show("远程控制链接已复制");
    } catch (e) {
      show(`复制失败: ${String(e)}`);
    }
  };

  return createPortal(
    <div className="modal-overlay">
      <div className="modal remote-modal">
        <div className="modal-header">
          <Globe size={18} className="modal-icon" />
          <h3>网络视频播放</h3>
          <button className="icon-btn" onClick={onClose} title="关闭">
            <X size={16} />
          </button>
        </div>

        {/* 本机输入播放 */}
        <p className="modal-desc">输入网络视频 URL（http/https）播放：</p>
        <div className="remote-input-wrap">
          <div className="remote-input-row">
            <input
              className="remote-url-input"
              type="url"
              placeholder="https://example.com/video.mp4"
              value={inputUrl}
              onChange={(e) => setInputUrl(e.target.value)}
              onFocus={() => setHistoryOpen(true)}
              onBlur={() => {
                // 延迟关闭，允许点击下拉项
                window.setTimeout(() => setHistoryOpen(false), 180);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && inputUrl.trim()) handlePlay(inputUrl.trim());
              }}
            />
            <button
              className="modal-btn accept"
              disabled={!inputUrl.trim()}
              onClick={() => handlePlay(inputUrl.trim())}
            >
              <Play size={14} /> 播放
            </button>
          </div>
          {/* 播放历史下拉列表 */}
          {historyOpen && history.length > 0 && (
            <div className="remote-history">
              <div className="remote-history-title">
                <Clock size={12} /> 播放记录
              </div>
              {history.map((u) => (
                <div
                  key={u}
                  className="remote-history-item"
                  onClick={() => handlePlay(u)}
                  title="点击播放"
                >
                  <span className="remote-history-url">{u}</span>
                  <button
                    className="remote-history-del"
                    title="删除记录"
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={(e) => {
                      e.stopPropagation();
                      removeHistory(u);
                    }}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* 手机远程控制 */}
        <div className="remote-divider">
          <span>或扫码在手机上远程控制</span>
        </div>
        {err ? (
          <p className="remote-err">{err}</p>
        ) : !info ? (
          <p className="modal-desc">正在启动远程服务…</p>
        ) : (
          <div className="remote-scan">
            <div className="remote-qr">
              {qrDataUrl ? (
                <img src={qrDataUrl} alt="远程控制二维码" />
              ) : (
                <span>生成中…</span>
              )}
            </div>
            <div className="remote-info">
              <div className="modal-info-row">
                <span className="info-label">局域网地址</span>
                <span className="info-value info-uri">{info.url}</span>
              </div>
              <div className="modal-info-row">
                <span className="info-label">IP</span>
                <span className="info-value">{info.ip}</span>
              </div>
              <div className="modal-info-row">
                <span className="info-label">端口</span>
                <span className="info-value">{info.port}</span>
              </div>
              <button className="remote-copy-btn" onClick={copyLink}>
                <Copy size={13} /> 复制链接
              </button>
            </div>
          </div>
        )}
        <p className="modal-desc remote-tip">
          手机扫码打开网页后，输入视频 URL 并点击播放，本机将自动开始播放。
        </p>
      </div>
    </div>,
    document.body
  );
}