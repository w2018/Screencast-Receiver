// 网络视频播放（远程控制）：
// - 内置 HTTP 服务：手机扫码打开网页，输入网络视频 URL 后提交
// - 后端收到 /play 请求 → emit "remote-play-request" → 前端自动播放
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// 远程播放服务信息（供前端弹窗展示 IP/端口/链接）
#[derive(Clone, serde::Serialize)]
pub struct RemoteInfo {
    pub ip: String,
    pub port: u16,
    pub url: String,
}

/// 服务状态：None = 未启动
pub struct RemoteState(pub Mutex<Option<RemoteInfo>>);

/// 二维码/弹窗展示用的局域网地址
fn service_url(ip: &str, port: u16) -> String {
    format!("http://{}:{}/", ip, port)
}

/// 简易 HTTP 响应
fn http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        content_type,
        body.as_bytes().len(),
        body
    )
}

/// 手机端操作页面（HTML）
fn control_page(ip: &str, port: u16) -> String {
    let addr = service_url(ip, port);
    format!(
        r#"<!DOCTYPE html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>投屏助手 · 网络视频播放</title>
<style>
body{{font-family:-apple-system,'Segoe UI',sans-serif;background:#0f1117;color:#eaeaea;margin:0;padding:24px;display:flex;flex-direction:column;min-height:100vh;box-sizing:border-box}}
h2{{margin:0 0 6px;font-size:22px;color:#818cf8}}
p.desc{{margin:0 0 20px;color:#a0a0b0;font-size:14px}}
input{{width:100%;box-sizing:border-box;padding:14px;border-radius:10px;border:1px solid #3a3f4d;background:#1a1d27;color:#eaeaea;font-size:16px;outline:none}}
input:focus{{border-color:#6366f1}}
button{{margin-top:14px;width:100%;padding:14px;border:none;border-radius:10px;background:#6366f1;color:#fff;font-size:16px;cursor:pointer}}
button:active{{opacity:.85}}
#s{{margin-top:14px;font-size:14px;min-height:20px;color:#a0a0b0;word-break:break-all}}
.foot{{margin-top:auto;padding-top:20px;font-size:12px;color:#6b7280;text-align:center}}
</style>
</head>
<body>
<h2>投屏助手</h2>
<p class="desc">输入网络视频 URL（http/https），点击播放后电脑端自动播放</p>
<input id="u" type="url" placeholder="https://example.com/video.mp4" autocomplete="off">
<button id="b">播 放</button>
<div id="s"></div>
<div class="foot">服务地址：{addr}</div>
<script>
function go() {{
  var u = document.getElementById('u').value.trim();
  var s = document.getElementById('s');
  if (!u) {{ s.textContent = '请输入 URL'; return; }}
  if (!/^https?:\/\//i.test(u)) {{ s.textContent = 'URL 需以 http:// 或 https:// 开头'; return; }}
  s.textContent = '发送中…';
  fetch('/play', {{
    method: 'POST',
    headers: {{'Content-Type': 'application/json'}},
    body: JSON.stringify({{url: u}})
  }}).then(function(r) {{ return r.json(); }}).then(function(j) {{
    s.textContent = j.ok ? '已发送，电脑端开始播放' : ('发送失败：' + (j.error || ''));
  }}).catch(function(e) {{ s.textContent = '发送失败：' + e; }});
}}
document.getElementById('b').onclick = go;
document.getElementById('u').addEventListener('keydown', function(e) {{ if (e.key === 'Enter') go(); }});
</script>
</body>
</html>"#,
        addr = addr
    )
}

/// 处理单个 HTTP 连接
fn handle_conn(app: AppHandle, mut stream: std::net::TcpStream) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(idx) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    // 读取 Content-Length 补全 body
                    let header = String::from_utf8_lossy(&buf[..idx]).to_string();
                    let cl = header
                        .lines()
                        .find_map(|l| {
                            let l = l.to_lowercase();
                            l.strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    let body_start = idx + 4;
                    while buf.len() < body_start + cl {
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
        if buf.len() > 100_000 {
            break;
        }
    }

    let request = String::from_utf8_lossy(&buf).to_string();
    let first_line = request.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    let resp = match (method, path) {
        ("GET", "/") => {
            // 从状态中读取当前 IP/端口展示页面地址
            let ip = app
                .state::<RemoteState>()
                .0
                .lock()
                .map(|s| s.as_ref().map(|i| i.ip.clone()).unwrap_or_else(|| "127.0.0.1".to_string()))
                .unwrap_or_else(|_| "127.0.0.1".to_string());
            let port = app
                .state::<RemoteState>()
                .0
                .lock()
                .ok()
                .and_then(|s| s.as_ref().map(|i| i.port))
                .unwrap_or(0);
            http_response("200 OK", r#"text/html; charset="utf-8""#, &control_page(&ip, port))
        }
        ("POST", "/play") => {
            // 解析 body JSON {url}
            let body = request
                .split_once("\r\n\r\n")
                .map(|(_, b)| b.to_string())
                .unwrap_or_default();
            let url = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()))
                .unwrap_or_default();
            if url.is_empty() || !(url.starts_with("http://") || url.starts_with("https://")) {
                http_response(
                    "400 Bad Request",
                    "application/json",
                    r#"{"ok":false,"error":"URL 需以 http:// 或 https:// 开头"}"#,
                )
            } else {
                println!("[REMOTE] 收到播放请求: {}", url);
                let _ = app.emit("remote-play-request", url);
                http_response("200 OK", "application/json", r#"{"ok":true}"#)
            }
        }
        _ => http_response(
            "404 Not Found",
            "text/plain",
            "404 Not Found",
        ),
    };
    let _ = stream.write_all(resp.as_bytes());
}

/// 启动远程播放 HTTP 服务（随机空闲端口）；已在运行时直接返回现有信息
pub fn start(app: AppHandle) -> Result<RemoteInfo, String> {
    let state = app.state::<RemoteState>();
    // 已在运行：复用
    if let Ok(guard) = state.0.lock() {
        if let Some(info) = guard.as_ref() {
            return Ok(info.clone());
        }
    }
    let listener = TcpListener::bind(("0.0.0.0", 0)).map_err(|e| format!("绑定端口失败: {e}"))?;
    let port = listener
        .local_addr()
        .map(|a| a.port())
        .map_err(|e| format!("获取端口失败: {e}"))?;
    let ip = crate::dlna_renderer::local_ip();
    let info = RemoteInfo {
        ip: ip.clone(),
        port,
        url: service_url(&ip, port),
    };
    println!("[REMOTE] 网络视频播放服务已启动: {}", info.url);
    {
        let mut guard = state
            .0
            .lock()
            .map_err(|_| "状态锁失败".to_string())?;
        *guard = Some(info.clone());
    }

    std::thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let app2 = app.clone();
                    std::thread::spawn(move || handle_conn(app2, stream));
                }
                Err(_) => break,
            }
        }
    });
    Ok(info)
}

/// 供前端弹窗调用的命令：获取（或启动）远程播放服务信息
#[tauri::command]
pub fn start_remote_play_server(app: AppHandle) -> Result<RemoteInfo, String> {
    start(app)
}