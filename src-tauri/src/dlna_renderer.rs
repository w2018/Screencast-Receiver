// DLNA Renderer 接收端（F9）
// 实现最小化 UPnP MediaRenderer：
//  - SSDP 响应（UDP 1900）：宣告设备存在，供手机发现
//  - HTTP 服务：设备描述 / SCPD / SOAP 控制
//  - SOAP 动作：SetAVTransportURI / Play / Pause / Stop / Seek /
//    GetPositionInfo / GetTransportInfo / SetVolume / GetVolume / SetMute
//  - 通过 tauri-plugin-libmpv 的 MpvExt 直接控制 MPV 播放
//
// 说明：本实现为最小化 Renderer，仅响应关键 SOAP 动作（文档第九节允许）。
use serde::Serialize;
use serde_json::json;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::Mutex;
use std::thread;
use tauri::{AppHandle, Emitter, Manager};

// ===== 常量 =====
const DEVICE_TYPE: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
const DEVICE_TYPE_2: &str = "urn:schemas-upnp-org:device:MediaRenderer:2";
const SERVICE_AVT: &str = "urn:schemas-upnp-org:service:AVTransport:1";
const SERVICE_RC: &str = "urn:schemas-upnp-org:service:RenderingControl:1";
const SERVICE_CM: &str = "urn:schemas-upnp-org:service:ConnectionManager:1";
const SSDP_PORT: u16 = 1900;

/// 设备图标（48x48 PNG，base64 编码）——部分 DLNA 客户端无图标会过滤设备
const ICON_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAADAAAAAwCAYAAABXAvmHAAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAFcSURBVGhD7ZYxDsIwDEW5F+fhImwcgSuwcIkOvUJ3RmYQosKoSEiJnVDX/iWLI72ldaU859fKZru/0SqcR8rW8JA1ADb8AYwQ0BECVUJARwhUCQEdIVAlBHSEQJUQ0LGewJ8IgdaEQGtCoDUh0JoQaE0ItMYocKfTNb9s6m+bD+qzD190OvIaPUaBG+26V7YNopEOhToBv2Zfn7TjNQswC2yPT7rkW6H+XKhjHIbl3/zCLmCKEY+P8tR+4BAwxIjHZ1Z4HpfA0hih4zPhE1gSIyE7c1pKnAL6GPG6S3cXNRbcArKzpWjwk/LN/hS/gNhcobtc0jn7UwACMh58g/y9EHQAERAdziLCT6j8j1jBCIhNJl3mcrUpZQQkIGPyjRF/Ln9wHzAB0elPjPjJYOMzgRMoxWgYV43PBFBAxiVfuNmfAhWQMUoWcPanYAUKMfou5OxPAQvUYrROfCbeuJBkKXsDPz8AAAAASUVORK5CYII=";

/// 当前投屏会话（单会话，存当前 URI）
pub struct DlnaSession(pub Mutex<String>);

/// DLNA 服务状态（绑定 IP + 端口，供设置界面显示）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DlnaStatus {
    pub ip: String,
    pub port: u16,
}

/// DLNA 状态存储（Tauri state）
pub struct DlnaStatusState(pub Mutex<Option<DlnaStatus>>);

/// GENA 事件订阅者（SID + 回调地址），用于 LastChange 事件通知
#[derive(Clone)]
pub struct EventSubscriber {
    pub sid: String,
    pub callback: String,
}

/// GENA 订阅状态存储
pub struct EventSubs(pub Mutex<Vec<EventSubscriber>>);

/// byebye 宣告所需状态（uuid + port + 接口 IP，应用退出时发送 ssdp:byebye）
static BYEBYE_STATE: std::sync::OnceLock<std::sync::Mutex<Option<(String, u16)>>> =
    std::sync::OnceLock::new();

/// DLNA 服务运行标志（设置开关动态启停：false 时 ssdp 循环退出并发送 byebye）
static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// HTTP 监听器句柄（停止服务时关闭以结束 accept 循环）
static HTTP_LISTENER: std::sync::OnceLock<std::sync::Mutex<Option<TcpListener>>> =
    std::sync::OnceLock::new();

fn byebye_state() -> &'static std::sync::Mutex<Option<(String, u16)>> {
    BYEBYE_STATE.get_or_init(|| std::sync::Mutex::new(None))
}

// ===== 工具函数 =====

/// 生成 RFC 1123 格式的 HTTP DATE 头值（UPnP 规范要求 SSDP 消息携带 DATE）
fn http_date() -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let wday = ((days + 4).rem_euclid(7)) as usize;
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s) = (
        secs.rem_euclid(86400) / 3600,
        secs.rem_euclid(3600) / 60,
        secs.rem_euclid(60),
    );
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        DAYS[wday], d, MONTHS[(m - 1) as usize], y, h, mi, s
    )
}

/// 儒略日 → (年, 月, 日)（Howard Hinnant 算法）
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 获取本机局域网 IP（WLAN 优先，排除 Tailscale/vEthernet 等虚拟网卡）
/// 兜底：UDP connect 8.8.8.8 走默认路由
pub fn local_ip() -> String {
    let mut wired: Option<String> = None;
    if let Ok(ifaces) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in ifaces {
            if let std::net::IpAddr::V4(v4) = ip {
                if !is_useful_iface(&name, &v4) {
                    continue;
                }
                let n = name.to_lowercase();
                // 无线网卡优先（手机投屏场景：手机与 PC 通常都连 WiFi）
                if n.contains("wlan") || n.contains("wireless") || n.contains("wi-fi") || n.contains("无线") {
                    return v4.to_string();
                }
                if wired.is_none() {
                    wired = Some(v4.to_string());
                }
            }
        }
    }
    if let Some(w) = wired {
        return w;
    }
    let socket = UdpSocket::bind("0.0.0.0:0").ok();
    if let Some(s) = socket {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = s.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

/// DLNA 可用网卡（供设置页选择投屏网卡）
#[derive(Debug, Clone, Serialize)]
pub struct DlnaIface {
    pub name: String,
    pub ip: String,
}

/// 列出所有可用于 DLNA 投屏的物理网卡（已过滤虚拟网卡/链路本地）
#[tauri::command]
pub fn list_dlna_ifaces() -> Result<Vec<DlnaIface>, String> {
    let mut list = Vec::new();
    if let Ok(ifaces) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in ifaces {
            if let std::net::IpAddr::V4(v4) = ip {
                if is_useful_iface(&name, &v4) {
                    list.push(DlnaIface {
                        name,
                        ip: v4.to_string(),
                    });
                }
            }
        }
    }
    Ok(list)
}

/// 生成设备 UUID
fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 秒数 → "HH:MM:SS"
fn fmt_duration(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "00:00:00".to_string();
    }
    let s = secs as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// "HH:MM:SS" 或 "MM:SS" 或纯秒 → 秒数
fn parse_duration(s: &str) -> f64 {
    let parts: Vec<&str> = s.split(':').collect();
    let mut secs = 0.0;
    for p in parts {
        secs = secs * 60.0 + p.trim().parse::<f64>().unwrap_or(0.0);
    }
    secs
}

/// 提取 SOAP 动作名（匹配 `<u:ActionName` 或 `<ActionName`）
fn extract_action(body: &str) -> String {
    for prefix in ["<u:", "<", "<ns0:"] {
        if let Some(idx) = body.find(prefix) {
            let rest = &body[idx + prefix.len()..];
            let end = rest
                .find(|c: char| c == ' ' || c == '>' || c == ':' || c == '/')
                .unwrap_or(0);
            let name = &rest[..end];
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric()) {
                return name.to_string();
            }
        }
    }
    String::new()
}

