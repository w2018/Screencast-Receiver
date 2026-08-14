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
const SERVICE_AVT: &str = "urn:schemas-upnp-org:service:AVTransport:1";
const SERVICE_RC: &str = "urn:schemas-upnp-org:service:RenderingControl:1";
const SSDP_PORT: u16 = 1900;

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

// ===== 工具函数 =====

/// 获取本机局域网 IP
fn local_ip() -> String {
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
            if let Some(uri) = extract_param(body, "CurrentURI") {
                // 保存会话（待用户确认后播放）
                if let Ok(mut s) = session.0.lock() {
                    *s = uri.clone();
                }
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
            soap_response("SetAVTransportURI", SERVICE_AVT, "")
        }
        "Play" => {
            mpv_set(app, "pause", json!(false));
            println!("[DLNA] Play");
            soap_response("Play", SERVICE_AVT, "")
        }
        "Pause" => {
            mpv_set(app, "pause", json!(true));
            println!("[DLNA] Pause");
            soap_response("Pause", SERVICE_AVT, "")
        }
        "Stop" => {
            mpv_cmd(app, "stop", vec![]);
            println!("[DLNA] Stop");
            soap_response("Stop", SERVICE_AVT, "")
        }
        "Seek" => {
            let target = extract_param(body, "Target").unwrap_or_default();
            let secs = parse_duration(&target);
            mpv_cmd(app, "seek", vec![json!(secs), json!("absolute")]);
            println!("[DLNA] Seek to {}", target);
            soap_response("Seek", SERVICE_AVT, "")
        }
        "Next" | "Previous" => soap_response(&action, SERVICE_AVT, ""),
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

/// XML 转义
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
        ("GET", "/description.xml") => device_description(uuid, port, device_name),
        ("GET", "/upnp/scpd/AVTransport.xml") => avt_scpd(),
        ("GET", "/upnp/scpd/RenderingControl.xml") => rc_scpd(),
        ("POST", "/upnp/control/AVTransport") => {
            let session = app.state::<DlnaSession>();
            handle_avt(app, body, &session, client_ip, user_agent)
        }
        ("POST", "/upnp/control/RenderingControl") => handle_rc(app, body),
        ("GET", "/") => http_response(
            "200 OK",
            "text/html",
            "<html><body>ScreenCast Receiver DLNA</body></html>",
        ),
        _ => http_response("404 Not Found", "text/plain", "Not Found"),
    }
}

/// 设备描述 XML（文档附录 D 模板）
fn device_description(uuid: &str, port: u16, device_name: &str) -> String {
    let ip = local_ip();
    let base = format!("http://{}:{}", ip, port);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>{}</deviceType>
    <friendlyName>{}</friendlyName>
    <manufacturer>ScreenCast</manufacturer>
    <manufacturerURL>{}</manufacturerURL>
    <modelDescription>DLNA Media Renderer</modelDescription>
    <modelName>ScreenCast Receiver</modelName>
    <modelNumber>1.0</modelNumber>
    <UDN>uuid:{}</UDN>
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
    </serviceList>
  </device>
</root>"#,
        DEVICE_TYPE,
        xml_escape(device_name),
        base,
        uuid,
        SERVICE_AVT,
        SERVICE_RC
    );
    http_response("200 OK", r#"text/xml; charset="utf-8""#, &xml)
}

