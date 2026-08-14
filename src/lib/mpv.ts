// MPV 命令封装层
// 基于 tauri-plugin-libmpv-api 的命令/属性 API，封装成语义化函数
// 参考 MPV 官方文档：https://mpv.io/manual/master/#list-of-input-commands
import {
  init as mpvInit,
  command,
  setProperty,
  getProperty,
  type MpvConfig,
  type MpvObservableProperty,
} from "tauri-plugin-libmpv-api";

// ===== MPV 初始化配置（对应 MPV 命令行参数，无 "--" 前缀）=====
export const INITIAL_OPTIONS: Record<string, string | boolean | number> = {
  vo: "gpu-next", // GPU 渲染（更稳定）
  hwdec: "d3d11va", // D3D11 硬件解码
  "hwdec-codecs": "all", // 全部 codec 尝试硬解
  "keep-open": "yes", // 播完不关窗口
  "force-window": "yes", // 强制显示窗口
  "network-timeout": "15", // 网络超时 15 秒
  "rtsp-transport": "tcp", // RTSP 优先 TCP
  cache: "yes", // 开启缓存
  "cache-secs": "60", // 缓存 60 秒
  "demuxer-max-bytes": "100M", // 解复用器最大缓冲
  "http-header-fields": "User-Agent: Mozilla/5.0", // 伪装 UA
  aid: "auto", // 自动音轨
  vid: "auto", // 自动视频轨
  "osd-level": "0", // 关闭 OSD
  "input-default-bindings": "no", // 禁用 MPV 默认键绑定
  osc: "no", // 禁用 MPV 内置控制条
};

/** 需要监听的属性 */
export const OBSERVED_PROPERTIES = [
  ["pause", "flag"],
  ["time-pos", "double", "none"],
  ["duration", "double", "none"],
  ["volume", "int64"],
  ["mute", "flag"],
  ["speed", "double"],
  ["width", "int64", "none"],
  ["height", "int64", "none"],
  ["filename", "string", "none"],
  ["eof-reached", "flag"],
  ["cache-buffering-state", "int64", "none"],
] as const satisfies readonly MpvObservableProperty[];

/** 初始化 MPV 播放器，支持按设置覆盖初始选项 */
export function initMpv(
  overrides?: Record<string, string | boolean | number>,
): Promise<string> {
  const config: MpvConfig = {
    initialOptions: { ...INITIAL_OPTIONS, ...overrides },
    observedProperties: OBSERVED_PROPERTIES,
  };
  return mpvInit(config);
}

/** 加载并播放一个文件或网络流 */
export function loadFile(url: string): Promise<void> {
  return command("loadfile", [url]);
}

/** 开始播放（取消暂停） */
export function play(): Promise<void> {
  return setProperty("pause", false);
}

/** 暂停播放 */
export function pause(): Promise<void> {
  return setProperty("pause", true);
}

/** 切换播放/暂停 */
export function togglePause(): Promise<void> {
  return command("cycle", ["pause"]);
}

/** 停止播放 */
export function stop(): Promise<void> {
  return command("stop");
}

/** 绝对跳转到指定秒 */
export function seekAbsolute(seconds: number): Promise<void> {
  return command("seek", [seconds, "absolute"]);
}

/** 相对跳转（正数前进，负数后退） */
export function seekRelative(seconds: number): Promise<void> {
  return command("seek", [seconds, "relative"]);
}

/** 设置倍速 */
export function setSpeed(speed: number): Promise<void> {
  return setProperty("speed", speed);
}

/** 设置音量（0-100） */
export function setVolume(volume: number): Promise<void> {
  return setProperty("volume", Math.max(0, Math.min(100, volume)));
}

/** 切换静音 */
export function toggleMute(): Promise<void> {
  return command("cycle", ["mute"]);
}

/** 设置静音状态 */
export function setMute(muted: boolean): Promise<void> {
  return setProperty("mute", muted);
}

/** 设置循环播放 */
export function setLoop(loop: boolean): Promise<void> {
  return setProperty("loop-file", loop ? "inf" : "no");
}

/** 设置视频滤镜（用于镜像翻转，null 清除） */
export function setVideoFilter(filter: string | null): Promise<void> {
  return setProperty("vf", filter ?? "");
}

/** 读取当前播放位置（秒） */
export async function getTimePos(): Promise<number | null> {
  return (await getProperty("time-pos", "double")) as number | null;
}

/** 读取视频总时长（秒） */
export async function getDuration(): Promise<number | null> {
  return (await getProperty("duration", "double")) as number | null;
}

/** 读取音量 */
export async function getVolume(): Promise<number | null> {
  return (await getProperty("volume", "int64")) as number | null;
}

/** 读取倍速 */
export async function getSpeed(): Promise<number | null> {
  return (await getProperty("speed", "double")) as number | null;
}

/** 读取是否暂停 */
export async function getPause(): Promise<boolean | null> {
  return (await getProperty("pause", "flag")) as boolean | null;
}

/** 读取静音状态 */
export async function getMute(): Promise<boolean | null> {
  return (await getProperty("mute", "flag")) as boolean | null;
}

/** 读取当前文件名 */
export async function getFilename(): Promise<string | null> {
  return (await getProperty("filename", "string")) as string | null;
}

/** 读取视频原始宽度 */
export async function getVideoWidth(): Promise<number | null> {
  return (await getProperty("width", "int64")) as number | null;
}

/** 读取视频原始高度 */
export async function getVideoHeight(): Promise<number | null> {
  return (await getProperty("height", "int64")) as number | null;
}