/// 提取 XML 标签参数值 `<Name>value</Name>`
fn extract_param(body: &str, name: &str) -> Option<String> {
    let close = format!("</{}>", name);
    for open in [format!("<{}>", name), format!("<{} ", name)] {
        if let Some(s) = body.find(&open) {
            let start = s + open.len();
            // 若开始标签带属性，先找到 '>'
            let content_start = if open.ends_with(' ') {
                match body[s..].find('>') {
                    Some(gt) => s + gt + 1,
                    None => continue,
                }
            } else {
                start
            };
            if let Some(e) = body[content_start..].find(&close) {
                let val = body[content_start..content_start + e].trim().to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
    }
    None
}

/// HTTP 响应
fn http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body
    )
}

/// SOAP 成功响应
fn soap_response(action: &str, service: &str, inner: &str) -> String {
    let body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<s:Body><u:{}Response xmlns:u="{}">{}</u:{}Response></s:Body></s:Envelope>"#,
        action, service, inner, action
    );
    http_response("200 OK", r#"text/xml; charset="utf-8""#, &body)
}

/// SOAP 错误响应
fn soap_error(code: &str, desc: &str) -> String {
    let body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body><s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring>
<detail><UPnPError xmlns="urn:schemas-upnp-org:control-1-0"><errorCode>{}</errorCode><errorDescription>{}</errorDescription></UPnPError></detail>
</s:Fault></s:Body></s:Envelope>"#,
        code, desc
    );
    http_response(
        "500 Internal Server Error",
        r#"text/xml; charset="utf-8""#,
        &body,
    )
}

// ===== MPV 控制（通过 libmpv 插件）=====
fn mpv_cmd(app: &AppHandle, name: &str, args: Vec<serde_json::Value>) {
    use tauri_plugin_libmpv::MpvExt;
    if let Err(e) = app.mpv().command(name, &args, "main") {
        eprintln!("[DLNA] mpv command '{name}' 失败: {e}");
    }
}

fn mpv_set(app: &AppHandle, name: &str, value: serde_json::Value) {
    use tauri_plugin_libmpv::MpvExt;
    if let Err(e) = app.mpv().set_property(name, &value, "main") {
        eprintln!("[DLNA] mpv set '{name}' 失败: {e}");
    }
}

fn mpv_get(app: &AppHandle, name: &str, format: &str) -> serde_json::Value {
    use tauri_plugin_libmpv::MpvExt;
    app.mpv()
        .get_property(name.to_string(), format.to_string(), "main")
        .unwrap_or(serde_json::Value::Null)
}