/// AVTransport SCPD（最小声明）
fn avt_scpd() -> String {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <actionList>
    <action><name>SetAVTransportURI</name></action>
    <action><name>Play</name></action>
    <action><name>Pause</name></action>
    <action><name>Stop</name></action>
    <action><name>Seek</name></action>
    <action><name>GetPositionInfo</name></action>
    <action><name>GetTransportInfo</name></action>
  </actionList>
  <serviceStateTable></serviceStateTable>
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
  <serviceStateTable></serviceStateTable>
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
pub fn start(app: AppHandle, port_hint: u16, device_name: String) -> Result<(), String> {
    // 注册会话状态
    app.manage(DlnaSession(Mutex::new(String::new())));

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

    // 更新状态（绑定 IP + 端口，供设置界面显示）
    let status_state = app.state::<DlnaStatusState>();
    if let Ok(mut s) = status_state.0.lock() {
        *s = Some(DlnaStatus {
            ip: local_ip(),
            port: actual_port,
        });
    }

    let uuid = new_uuid();
    println!(
        "[DLNA] Renderer 已启动, 端口: {}, 设备名: {}, UUID: {}",
        actual_port, device_name, uuid
    );
    let app2 = app.clone();
    let uuid2 = uuid.clone();
    let name2 = device_name.clone();

    // SSDP 响应线程（UDP 1900），用实际端口广播
    thread::spawn(move || {
        ssdp_loop(uuid2, actual_port);
    });

    // HTTP 服务线程（使用已绑定的 listener）
    thread::spawn(move || {
        http_loop(app2, listener, uuid, name2);
    });

    Ok(())
}

/// SSDP 循环：响应 M-SEARCH 请求
/// 使用 socket2 以支持：SO_REUSEADDR（Windows SSDP 服务占用 1900）+ 加入组播组
fn ssdp_loop(uuid: String, port: u16) {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[DLNA] 创建 SSDP socket 失败: {}", e);
            return;
        }
    };
    // 允许端口复用（Windows 系统 SSDP 服务可能占用 1900）
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
    match local_ip_address::list_afinet_netifas() {
        Ok(ifaces) => {
            for (_name, ip) in ifaces {
                if let std::net::IpAddr::V4(v4) = ip {
                    if socket.join_multicast_v4(&group, &v4).is_ok() {
                        joined_interfaces.push(v4.to_string());
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("[DLNA] 枚举网络接口失败: {e}");
            let _ = socket.join_multicast_v4(&group, &std::net::Ipv4Addr::UNSPECIFIED);
        }
    }
    println!("[DLNA] 已加入组播的接口: {}", joined_interfaces.join(", "));
    let _ = socket.set_multicast_loop_v4(true);
    let _ = socket.set_read_timeout(Some(std::time::Duration::from_millis(200)));

    let socket: std::net::UdpSocket = socket.into();
    let multicast: std::net::SocketAddr = "239.255.255.250:1900".parse().unwrap();

    // 启动时主动宣告设备存在（ssdp:alive），多数手机 App 依赖 NOTIFY 发现设备
    send_alive(&socket, &uuid, port, &multicast);

    let mut notify_tick: u32 = 0;
    let mut buf = [0u8; 4096];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, addr)) => {
                let msg = String::from_utf8_lossy(&buf[..len]).to_string();
                if msg.contains("M-SEARCH") {
                    // 响应匹配的搜索请求
                    let match_target = msg.contains(DEVICE_TYPE)
                        || msg.contains("ssdp:all")
                        || msg.contains("urn:schemas-upnp-org:service:AVTransport:1")
                        || msg.contains("urn:schemas-upnp-org:service:RenderingControl:1");
                    if match_target {
                        let ip = local_ip();
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nEXT:\r\nLOCATION: http://{}:{}/description.xml\r\nSERVER: Windows/10 UPnP/1.0 ScreenCastReceiver/1.0\r\nST: {}\r\nUSN: uuid:{}::{}\r\n\r\n",
                            ip, port, DEVICE_TYPE, uuid, DEVICE_TYPE
                        );
                        let _ = socket.send_to(response.as_bytes(), addr);
                    }
                }
            }
            Err(_) => {
                // 读超时（200ms）：周期性重新宣告 alive（每约 30 秒）
                notify_tick += 1;
                if notify_tick % 150 == 0 {
                    send_alive(&socket, &uuid, port, &multicast);
                }
            }
        }
    }
}

/// 发送 SSDP alive 宣告到组播（让控制器主动发现本设备）
fn send_alive(
    socket: &std::net::UdpSocket,
    uuid: &str,
    port: u16,
    multicast: &std::net::SocketAddr,
) {
    let ip = local_ip();
    let notify = format!(
        "NOTIFY * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nCACHE-CONTROL: max-age=1800\r\nLOCATION: http://{}:{}/description.xml\r\nNT: {}\r\nNTS: ssdp:alive\r\nSERVER: Windows/10 UPnP/1.0 ScreenCastReceiver/1.0\r\nUSN: uuid:{}::{}\r\n\r\n",
        ip, port, DEVICE_TYPE, uuid, DEVICE_TYPE
    );
    let _ = socket.send_to(notify.as_bytes(), multicast);
    println!("[DLNA] 已发送 SSDP alive 宣告");
}

/// HTTP 循环：接受 TCP 连接（listener 已在 start 中绑定）
fn http_loop(app: AppHandle, listener: TcpListener, uuid: String, device_name: String) {
    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let app = app.clone();
            let uuid = uuid.clone();
            let name = device_name.clone();
            let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
            thread::spawn(move || handle_conn(app, stream, uuid, port, name));
        }
    }
}
