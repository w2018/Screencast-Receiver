// Tauri commands：数据库操作封装（播放进度 + 设置）
use tauri::State;

use crate::db::{self, Db, ProgressInfo};
use crate::dlna_renderer::{DlnaStatus, DlnaStatusState};

/// 保存播放进度
#[tauri::command]
pub fn save_progress(
    state: State<Db>,
    url: String,
    title: Option<String>,
    position: f64,
    duration: Option<f64>,
) -> Result<(), String> {
    db::save_progress(&state, &url, title.as_deref(), position, duration)
}

/// 获取播放进度（已完成返回 None）
#[tauri::command]
pub fn get_progress(state: State<Db>, url: String) -> Result<Option<ProgressInfo>, String> {
    db::get_progress(&state, &url)
}

/// 清除播放进度
#[tauri::command]
pub fn clear_progress(state: State<Db>, url: String) -> Result<(), String> {
    db::clear_progress(&state, &url)
}

/// 获取播放历史（先裁剪到最近 100 条）
#[tauri::command]
pub fn list_progress(state: State<Db>) -> Result<Vec<ProgressInfo>, String> {
    db::trim_progress(&state, 100)?;
    db::list_progress(&state, 100)
}

/// 写入设置值
#[tauri::command]
pub fn set_setting(state: State<Db>, key: String, value: String) -> Result<(), String> {
    db::set_setting(&state, &key, &value)
}

/// 读取设置值
#[tauri::command]
pub fn get_setting(state: State<Db>, key: String) -> Result<Option<String>, String> {
    db::get_setting(&state, &key)
}

/// 根据投屏 URL 恢复防盗链请求头（托盘恢复播放时 mpv 实例重建，Referer 会丢失）
#[tauri::command]
pub fn setup_stream_headers_command(app: tauri::AppHandle, uri: String) {
    crate::dlna_renderer::setup_stream_headers(&app, &uri);
}

/// 获取 DLNA 服务当前绑定的 IP 和端口（设置界面显示）
#[tauri::command]
pub fn get_dlna_status(state: State<DlnaStatusState>) -> Result<Option<DlnaStatus>, String> {
    Ok(state.0.lock().map_err(|e| e.to_string())?.clone())
}

/// 生成完整的防火墙放行命令（含实际 exe 路径），供用户复制到管理员命令行执行
#[tauri::command]
pub fn get_firewall_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.display().to_string();
    Ok(format!(
        "netsh advfirewall firewall add rule name=\"投屏助手\" dir=in action=allow program=\"{}\" enable=yes",
        exe_str
    ))
}

/// 检查防火墙是否已放行本程序（按程序名查询规则）
/// 用 CREATE_NO_WINDOW 隐藏 netsh 控制台窗口，避免设置页打开时闪黑框
#[tauri::command]
pub fn check_firewall_rule() -> Result<bool, String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            "name=\"投屏助手\"",
        ])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW：隐藏控制台窗口
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let lower = text.to_lowercase();
    // 规则存在且操作为允许（中文系统输出"允许"，英文输出"Allow"）
    Ok(text.contains("投屏助手")
        && (text.contains("允许") || lower.contains("allow")))
}

/// 一键放行防火墙：通过 UAC 提权执行 netsh 添加本程序的入站允许规则
/// （覆盖 UDP 1900 SSDP 发现 + TCP 随机端口设备描述/视频流）
#[tauri::command]
pub fn add_firewall_rule() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.display().to_string();
    let args = format!(
        "advfirewall firewall add rule name=\"投屏助手\" dir=in action=allow program=\"{}\" enable=yes",
        exe_str
    );
    let to_wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let verb = to_wide("runas");
    let file = to_wide("netsh.exe");
    let params = to_wide(&args);
    let dir = to_wide("");
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            dir.as_ptr(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
        )
    };
    // ShellExecuteW 返回值 > 32 表示成功启动
    if result as isize <= 32 {
        Err("未能启动管理员授权（可能被取消）。请复制下方命令，以管理员身份手动执行。".to_string())
    } else {
        Ok(())
    }
}