/// 根据投屏 URL 的域名判断来源，设置 mpv 请求头（防盗链 Referer + 浏览器 UA）。
/// 视频平台（B 站/爱奇艺等）的 CDN 会校验 Referer，缺少时返回 403 → 播放器报"网络异常"。
pub fn setup_stream_headers(app: &AppHandle, uri: &str) {
    let host = uri
        .split("://")
        .nth(1)
        .unwrap_or("")
        .split(['/', ':', '?'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    let referer = if host.contains("bilibili.com") || host.contains("b23.tv") {
        Some("https://www.bilibili.com/")
    } else if host.contains("iqiyi.com") || host.contains("qiyi.com") {
        Some("https://www.iqiyi.com/")
    } else if host.contains("v.qq.com")
        || host.contains("qq.com")
        || host.contains("tencent.com")
    {
        Some("https://v.qq.com/")
    } else if host.contains("youku.com") {
        Some("https://www.youku.com/")
    } else if host.contains("mgtv.com") {
        Some("https://www.mgtv.com/")
    } else {
        None
    };
    let mut headers = vec![json!("User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)")];
    if let Some(r) = referer {
        headers.push(json!(format!("Referer: {}", r)));
    }
    mpv_cmd(
        app,
        "set",
        vec![
            json!("http-header-fields"),
            serde_json::Value::Array(headers),
        ],
    );
    println!(
        "[DLNA] 已设置播放请求头: {}",
        match referer {
            Some(r) => format!("带防盗链 Referer {}", r),
            None => "默认（无 Referer）".to_string(),
        }
    );
}

// ===== SOAP 动作处理 =====
fn handle_avt(
    app: &AppHandle,
    body: &str,
    session: &DlnaSession,
    client_ip: &str,
    user_agent: &str,
) -> String {
    let action = extract_action(body);
    match action.as_str() {
        "SetAVTransportURI" => {
            if let Some(raw_uri) = extract_param(body, "CurrentURI") {
                // 反转义 XML 实体（B 站等平台 URL 含 &amp;，不转回 & 会播放失败）
                let uri = xml_unescape(&raw_uri);
                // 保存会话（待用户确认后播放）
                if let Ok(mut s) = session.0.lock() {
                    *s = uri.clone();
                }
                // 根据来源设置防盗链请求头（B 站等平台 CDN 校验 Referer，缺失会 403）
                setup_stream_headers(app, &uri);
                // 唤醒窗口（可能在托盘隐藏）
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
                // 通知前端弹确认框（允许/拒绝 + 设备信息）
                let payload = serde_json::json!({
                    "uri": uri,
                    "clientIp": client_ip,
                    "userAgent": user_agent,
                });
                let _ = app.emit("dlna-request", payload);
                println!("[DLNA] 收到投屏请求: {} (from {})", uri, client_ip);
            }
            notify_last_change(app);
            soap_response("SetAVTransportURI", SERVICE_AVT, "")
        }
        "Play" => {
            mpv_set(app, "pause", json!(false));
            println!("[DLNA] Play");
            notify_last_change(app);
            soap_response("Play", SERVICE_AVT, "")
        }
        "Pause" => {
            mpv_set(app, "pause", json!(true));
            println!("[DLNA] Pause");
            notify_last_change(app);
            soap_response("Pause", SERVICE_AVT, "")
        }
        "Stop" => {
            mpv_cmd(app, "stop", vec![]);
            println!("[DLNA] Stop");
            notify_last_change(app);
            soap_response("Stop", SERVICE_AVT, "")
        }
        "Seek" => {
            let target = extract_param(body, "Target").unwrap_or_default();
            let secs = parse_duration(&target);
            mpv_cmd(app, "seek", vec![json!(secs), json!("absolute")]);
            println!("[DLNA] Seek to {}", target);
            notify_last_change(app);
            soap_response("Seek", SERVICE_AVT, "")
        }
        "Next" | "Previous" => soap_response(&action, SERVICE_AVT, ""),
        "SetNextAVTransportURI" => soap_response("SetNextAVTransportURI", SERVICE_AVT, ""),
        "GetMediaInfo" => {
            let dur = mpv_get(app, "duration", "double").as_f64().unwrap_or(0.0);
            let uri = session.0.lock().map(|s| s.clone()).unwrap_or_default();
            let inner = format!(
                "<NrTracks>1</NrTracks><MediaDuration>{}</MediaDuration><CurrentURI>{}</CurrentURI><CurrentURIMetaData></CurrentURIMetaData><NextURI></NextURI><NextURIMetaData></NextURIMetaData><PlayMedium>NETWORK</PlayMedium><RecordMedium>NONE</RecordMedium><WriteStatus>NOT_WRITABLE</WriteStatus>",
                fmt_duration(dur),
                xml_escape(&uri)
            );
            soap_response("GetMediaInfo", SERVICE_AVT, &inner)
        }
        "GetDeviceCapabilities" => soap_response(
            "GetDeviceCapabilities",
            SERVICE_AVT,
            "<PlayMedia>NETWORK</PlayMedia><RecMedia></RecMedia><RecQualityModes></RecQualityModes>",
        ),
        "GetPositionInfo" => {
            let dur = mpv_get(app, "duration", "double").as_f64().unwrap_or(0.0);
            let pos = mpv_get(app, "time-pos", "double").as_f64().unwrap_or(0.0);
            let uri = session.0.lock().map(|s| s.clone()).unwrap_or_default();
            let inner = format!(
                "<Track>1</Track><TrackDuration>{}</TrackDuration><TrackMetaData></TrackMetaData><TrackURI>{}</TrackURI><RelTime>{}</RelTime><AbsTime>{}</AbsTime><RelCount>0</RelCount><AbsCount>0</AbsCount>",
                fmt_duration(dur),
                xml_escape(&uri),
                fmt_duration(pos),
                fmt_duration(pos)
            );
            soap_response("GetPositionInfo", SERVICE_AVT, &inner)
        }
        "GetTransportInfo" => {
            let paused = mpv_get(app, "pause", "flag").as_bool().unwrap_or(false);
            let state = if paused { "PAUSED_PLAYBACK" } else { "PLAYING" };
            let inner = format!(
                "<CurrentTransportState>{}</CurrentTransportState><CurrentTransportStatus>OK</CurrentTransportStatus><CurrentSpeed>1</CurrentSpeed>",
                state
            );
            soap_response("GetTransportInfo", SERVICE_AVT, &inner)
        }
        "GetTransportSettings" => soap_response(
            "GetTransportSettings",
            SERVICE_AVT,
            "<PlayMode>NORMAL</PlayMode><RecQualityMode>0</RecQualityMode>",
        ),
        _ => soap_error("401", "Invalid Action"),
    }
}

fn handle_rc(app: &AppHandle, body: &str) -> String {
    let action = extract_action(body);
    match action.as_str() {
        "SetVolume" => {
            if let Some(v) = extract_param(body, "DesiredVolume") {
                if let Ok(n) = v.parse::<f64>() {
                    mpv_set(app, "volume", json!(n));
                }
            }
            notify_last_change(app);
            soap_response("SetVolume", SERVICE_RC, "")
        }
        "GetVolume" => {
            let vol = mpv_get(app, "volume", "int64").as_f64().unwrap_or(100.0);
            soap_response(
                "GetVolume",
                SERVICE_RC,
                &format!("<CurrentVolume>{}</CurrentVolume>", vol as u64),
            )
        }
        "SetMute" => {
            let v = extract_param(body, "DesiredMute").unwrap_or_default();
            mpv_set(
                app,
                "mute",
                json!(v == "1" || v.eq_ignore_ascii_case("true")),
            );
            notify_last_change(app);
            soap_response("SetMute", SERVICE_RC, "")
        }
        "GetMute" => {
            let muted = mpv_get(app, "mute", "flag").as_bool().unwrap_or(false);
            soap_response(
                "GetMute",
                SERVICE_RC,
                &format!("<CurrentMute>{}</CurrentMute>", if muted { 1 } else { 0 }),
            )
        }
        _ => soap_error("401", "Invalid Action"),
    }
}

/// ConnectionManager 动作（DLNA 控制点在投屏前会调用 GetProtocolInfo 检查传输协议，
/// 缺失此服务会被客户端判定为不兼容设备）
fn handle_cm(body: &str) -> String {
    let action = extract_action(body);
    match action.as_str() {
        "GetProtocolInfo" => soap_response(
            "GetProtocolInfo",
            SERVICE_CM,
            "<Source></Source><Sink>http-get:*:video/mp4:*,http-get:*:video/x-matroska:*,http-get:*:video/mp2t:*,http-get:*:video/quicktime:*,http-get:*:application/vnd.apple.mpegurl:*,http-get:*:audio/mpeg:*,http-get:*:audio/mp4:*</Sink>",
        ),
        "GetCurrentConnectionIDs" => {
            soap_response("GetCurrentConnectionIDs", SERVICE_CM, "<ConnectionIDs>0</ConnectionIDs>")
        }
        "GetCurrentConnectionInfo" => soap_response(
            "GetCurrentConnectionInfo",
            SERVICE_CM,
            "<RcsID>0</RcsID><AVTransportID>0</AVTransportID><ProtocolInfo>http-get:*:video/mp4:*</ProtocolInfo><PeerConnectionManager></PeerConnectionManager><PeerConnectionID>-1</PeerConnectionID><Direction>Input</Direction><Status>OK</Status>",
        ),
        _ => soap_error("401", "Invalid Action"),
    }
}

/// XML 转义
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// XML 实体反转义（SOAP 参数中的 &amp; 等实体 → 原始字符）。
/// 关键：视频平台（B 站等）投屏 URL 含大量 & 查询参数，
/// 在 SOAP 请求中会以 &amp; 形式传输，不反转义则 URL 无效 → 播放失败（error=-13）
fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

// ===== HTTP 请求分发 =====
fn dispatch(
    app: &AppHandle,
    request: &str,
    uuid: &str,
    port: u16,
    device_name: &str,
    client_ip: &str,
    user_agent: &str,
    announce_ip: &str,
) -> String {
    // 解析请求行
    let mut lines = request.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    // 提取 body（HTTP 请求尾部）
    let body = match request.find("\r\n\r\n") {
        Some(idx) => &request[idx + 4..],
        None => "",
    };

    match (method, path) {
        ("GET", "/description.xml") | ("GET", "/device.xml") => {
            device_description(uuid, port, device_name, announce_ip)
        }
        ("GET", "/upnp/scpd/AVTransport.xml") => avt_scpd(),
        ("GET", "/upnp/scpd/RenderingControl.xml") => rc_scpd(),
        ("GET", "/upnp/scpd/ConnectionManager.xml") => cm_scpd(),
        ("POST", "/upnp/control/AVTransport") => {
            let session = app.state::<DlnaSession>();
            handle_avt(app, body, &session, client_ip, user_agent)
        }
        ("POST", "/upnp/control/RenderingControl") => handle_rc(app, body),
        ("POST", "/upnp/control/ConnectionManager") => handle_cm(body),
        ("SUBSCRIBE", path) if path.starts_with("/upnp/event/") => {
            handle_event_subscribe(app, request)
        }
        ("UNSUBSCRIBE", path) if path.starts_with("/upnp/event/") => {
            handle_event_unsubscribe(app, request)
        }
        ("GET", "/") => http_response(
            "200 OK",
            "text/html",
            "<html><body>ScreenCast Receiver DLNA</body></html>",
        ),
        _ => http_response("404 Not Found", "text/plain", "Not Found"),
    }
}

// ===== GENA 事件订阅（LastChange 通知）=====

/// 提取请求头字段值（从完整请求字符串）
fn extract_header_val(request: &str, name: &str) -> Option<String> {
    let lower_name = name.to_lowercase();
    for line in request.lines() {
        let lower_line = line.to_lowercase();
        if lower_line.starts_with(&lower_name) {
            if let Some(v) = lower_line.splitn(2, ':').nth(1) {
                return Some(v.trim().to_string());
            }
        }
        if line.is_empty() {
            break;
        }
    }
    None
}

/// 处理 SUBSCRIBE（GENA 订阅）：注册订阅者并返回 SID
/// 第三方控制端（BubbleUPnP/VLC 等）投屏前必须订阅成功，否则判定设备不兼容
fn handle_event_subscribe(app: &AppHandle, request: &str) -> String {
    let callback = extract_header_val(request, "CALLBACK")
        .unwrap_or_default()
        .trim_matches('<')
        .trim_matches('>')
        .to_string();
    let nt = extract_header_val(request, "NT").unwrap_or_default();
    if callback.is_empty() || !nt.eq_ignore_ascii_case("upnp:event") {
        return http_response(
            "412 Precondition Failed",
            "text/plain",
            "Missing CALLBACK or NT",
        );
    }
    // 从 callback 提取服务名（如 /upnp/event/AVTransport）
    let sid = format!("uuid:{}", uuid::Uuid::new_v4());
    if let Ok(mut subs) = app.state::<EventSubs>().0.lock() {
        // 同 callback 重复订阅：替换旧 SID
        subs.retain(|s| s.callback != callback);
        subs.push(EventSubscriber {
            sid: sid.clone(),
            callback: callback.clone(),
        });
    }
    println!("[DLNA] 事件订阅: {} -> {}", callback, sid);
    // 订阅成功后立即发送一次当前状态（部分控制端等待 NOTIFY 后才更新 UI 状态）
    let app2 = app.clone();
    let cb = callback.clone();
    let sid2 = sid.clone();
    thread::spawn(move || {
        send_last_change(&app2, &cb, &sid2);
    });
    format!(
        "HTTP/1.1 200 OK\r\nSID: {}\r\nTIMEOUT: Second-1800\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        sid
    )
}

/// 处理 UNSUBSCRIBE：移除订阅者
fn handle_event_unsubscribe(app: &AppHandle, request: &str) -> String {
    let sid = extract_header_val(request, "SID").unwrap_or_default();
    if let Ok(mut subs) = app.state::<EventSubs>().0.lock() {
        subs.retain(|s| s.sid != sid);
    }
    println!("[DLNA] 事件退订: {}", sid);
    http_response("200 OK", "text/plain", "")
}

/// 生成 LastChange 事件 XML（AVTransport 状态）
fn last_change_xml(app: &AppHandle) -> String {
    let paused = mpv_get(app, "pause", "flag").as_bool().unwrap_or(false);
    let state = if paused { "PAUSED_PLAYBACK" } else { "PLAYING" };
    format!(
        r#"<Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/"><InstanceID val="0"><TransportState val="{}"/></InstanceID></Event>"#,
        state
    )
}

/// 向单个订阅者发送 LastChange NOTIFY 事件（POST 到回调地址）
fn send_last_change(app: &AppHandle, callback: &str, sid: &str) {
    let url = callback.trim();
    if !url.starts_with("http://") {
        return;
    }
    let rest = &url[7..];
    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(80)),
        None => (host_port, 80),
    };
    let body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0"><e:property><LastChange>{}</LastChange></e:property></e:propertyset>"#,
        xml_escape(&last_change_xml(app))
    );
    let msg = format!(
        "NOTIFY {} HTTP/1.1\r\nHOST: {}:{}\r\nCONTENT-TYPE: text/xml; charset=\"utf-8\"\r\nNT: upnp:event\r\nNTS: upnp:propchange\r\nSID: {}\r\nSEQ: 0\r\nCONTENT-LENGTH: {}\r\nCONNECTION: close\r\n\r\n{}",
        path,
        host,
        port,
        sid,
        body.len(),
        body
    );
    let _ = std::net::TcpStream::connect((host, port))
        .and_then(|mut s| {
            use std::io::Write;
            s.write_all(msg.as_bytes())?;
            let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(800)));
            let mut tmp = [0u8; 256];
            let _ = s.read(&mut tmp);
            Ok(())
        })
        .map_err(|e| eprintln!("[DLNA] LastChange 事件发送失败: {e}"));
}

