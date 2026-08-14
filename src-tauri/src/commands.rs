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
