// 设置面板页面（F8）
// 分类：通用 / 播放 / 快捷键 / 网络
// 所有设置即时保存到 SQLite
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { X, RotateCcw, Copy, MonitorPlay, User } from "lucide-react";
import {
  useSettingsStore,
  DEFAULT_SHORTCUTS,
  type SettingKey,
  type ShortcutsConfig,
} from "../../stores/settingsStore";

interface SettingsPageProps {
  onClose: () => void;
}

type Tab = "general" | "playback" | "shortcuts" | "network" | "about";

// ===== 开关控件 =====
function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      className={`toggle ${checked ? "on" : ""}`}
      onClick={() => onChange(!checked)}
      role="switch"
      aria-checked={checked}
    >
      <span className="toggle-thumb" />
    </button>
  );
}

// ===== 设置行 =====
function SettingRow({
  label,
  desc,
  children,
}: {
  label: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="setting-row">
      <div className="setting-info">
        <div className="setting-label">{label}</div>
        {desc && <div className="setting-desc">{desc}</div>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

// 快捷键动作中文名
const SHORTCUT_NAMES: Record<string, string> = {
  play_pause: "播放/暂停",
  seek_back: "后退 5 秒",
  seek_forward: "前进 5 秒",
  volume_up: "音量 +5",
  volume_down: "音量 -5",
  mute: "静音切换",
  fullscreen: "全屏切换",
  mirror: "镜像翻转",
};

export function SettingsPage({ onClose }: SettingsPageProps) {
  const [tab, setTab] = useState<Tab>("general");
  const [recording, setRecording] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [dlnaStatus, setDlnaStatus] = useState<{
    ip: string;
    port: number;
  } | null>(null);
  const [firewallCmd, setFirewallCmd] = useState("");
  const [fwAllowed, setFwAllowed] = useState<boolean | null>(null);
  const [fwBusy, setFwBusy] = useState(false);
  const [appVersion, setAppVersion] = useState("");

  // 查询 DLNA 服务状态（绑定 IP + 端口）
  useEffect(() => {
    invoke<{ ip: string; port: number } | null>("get_dlna_status")
      .then(setDlnaStatus)
      .catch(() => {});
    // 获取完整防火墙命令
    invoke<string>("get_firewall_command")
      .then(setFirewallCmd)
      .catch(() => {});
    // 获取应用版本号
    getVersion()
      .then(setAppVersion)
      .catch(() => {});
    // 检查防火墙放行状态
    invoke<boolean>("check_firewall_rule")
      .then(setFwAllowed)
      .catch(() => setFwAllowed(null));
  }, []);

  // 一键放行防火墙（UAC 提权）
  const allowFirewall = async () => {
    setFwBusy(true);
    try {
      await invoke("add_firewall_rule");
      flashKey("请在弹出的 UAC 窗口中点击“是”");
      // 延迟几秒后复查状态（等 netsh 执行完）
      setTimeout(async () => {
        try {
          const ok = await invoke<boolean>("check_firewall_rule");
          setFwAllowed(ok);
          flashKey(ok ? "防火墙已放行" : "仍未放行，请检查 UAC 或手动执行命令");
        } catch {
          setFwAllowed(null);
        }
      }, 3000);
    } catch (e) {
      flashKey(String(e));
    } finally {
      setFwBusy(false);
    }
  };

  // 复制命令到剪贴板
  const copyCommand = async () => {
    try {
      await writeText(firewallCmd);
      flashKey("命令已复制，请在管理员命令行中粘贴执行");
    } catch {
      flashKey("复制失败，请手动复制");
    }
  };

  const s = useSettingsStore();

  // 快捷键录制：监听键盘
  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const combo = parseKeyCombo(e);
      if (!combo) return;
      const next = {
        ...s.shortcuts,
        [recording]: { ...s.shortcuts[recording], shortcut: combo },
      };
      s.updateShortcuts(next);
      setRecording(null);
      flashKey(`已设置为 ${combo}`);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, s]);

  // 全局快捷键开关
  const toggleGlobal = (action: string, global: boolean) => {
    const next = {
      ...s.shortcuts,
      [action]: { ...s.shortcuts[action], global },
    } as ShortcutsConfig;
    s.updateShortcuts(next);
    flashKey(global ? `已启用全局：${s.shortcuts[action].shortcut}` : "已关闭全局");
  };

  // 按 Esc 关闭设置页
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const flashKey = (msg: string) => {
    setFlash(msg);
    setTimeout(() => setFlash(null), 2000);
  };

  const update = (key: SettingKey, value: string | number | boolean) => {
    s.updateSetting(key, value);
  };

  return (
    <div className="settings-page" onClick={(e) => e.stopPropagation()}>
      {/* 顶栏 */}
      <div className="settings-header">
        <h2>设置</h2>
        <button className="icon-btn" onClick={onClose} title="关闭">
          <X size={18} />
        </button>
      </div>

      {/* 标签页 */}
      <div className="settings-tabs">
        <button className={tab === "general" ? "active" : ""} onClick={() => setTab("general")}>
          通用
        </button>
        <button className={tab === "playback" ? "active" : ""} onClick={() => setTab("playback")}>
          播放
        </button>
        <button className={tab === "shortcuts" ? "active" : ""} onClick={() => setTab("shortcuts")}>
          快捷键
        </button>
        <button className={tab === "network" ? "active" : ""} onClick={() => setTab("network")}>
          网络
        </button>
        <button className={tab === "about" ? "active" : ""} onClick={() => setTab("about")}>
          关于
        </button>
      </div>

      <div className="settings-body">
        {/* ===== 通用设置 ===== */}
        {tab === "general" && (
          <>
            <SettingRow label="开机自动启动" desc="登录 Windows 后自动启动应用">
              <Toggle
                checked={s.autoStart}
                onChange={(v) => update("autoStart", v)}
              />
            </SettingRow>
            <SettingRow label="关闭窗口时最小化到托盘" desc="关闭按钮只隐藏到系统托盘，不退出">
              <Toggle
                checked={s.minimizeToTray}
                onChange={(v) => update("minimizeToTray", v)}
              />
            </SettingRow>
            <SettingRow label="最小化到托盘时暂停播放" desc="隐藏窗口时自动暂停，恢复后继续">
              <Toggle
                checked={s.pauseOnMinimize}
                onChange={(v) => update("pauseOnMinimize", v)}
              />
            </SettingRow>
            <SettingRow label="界面语言" desc="简体中文 / English">
              <select
                className="select"
                value={s.language}
                onChange={(e) => update("language", e.target.value)}
              >
                <option value="zh-CN">简体中文</option>
                <option value="en">English</option>
              </select>
            </SettingRow>
            <SettingRow label="界面主题" desc="深色 / 浅色">
              <select
                className="select"
                value={s.theme}
                onChange={(e) => update("theme", e.target.value)}
              >
                <option value="dark">深色</option>
                <option value="light">浅色</option>
              </select>
            </SettingRow>
          </>
        )}

        {/* ===== 播放设置 ===== */}
        {tab === "playback" && (
          <>
            <SettingRow label={`默认音量 ${s.defaultVolume}%`} desc="0-100，播放时应用">
              <input
                type="range"
                className="slider"
                min={0}
                max={100}
                value={s.defaultVolume}
                onChange={(e) => update("defaultVolume", Number(e.target.value))}
              />
            </SettingRow>
            <SettingRow label={`默认倍速 ${s.defaultSpeed}x`} desc="打开视频时应用的播放速度">
              <select
                className="select"
                value={s.defaultSpeed}
                onChange={(e) => update("defaultSpeed", Number(e.target.value))}
              >
                {[0.5, 0.75, 1.0, 1.25, 1.5, 2.0].map((v) => (
                  <option key={v} value={v}>
                    {v}x
                  </option>
                ))}
              </select>
            </SettingRow>
            <SettingRow label="打开视频时自动调整窗口大小" desc="窗口调整为视频原始分辨率">
              <Toggle
                checked={s.autoFitWindow}
                onChange={(v) => update("autoFitWindow", v)}
              />
            </SettingRow>
            <SettingRow label="硬件解码" desc="使用 GPU 加速解码（D3D11VA）">
              <Toggle
                checked={s.hardwareDecode}
                onChange={(v) => update("hardwareDecode", v)}
              />
            </SettingRow>
            <SettingRow label={`网络缓冲 ${s.networkCacheSecs} 秒`} desc="网络流预缓冲时长">
              <input
                type="range"
                className="slider"
                min={1}
                max={60}
                value={s.networkCacheSecs}
                onChange={(e) => update("networkCacheSecs", Number(e.target.value))}
              />
            </SettingRow>
            <SettingRow label="循环播放" desc="播放完毕后自动重播">
              <Toggle
                checked={s.loopPlayback}
                onChange={(v) => update("loopPlayback", v)}
              />
            </SettingRow>
          </>
        )}

        {/* ===== 快捷键设置 ===== */}
        {tab === "shortcuts" && (
          <>
            <div className="shortcuts-header">
              <span>点击快捷键可重新录制，可启用全局（应用不在前台时生效）</span>
              <button
                className="text-btn"
                onClick={async () => {
                  await s.resetShortcuts();
                  flashKey("已重置为默认快捷键");
                }}
              >
                <RotateCcw size={14} /> 重置默认
              </button>
            </div>
            <div className="shortcut-list">
              {Object.entries(DEFAULT_SHORTCUTS).map(([action]) => (
                <div key={action} className="shortcut-row">
                  <span className="shortcut-name">
                    {SHORTCUT_NAMES[action] ?? action}
                  </span>
                  <span className="shortcut-ctrl">
                    <button
                      className={`key-box ${recording === action ? "recording" : ""}`}
                      onClick={() => setRecording(recording === action ? null : action)}
                    >
                      {recording === action
                        ? "按下新快捷键…"
                        : s.shortcuts[action]?.shortcut ?? "未设置"}
                    </button>
                    <Toggle
                      checked={s.shortcuts[action]?.global ?? false}
                      onChange={(v) => toggleGlobal(action, v)}
                    />
                    <span className="global-tag">
                      {s.shortcuts[action]?.global ? "全局" : "窗口"}
                    </span>
                  </span>
                </div>
              ))}
            </div>
          </>
        )}

        {/* ===== 网络设置 ===== */}
        {tab === "network" && (
          <>
            <SettingRow label="DLNA 投屏接收" desc="作为 DLNA Renderer，接收手机投屏">
              <Toggle
                checked={s.dlnaEnabled}
                onChange={(v) => update("dlnaEnabled", v)}
              />
            </SettingRow>
            <SettingRow label="投屏设备名称" desc="手机上显示的投屏目标名称">
              <input
                type="text"
                className="text-input"
                defaultValue={s.deviceName}
                maxLength={32}
                onBlur={(e) => {
                  if (e.target.value.trim() !== s.deviceName) {
                    update("deviceName", e.target.value.trim());
                  }
                }}
              />
            </SettingRow>
            <div className="setting-note">
              <strong>DLNA 服务状态</strong>：{dlnaStatus
                ? `已启动，监听 ${dlnaStatus.ip}:${dlnaStatus.port}`
                : "未启动"}
            </div>
            <div className="setting-note">
              <strong>⚠️ 手机扫描不到设备时的排查：</strong>
              <br />① 确认手机与电脑在<strong>同一 Wi-Fi 局域网</strong>；
              <br />② 放行防火墙（未放行时手机无法连接投屏服务）：
            </div>
            <div className="fw-cmd-box">
              <code className="fw-cmd">{firewallCmd || "正在获取命令…"}</code>
              <button
                className="text-btn fw-copy-btn"
                onClick={copyCommand}
                disabled={!firewallCmd}
                title="复制命令"
              >
                <Copy size={14} /> 复制命令
              </button>
            </div>
            <div className="fw-status">
              {fwBusy ? (
                <span className="fw-status-warn">正在请求管理员授权…</span>
              ) : fwAllowed === true ? (
                <span className="fw-status-ok">✔ 防火墙已放行，手机可以连接本服务</span>
              ) : fwAllowed === false ? (
                <span className="fw-status-warn">
                  ⚠ 防火墙未放行，手机可能无法发现/连接本服务
                </span>
              ) : (
                <span className="fw-status-warn">防火墙状态未知</span>
              )}
              <button
                className="text-btn fw-allow-btn"
                onClick={allowFirewall}
                disabled={fwBusy || fwAllowed === true}
              >
                一键放行（需管理员授权）
              </button>
            </div>
            <div className="setting-note">
              使用方法：点击"一键放行"→ 在弹出的 UAC 窗口点"是"；或复制上方命令 → 以<strong>管理员身份</strong>打开命令提示符 → 粘贴执行。
            </div>
          </>
        )}

        {/* ===== 关于 ===== */}
        {tab === "about" && (
          <div className="about">
            <div className="about-logo">
              <MonitorPlay size={36} />
            </div>
            <div className="about-name">投屏助手</div>
            <div className="about-version">版本 v{appVersion || "1.2.1"}</div>
            <div className="about-author">
              <User size={13} /> 作者：曾先生
            </div>
            <div className="about-desc">
              基于 Tauri 2 + libmpv 内核的 Windows 桌面 DLNA 投屏接收端，接收手机/设备的投屏并自动播放。
            </div>
          </div>
        )}
      </div>

      {/* 底部提示 */}
      {flash && <div className="settings-toast">{flash}</div>}
    </div>
  );
}

/** 将键盘事件解析为快捷键字符串 */
function parseKeyCombo(e: KeyboardEvent): string | null {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Win");

  let key = "";
  if (e.code === "Space") key = "Space";
  else if (e.key.length === 1) key = e.key.toUpperCase();
  else if (e.key.startsWith("F") && e.key.length <= 3) key = e.key;
  else if (["Enter", "Tab", "Escape"].includes(e.key)) key = e.key;
  else return null;

  return [...parts, key].join("+");
}