/// 向全部订阅者广播 LastChange 事件（SOAP 状态动作后调用）
fn notify_last_change(app: &AppHandle) {
    let subs: Vec<(String, String)> = app
        .state::<EventSubs>()
        .0
        .lock()
        .map(|s| s.iter().map(|e| (e.callback.clone(), e.sid.clone())).collect())
        .unwrap_or_default();
    if subs.is_empty() {
        return;
    }
    for (cb, sid) in subs {
        let app = app.clone();
        let cb = cb.clone();
        let sid = sid.clone();
        thread::spawn(move || send_last_change(&app, &cb, &sid));
    }
}

/// 设备描述 XML（DLNA MediaRenderer 标准描述，含 X_DLNADOC + ConnectionManager）
fn device_description(uuid: &str, port: u16, device_name: &str, announce_ip: &str) -> String {
    let ip = announce_ip;
    let base = format!("http://{}:{}", ip, port);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0" xmlns:dlna="urn:schemas-dlna-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>{}</deviceType>
    <friendlyName>{}</friendlyName>
    <manufacturer>ScreenCast</manufacturer>
    <manufacturerURL>{}</manufacturerURL>
    <modelDescription>DLNA Media Renderer</modelDescription>
    <modelName>ScreenCast Receiver</modelName>
    <modelNumber>1.1</modelNumber>
    <dlna:X_DLNADOC>DMR-1.50</dlna:X_DLNADOC>
    <UDN>uuid:{}</UDN>
    <iconList>
      <icon>
        <mimetype>image/png</mimetype>
        <width>48</width>
        <height>48</height>
        <depth>24</depth>
        <url>/icon.png</url>
      </icon>
    </iconList>
    <presentationURL>{}</presentationURL>
    <serviceList>
      <service>
        <serviceType>{}</serviceType>
        <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
        <controlURL>/upnp/control/AVTransport</controlURL>
        <eventSubURL>/upnp/event/AVTransport</eventSubURL>
        <SCPDURL>/upnp/scpd/AVTransport.xml</SCPDURL>
      </service>
      <service>
        <serviceType>{}</serviceType>
        <serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
        <controlURL>/upnp/control/RenderingControl</controlURL>
        <eventSubURL>/upnp/event/RenderingControl</eventSubURL>
        <SCPDURL>/upnp/scpd/RenderingControl.xml</SCPDURL>
      </service>
      <service>
        <serviceType>{}</serviceType>
        <serviceId>urn:upnp-org:serviceId:ConnectionManager</serviceId>
        <controlURL>/upnp/control/ConnectionManager</controlURL>
        <eventSubURL>/upnp/event/ConnectionManager</eventSubURL>
        <SCPDURL>/upnp/scpd/ConnectionManager.xml</SCPDURL>
      </service>
    </serviceList>
  </device>
</root>"#,
        DEVICE_TYPE,
        xml_escape(device_name),
        base,
        uuid,
        base,
        SERVICE_AVT,
        SERVICE_RC,
        SERVICE_CM
    );
    http_response("200 OK", r#"text/xml; charset="utf-8""#, &xml)
}

/// AVTransport SCPD（声明全部已实现动作 + LastChange 状态变量）
fn avt_scpd() -> String {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <actionList>
    <action><name>SetAVTransportURI</name></action>
    <action><name>SetNextAVTransportURI</name></action>
    <action><name>Play</name></action>
    <action><name>Pause</name></action>
    <action><name>Stop</name></action>
    <action><name>Seek</name></action>
    <action><name>Next</name></action>
    <action><name>Previous</name></action>
    <action><name>GetMediaInfo</name></action>
    <action><name>GetPositionInfo</name></action>
    <action><name>GetTransportInfo</name></action>
    <action><name>GetTransportSettings</name></action>
    <action><name>GetDeviceCapabilities</name></action>
  </actionList>
  <serviceStateTable>
    <stateVariable sendEvents="yes"><name>LastChange</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>AVTransportURI</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>TransportState</name><dataType>string</dataType></stateVariable>
  </serviceStateTable>
</scpd>"#
    );
    http_response("200 OK", r#"text/xml; charset="utf-8""#, &xml)
}

/// RenderingControl SCPD（最小声明）
fn rc_scpd() -> String {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <actionList>
    <action><name>SetVolume</name></action>
    <action><name>GetVolume</name></action>
    <action><name>SetMute</name></action>
    <action><name>GetMute</name></action>
  </actionList>
  <serviceStateTable>
    <stateVariable sendEvents="yes"><name>LastChange</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>Volume</name><dataType>ui2</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>Mute</name><dataType>boolean</dataType></stateVariable>
  </serviceStateTable>
</scpd>"#
    );
    http_response("200 OK", r#"text/xml; charset="utf-8""#, &xml)
}

/// ConnectionManager SCPD（GetProtocolInfo 为 DLNA 投屏前置检查必需）
fn cm_scpd() -> String {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <actionList>
    <action><name>GetProtocolInfo</name></action>
    <action><name>GetCurrentConnectionIDs</name></action>
    <action><name>GetCurrentConnectionInfo</name></action>
  </actionList>
  <serviceStateTable>
    <stateVariable sendEvents="no"><name>SourceProtocolInfo</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>SinkProtocolInfo</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>CurrentConnectionIDs</name><dataType>string</dataType></stateVariable>
  </serviceStateTable>
</scpd>"#
    );
    http_response("200 OK", r#"text/xml; charset="utf-8""#, &xml)
}

// ===== 连接处理 =====
fn handle_conn(
    app: AppHandle,
    mut stream: TcpStream,
    uuid: String,
    port: u16,
    device_name: String,
    announce_ip: String,
) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];

    // 读请求头 + body
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(idx) = find_header_end(&buf) {
                    let header = String::from_utf8_lossy(&buf[..idx]).to_string();
                    let cl = parse_content_length(&header);
                    // 读满 body
                    while buf.len() < idx + 4 + cl {
                        match stream.read(&mut tmp) {
                            Ok(0) => break,
                            Ok(m) => buf.extend_from_slice(&tmp[..m]),
                            Err(_) => break,
                        }
                    }
                    break;
                }
            }
            Err(_) => break,
        }
        if buf.len() > 200_000 {
            break;
        }
    }

    let request = String::from_utf8_lossy(&buf).to_string();
    if request.is_empty() {
        return;
    }
    // 设备图标（二进制 PNG，无法走 String 响应的 dispatch）
    let icon_path = request
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("");
    if icon_path == "/icon.png" {
        use base64::Engine;
        let png = base64::engine::general_purpose::STANDARD
            .decode(ICON_PNG_B64)
            .unwrap_or_default();
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            png.len()
        );
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(&png);
        let _ = stream.flush();
        return;
    }
    // 获取客户端 IP 与 User-Agent（用于投屏请求的设备信息）
    let client_ip = stream
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "未知".to_string());
    let user_agent = extract_header(&request, "User-Agent");

    let response = dispatch(
        &app,
        &request,
        &uuid,
        port,
        &device_name,
        &client_ip,
        &user_agent,
        &announce_ip,
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// 从 HTTP 请求头提取指定头字段值
fn extract_header(request: &str, name: &str) -> String {
    let lower_name = name.to_lowercase();
    for line in request.lines() {
        let lower_line = line.to_lowercase();
        if lower_line.starts_with(&lower_name) {
            if let Some(v) = lower_line.splitn(2, ':').nth(1) {
                return v.trim().to_string();
            }
        }
        // 请求头结束
        if line.is_empty() {
            break;
        }
    }
    String::new()
}

/// 找请求头结束位置（\r\n\r\n）
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// 解析 Content-Length
fn parse_content_length(header: &str) -> usize {
    for line in header.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            if let Some(v) = line.split(':').nth(1) {
                return v.trim().parse().unwrap_or(0);
            }
        }
    }
    0
}

// ===== 启动 =====
/// 启动 DLNA Renderer（SSDP 广播 + HTTP 服务）
/// device_name：投屏设备名称（来自设置，手机上显示）
/// ip_override：用户指定的投屏网卡 IP（设置页选择），None = 自动选择
pub fn start(
    app: AppHandle,
    port_hint: u16,
    device_name: String,
    ip_override: Option<String>,
) -> Result<(), String> {
    // 标记运行中（设置页开关可动态停止，见 stop()）
    RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);

    // 绑定 HTTP 端口（0.0.0.0 = 允许所有来源 IP）
    // 优先使用设置端口，被占用时自动随机绑定空闲端口
    let listener = match TcpListener::bind(("0.0.0.0", port_hint)) {
        Ok(l) => l,
        Err(_) => {
            println!("[DLNA] 端口 {} 被占用，自动选择空闲端口", port_hint);
            TcpListener::bind(("0.0.0.0", 0)).map_err(|e| format!("绑定空闲端口失败: {e}"))?
        }
    };
    let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(port_hint);

    // 保存 HTTP 监听器句柄（stop() 时关闭以结束 http 线程）
    let holder = HTTP_LISTENER.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(mut h) = holder.lock() {
        *h = Some(listener.try_clone().map_err(|e| format!("克隆监听器失败: {e}"))?);
    }

    // 宣告用的局域网 IP（SSDP LOCATION / 设备描述），用户指定优先
    let announce_ip = match &ip_override {
        Some(ip) if !ip.trim().is_empty() => ip.clone(),
        _ => local_ip(),
    };

    // 更新状态（绑定 IP + 端口，供设置界面显示）
    let status_state = app.state::<DlnaStatusState>();
    if let Ok(mut s) = status_state.0.lock() {
        *s = Some(DlnaStatus {
            ip: announce_ip.clone(),
            port: actual_port,
        });
    }

    // 设备 UUID：持久化（每次安装生成一次），保证第三方控制端设备缓存稳定
    let db = app.state::<crate::db::Db>();
    let uuid = crate::db::get_or_create_device_uuid(&db).unwrap_or_else(|e| {
        eprintln!("[DLNA] 读取持久 UUID 失败({e})，使用临时 UUID");
        new_uuid()
    });
    println!(
        "[DLNA] Renderer 已启动, 端口: {}, 设备名: {}, UUID: {}, 宣告IP: {}",
        actual_port, device_name, uuid, announce_ip
    );
    let app2 = app.clone();
    let uuid2 = uuid.clone();
    let name2 = device_name.clone();

    // 记录 byebye 状态（应用退出时发送 ssdp:byebye 通知控制端设备下线）
    if let Ok(mut s) = byebye_state().lock() {
        *s = Some((uuid.clone(), actual_port));
    }

    // SSDP 响应线程（UDP 1900），用实际端口广播
    let ip_ov = ip_override.clone();
    thread::spawn(move || {
        ssdp_loop(uuid2, actual_port, ip_ov);
    });

    // HTTP 服务线程（使用已绑定的 listener）
    thread::spawn(move || {
        http_loop(app2, listener, uuid, name2, announce_ip);
    });

    Ok(())
}

/// 停止 DLNA 服务（设置页开关关闭时调用）：
/// 1. 清空运行标志 → ssdp 循环退出（不再响应 M-SEARCH / 宣告 alive）
/// 2. 发送 ssdp:byebye → 控制端立即移除设备（否则缓存期内手机仍能搜到旧设备）
/// 3. 关闭 HTTP 监听器 → http 线程 accept 失败退出
pub fn stop() {
    if !RUNNING.swap(false, std::sync::atomic::Ordering::SeqCst) {
        return; // 未运行，无需处理
    }
    println!("[DLNA] 服务停止");
    send_byebye();
    if let Ok(mut h) = HTTP_LISTENER.get_or_init(|| std::sync::Mutex::new(None)).lock() {
        *h = None; // drop TcpListener → http 线程退出
    }
    if let Ok(mut s) = byebye_state().lock() {
        *s = None;
    }
}

/// 按最新设置重启 DLNA 服务（设置页修改 开关/设备名/网卡 后调用）
/// 先停止旧服务（含 byebye 下线），再按新设置启动
pub fn restart(
    app: AppHandle,
    port_hint: u16,
    device_name: String,
    ip_override: Option<String>,
) -> Result<(), String> {
    stop();
    start(app, port_hint, device_name, ip_override)
}

/// 应用退出时发送 ssdp:byebye（尽力而为：枚举所有 IPv4 接口宣告设备下线）
pub fn send_byebye() {
    let (uuid, port) = match byebye_state().lock().map(|s| s.clone()) {
        Ok(Some(s)) => s,
        _ => return,
    };
    use socket2::{Domain, Protocol, Socket, Type};
    let multicast: std::net::SocketAddr = "239.255.255.250:1900".parse().unwrap();
    let ifaces = enumerate_v4();
    let mut sent = 0;
    for ip in ifaces {
        let sock = match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // 发送源端口为 1900 的 socket（与系统共存）
        let _ = sock.set_multicast_if_v4(&ip);
        let uuid_nt = format!("uuid:{}", uuid);
        let announcements: [(&str, String); 4] = [
            (DEVICE_TYPE, format!("uuid:{}::{}", uuid, DEVICE_TYPE)),
            (DEVICE_TYPE_2, format!("uuid:{}::{}", uuid, DEVICE_TYPE_2)),
            (
                "upnp:rootdevice",
                format!("uuid:{}::upnp:rootdevice", uuid),
            ),
            (&uuid_nt, uuid_nt.clone()),
        ];
        for (nt, usn) in &announcements {
            let msg = format!(
                "NOTIFY * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nCACHE-CONTROL: max-age=1800\r\nDATE: {}\r\nLOCATION: http://{}:{}/description.xml\r\nNT: {}\r\nNTS: ssdp:byebye\r\nSERVER: Windows/10 UPnP/1.0 DLNADOC/1.50 ScreenCastReceiver/1.2\r\nUSN: {}\r\nBOOTID.UPNP.ORG: 1\r\nCONFIGID.UPNP.ORG: 1\r\n\r\n",
                http_date(),
                ip,
                port,
                nt,
                usn
            );
            if sock
                .send_to(msg.as_bytes(), &socket2::SockAddr::from(multicast))
                .is_ok()
            {
                sent += 1;
            }
        }
    }
    println!("[DLNA] 已发送 ssdp:byebye 下线宣告（{} 个包）", sent);
}

/// 接口专用 SSDP 发送 socket：绑定到具体接口 IP，保证响应/alive 宣告的
/// 源地址与出口接口正确（多网卡机器上默认路由接口 ≠ 手机所在网卡是搜不到的主因）
struct SsdpIfaceSocket {
    ip: std::net::Ipv4Addr,
    sock: std::net::UdpSocket,
}

/// 解析 M-SEARCH 请求中的 ST 头（UPnP 1.0 要求响应 ST 与请求一致）
fn extract_st(msg: &str) -> Option<String> {
    for line in msg.lines() {
        let l = line.trim_start();
        if l.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("st:")) {
            let v = l[3..].trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// 判断接口是否可用于 DLNA 投屏（排除虚拟网卡 / 链路本地地址）。
/// 关键：Tailscale(100.x/10.x)、Hyper-V vEthernet、169.254 链路本地等地址
/// 对局域网手机不可达——第三方严格控制端(cling 等)拿到不可达的 LOCATION 会丢弃设备
fn is_useful_iface(name: &str, ip: &std::net::Ipv4Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    // 169.254.x.x 链路本地（APIPA，无 DHCP 时的自动地址，不可达）
    let oct = ip.octets();
    if oct[0] == 169 && oct[1] == 254 {
        return false;
    }
    let n = name.to_lowercase();
    // 虚拟网卡黑名单（Tailscale / ZeroTier / VM / Hyper-V / Docker / WSL 等）
    // 注意：Hyper-V 虚拟交换机在 Windows 上命名"vEthernet (...)"或"本地连接* N"（带星号）
    const VIRTUAL: [&str; 14] = [
        "tailscale",
        "wintun",
        "zerotier",
        "vmnet",
        "vmware",
        "virtualbox",
        "vethernet",
        "hyper-v",
        "docker",
        "wsl",
        "tun",
        "tap",
        "本地连接*",
        "loopback",
    ];
    !VIRTUAL.iter().any(|v| n.contains(v))
}

/// 枚举所有可用的 IPv4 接口地址（过滤虚拟网卡 / 回环 / 链路本地）
fn enumerate_v4() -> Vec<std::net::Ipv4Addr> {
    let mut iface_ips: Vec<std::net::Ipv4Addr> = Vec::new();
    match local_ip_address::list_afinet_netifas() {
        Ok(ifaces) => {
            for (name, ip) in ifaces {
                if let std::net::IpAddr::V4(v4) = ip {
                    if is_useful_iface(&name, &v4) {
                        println!("[DLNA] 可用接口: {} = {}", name, v4);
                        iface_ips.push(v4);
                    } else {
                        println!("[DLNA] 过滤虚拟接口: {} = {}", name, v4);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("[DLNA] 枚举网络接口失败: {e}");
        }
    }
    iface_ips
}

/// 为每个接口创建专用 SSDP 发送 socket（绑定 接口IP:1900 + set_multicast_if），
/// 保证 M-SEARCH 响应与 alive 宣告的源 IP 是手机可达的接口地址
fn build_iface_socks(iface_ips: &[std::net::Ipv4Addr]) -> Vec<SsdpIfaceSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let mut iface_socks: Vec<SsdpIfaceSocket> = Vec::new();
    for ip in iface_ips {
        let s = match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let _ = s.set_reuse_address(true);
        let bind_addr: std::net::SocketAddr = format!("{}:{}", ip, SSDP_PORT).parse().unwrap();
        if s.bind(&socket2::SockAddr::from(bind_addr)).is_err() {
            // 个别接口（虚拟网卡）可能绑定失败，跳过不影响其他接口
            continue;
        }
        let _ = s.set_multicast_if_v4(ip);
        iface_socks.push(SsdpIfaceSocket {
            ip: *ip,
            sock: s.into(),
        });
    }
    iface_socks
}

/// SSDP 循环：响应 M-SEARCH 请求 + 周期性宣告 alive + 监听网络变化自动重新宣告
/// 使用 socket2 以支持：SO_REUSEADDR（Windows SSDP 服务占用 1900）+ 加入组播组
/// ip_override：用户指定网卡 IP（设置页选择），Some 时只使用该网卡
fn ssdp_loop(uuid: String, port: u16, ip_override: Option<String>) {
    use socket2::{Domain, Protocol, Socket, Type};

    // 接口列表：用户指定时只用指定接口；否则枚举物理接口
    let mut iface_ips: Vec<std::net::Ipv4Addr> = match ip_override.as_deref() {
        Some(ip) if !ip.trim().is_empty() => match ip.trim().parse::<std::net::Ipv4Addr>() {
            Ok(v4) => {
                println!("[DLNA] 使用用户指定网卡: {}", v4);
                vec![v4]
            }
            Err(_) => {
                eprintln!("[DLNA] 用户指定网卡 IP 无效: {}, 回退自动选择", ip);
                enumerate_v4()
            }
        },
        _ => enumerate_v4(),
    };

    // 主接收 socket：0.0.0.0:1900（SO_REUSEADDR 与系统 SSDP 服务共存）
    let socket = match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[DLNA] 创建 SSDP socket 失败: {}", e);
            return;
        }
    };
    let _ = socket.set_reuse_address(true);

    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", SSDP_PORT).parse().unwrap();
    if let Err(e) = socket.bind(&socket2::SockAddr::from(addr)) {
        eprintln!(
            "[DLNA] 无法绑定 UDP {} 端口（可能被 SSDP 服务占用）: {}",
            SSDP_PORT, e
        );
        return;
    }
    // 加入 SSDP 组播组，才能收到控制器广播的 M-SEARCH
    // 关键：多网卡机器（WLAN/VMware/Tailscale 等）必须对每个 IPv4 接口加入组播，
    // 否则手机在特定网卡发出的组播收不到（表现为设备无法被扫描到）
    let group = std::net::Ipv4Addr::new(239, 255, 255, 250);
    let mut joined_interfaces = Vec::new();
    for ip in &iface_ips {
        if socket.join_multicast_v4(&group, ip).is_ok() {
            joined_interfaces.push(ip.to_string());
        }
    }
    if joined_interfaces.is_empty() {
        // 兜底：默认接口
        let _ = socket.join_multicast_v4(&group, &std::net::Ipv4Addr::UNSPECIFIED);
        println!("[DLNA] 警告：未能加入任何接口的组播组");
    }
    println!("[DLNA] 已加入组播的接口: {}", joined_interfaces.join(", "));
    let _ = socket.set_multicast_loop_v4(true);
    let _ = socket.set_read_timeout(Some(std::time::Duration::from_millis(200)));

    let mut iface_socks = build_iface_socks(&iface_ips);
    if iface_socks.is_empty() {
        println!("[DLNA] 警告：未创建任何接口发送 socket，将使用默认路由接口兜底");
    } else {
        let names: Vec<String> = iface_socks.iter().map(|s| s.ip.to_string()).collect();
        println!("[DLNA] 接口发送 socket: {}", names.join(", "));
    }

    let socket: std::net::UdpSocket = socket.into();
    let multicast: std::net::SocketAddr = "239.255.255.250:1900".parse().unwrap();

    // 启动时主动宣告设备存在（ssdp:alive），多数手机 App 依赖 NOTIFY 发现设备
    send_alive_all(&iface_socks, &socket, &uuid, port, &multicast);

    // 周期性重新宣告（每 30 秒）+ 网络变化检测（每 5 秒对比接口列表）
    let mut last_alive = std::time::Instant::now();
    let mut last_net_check = std::time::Instant::now();
    let mut buf = [0u8; 4096];
    while RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
        if last_alive.elapsed() >= std::time::Duration::from_secs(15) {
            send_alive_all(&iface_socks, &socket, &uuid, port, &multicast);
            last_alive = std::time::Instant::now();
        }
        // 网络变化（IP 变更 / 新网卡接入 / 网线断开）：重新加入组播 + 重建发送 socket + 立即宣告
        if last_net_check.elapsed() >= std::time::Duration::from_secs(5) {
            if ip_override.is_none() {
                let cur = enumerate_v4();
                if cur != iface_ips {
                    println!(
                        "[DLNA] 检测到网络接口变化: {:?} → {:?}，重新广播",
                        iface_ips, cur
                    );
                    iface_ips = cur;
                    let mut newly_joined = Vec::new();
                    for ip in &iface_ips {
                        if socket.join_multicast_v4(&group, ip).is_ok() {
                            newly_joined.push(ip.to_string());
                        }
                    }
                    if !newly_joined.is_empty() {
                        println!("[DLNA] 新加入组播的接口: {}", newly_joined.join(", "));
                    }
                    iface_socks = build_iface_socks(&iface_ips);
                    send_alive_all(&iface_socks, &socket, &uuid, port, &multicast);
                }
            }
            last_net_check = std::time::Instant::now();
        }
        match socket.recv_from(&mut buf) {
            Ok((len, addr)) => {
                let msg = String::from_utf8_lossy(&buf[..len]).to_string();
                if msg.contains("M-SEARCH") {
                    // 响应匹配的搜索请求（DLNA 客户端会搜 MediaRenderer / ssdp:all / rootdevice / uuid）
                    let match_target = msg.contains(DEVICE_TYPE)
                        || msg.contains(DEVICE_TYPE_2)
                        || msg.contains("ssdp:all")
                        || msg.contains("upnp:rootdevice")
                        || msg.contains(&format!("uuid:{}", uuid))
                        || msg.contains("urn:schemas-upnp-org:service:AVTransport:1")
                        || msg.contains("urn:schemas-upnp-org:service:RenderingControl:1")
                        || msg.contains("urn:schemas-upnp-org:service:ConnectionManager:1");
                    if match_target {
                        // UPnP 1.0：响应 ST 回显请求值；无 ST 时用设备类型
                        let st = extract_st(&msg).unwrap_or_else(|| DEVICE_TYPE.to_string());
                        send_msearch_response(&iface_socks, &socket, &uuid, port, &st, &addr);
                        // 兼容增强：向搜索来源单播 alive 宣告。
                        // Android 上部分 DLNA 库（cling 类 DMC）依赖 NOTIFY 添加设备，
                        // 且手机未获取 MulticastLock 时收不到组播 NOTIFY，只能收单播响应。
                        // 单播 alive 不依赖 MulticastLock，可显著提高 cling 类 App 的发现率
                        send_alive_to(&iface_socks, &socket, &uuid, port, &addr);
                    }
                }
            }
            Err(_) => {}
        }
    }
}

/// 向指定来源单播发送 alive 宣告（NOTIFY ssdp:alive，目标 = M-SEARCH 请求者地址）。
/// cling 类 DMC 依赖 NOTIFY 将设备加入 Registry，Android 手机未拿 MulticastLock 时
/// 收不到组播 NOTIFY 但能收单播——故收到 M-SEARCH 后除标准响应外再补发单播 alive
fn send_alive_to(
    ifaces: &[SsdpIfaceSocket],
    fallback: &std::net::UdpSocket,
    uuid: &str,
    port: u16,
    to: &std::net::SocketAddr,
) {
    let uuid_nt = format!("uuid:{}", uuid);
    let announcements: [(&str, String); 3] = [
        (
            DEVICE_TYPE,
            format!("uuid:{}::{}", uuid, DEVICE_TYPE),
        ),
        (
            "upnp:rootdevice",
            format!("uuid:{}::upnp:rootdevice", uuid),
        ),
        (&uuid_nt, uuid_nt.clone()),
    ];
    let notify = |ip: &str, nt: &str, usn: &str| {
        format!(
            "NOTIFY * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nCACHE-CONTROL: max-age=1800\r\nDATE: {}\r\nLOCATION: http://{}:{}/description.xml\r\nNT: {}\r\nNTS: ssdp:alive\r\nSERVER: Windows/10 UPnP/1.0 DLNADOC/1.50 ScreenCastReceiver/1.2\r\nUSN: {}\r\nBOOTID.UPNP.ORG: 1\r\nCONFIGID.UPNP.ORG: 1\r\n\r\n",
            http_date(),
            ip,
            port,
            nt,
            usn
        )
    };
    if ifaces.is_empty() {
        for (nt, usn) in &announcements {
            let _ = fallback.send_to(notify(&local_ip(), nt, usn).as_bytes(), to);
        }
        return;
    }
    for s in ifaces {
        let ip = s.ip.to_string();
        for (nt, usn) in &announcements {
            let _ = s.sock.send_to(notify(&ip, nt, usn).as_bytes(), to);
        }
    }
}

/// 通过所有接口发送 SSDP alive 宣告（每个接口一个包，源 IP = 该接口地址），
/// 按 DLNA 标准宣告 rootdevice / 设备类型 / uuid 三种 NT，确保客户端能发现本设备
fn send_alive_all(
    ifaces: &[SsdpIfaceSocket],
    fallback: &std::net::UdpSocket,
    uuid: &str,
    port: u16,
    multicast: &std::net::SocketAddr,
) {
    let uuid_nt = format!("uuid:{}", uuid);
    // (NT, USN)：rootdevice / 设备类型 1.x / 设备类型 2 / uuid（部分客户端只搜 MediaRenderer:2）
    let announcements: [(&str, String); 4] = [
        (
            DEVICE_TYPE,
            format!("uuid:{}::{}", uuid, DEVICE_TYPE),
        ),
        (
            DEVICE_TYPE_2,
            format!("uuid:{}::{}", uuid, DEVICE_TYPE_2),
        ),
        (
            "upnp:rootdevice",
            format!("uuid:{}::upnp:rootdevice", uuid),
        ),
        (&uuid_nt, uuid_nt.clone()),
    ];
    let notify = |ip: &str, nt: &str, usn: &str| {
        format!(
            "NOTIFY * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nCACHE-CONTROL: max-age=1800\r\nDATE: {}\r\nLOCATION: http://{}:{}/description.xml\r\nNT: {}\r\nNTS: ssdp:alive\r\nSERVER: Windows/10 UPnP/1.0 DLNADOC/1.50 ScreenCastReceiver/1.2\r\nUSN: {}\r\nBOOTID.UPNP.ORG: 1\r\nCONFIGID.UPNP.ORG: 1\r\nSEARCHPORT.UPNP.ORG: 1900\r\n\r\n",
            http_date(),
            ip,
            port,
            nt,
            usn
        )
    };
    if ifaces.is_empty() {
        // 兜底：默认路由接口（源 IP 由系统决定）
        for (nt, usn) in &announcements {
            let _ = fallback.send_to(notify(&local_ip(), nt, usn).as_bytes(), multicast);
        }
        return;
    }
    let mut sent = 0;
    for s in ifaces {
        for (nt, usn) in &announcements {
            if s.sock
                .send_to(notify(&s.ip.to_string(), nt, usn).as_bytes(), multicast)
                .is_ok()
            {
                sent += 1;
            }
        }
    }
    println!("[DLNA] 已发送 SSDP alive 宣告（{} 个包）", sent);
}

/// 通过所有接口响应 M-SEARCH：LOCATION 指向对应接口 IP，
/// 手机只会接受自己可达网段的响应包；USN 按请求 ST 类型生成（rootdevice/uuid/设备类型）
/// 按 UPnP 1.0/1.1 标准携带 DATE/EXT/BOOTID 等头；对 ssdp:all 搜索发多个类型响应，
/// 提高各 DLNA App 的兼容性（部分客户端只接受与自身搜索目标一致的 ST 响应）
fn send_msearch_response(
    ifaces: &[SsdpIfaceSocket],
    fallback: &std::net::UdpSocket,
    uuid: &str,
    port: u16,
    st: &str,
    to: &std::net::SocketAddr,
) {
    // 按请求 ST 决定 USN 格式（UPnP 1.0 规范）
    let usn = |target: &str| -> String {
        if target.eq_ignore_ascii_case("upnp:rootdevice") {
            format!("uuid:{}::upnp:rootdevice", uuid)
        } else if target.starts_with("uuid:") {
            target.to_string()
        } else if target.eq_ignore_ascii_case(DEVICE_TYPE_2) {
            format!("uuid:{}::{}", uuid, DEVICE_TYPE_2)
        } else {
            format!("uuid:{}::{}", uuid, target)
        }
    };
    let response = |ip: &str, target: &str, usn_v: &str| {
        format!(
            "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nDATE: {}\r\nEXT:\r\nLOCATION: http://{}:{}/description.xml\r\nSERVER: Windows/10 UPnP/1.0 DLNADOC/1.50 ScreenCastReceiver/1.2\r\nST: {}\r\nUSN: {}\r\nBOOTID.UPNP.ORG: 1\r\nCONFIGID.UPNP.ORG: 1\r\nSEARCHPORT.UPNP.ORG: 1900\r\n\r\n",
            http_date(),
            ip,
            port,
            target,
            usn_v
        )
    };
    // ssdp:all 搜索：广播本设备所有类型（rootdevice / MediaRenderer:1 / MediaRenderer:2 / uuid）
    let targets: Vec<(String, String)> = if st.eq_ignore_ascii_case("ssdp:all") {
        vec![
            ("upnp:rootdevice".to_string(), usn("upnp:rootdevice")),
            (DEVICE_TYPE.to_string(), usn(DEVICE_TYPE)),
            (DEVICE_TYPE_2.to_string(), usn(DEVICE_TYPE_2)),
            (format!("uuid:{}", uuid), format!("uuid:{}", uuid)),
        ]
    } else {
        vec![(st.to_string(), usn(st))]
    };
    let send_one = |ip: &str, t: &str, u: &str| {
        let _ = if ifaces.is_empty() {
            fallback.send_to(response(ip, t, u).as_bytes(), to)
        } else {
            // 找到对应接口的发送 socket（同 ip），找不到则用 fallback
            let s = ifaces.iter().find(|s| s.ip.to_string() == ip);
            match s {
                Some(s) => s.sock.send_to(response(ip, t, u).as_bytes(), to),
                None => fallback.send_to(response(ip, t, u).as_bytes(), to),
            }
        };
    };
    if ifaces.is_empty() {
        // 兜底：默认路由接口
        let ip = local_ip();
        for (t, u) in &targets {
            send_one(&ip, t, u);
        }
        return;
    }
    for s in ifaces {
        let ip = s.ip.to_string();
        for (t, u) in &targets {
            let _ = s.sock.send_to(response(&ip, t, u).as_bytes(), to);
        }
    }
}

/// HTTP 循环：接受 TCP 连接（listener 已在 start 中绑定）
fn http_loop(
    app: AppHandle,
    listener: TcpListener,
    uuid: String,
    device_name: String,
    announce_ip: String,
) {
    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let app = app.clone();
            let uuid = uuid.clone();
            let name = device_name.clone();
            let ip = announce_ip.clone();
            let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
            thread::spawn(move || handle_conn(app, stream, uuid, port, name, ip));
        }
    }
}
