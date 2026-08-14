// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod db;
mod dlna_renderer;
mod tray;

use tauri::{Emitter, Manager};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// 前端日志转发：让前端关键状态打印到 tauri dev 终端，便于自动化验证
#[tauri::command]
fn log_frontend(msg: String) {
    println!("[FRONTEND] {}", msg);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        // 单实例：重复启动时聚焦已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_libmpv::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            log_frontend,
            commands::save_progress,
            commands::get_progress,
            commands::clear_progress,
            commands::list_progress,
            commands::set_setting,
            commands::get_setting,
            commands::get_dlna_status,
            commands::get_firewall_command
        ])
        .setup(|app| {
            println!("[SETUP] 应用初始化中...");
            // 初始化数据库（config/ 目录下）
            db::init(app.handle())?;
            // 创建系统托盘
            tray::create_tray(app.handle())?;

            // DLNA 状态存储（供设置界面显示绑定 IP/端口）
            app.manage(dlna_renderer::DlnaStatusState(std::sync::Mutex::new(None)));

            // 启动 DLNA Renderer（读取设置）
            let dlna_enabled = {
                let db = app.state::<db::Db>();
                db::get_setting(&db, "dlnaEnabled")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(true)
            };
            let dlna_name = {
                let db = app.state::<db::Db>();
                db::get_setting(&db, "deviceName")
                    .ok()
                    .flatten()
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "投屏助手".to_string())
            };
            if dlna_enabled {
                // 端口 0 = 系统随机分配空闲端口（随机监听，非固定）
                dlna_renderer::start(app.handle().clone(), 0, dlna_name)?;
            } else {
                println!("[DLNA] 已通过设置禁用");
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭窗口时最小化到托盘（读取设置：minimizeToTray）
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                // 读取"关闭到托盘"设置，若关闭则真正退出
                let minimize_setting = {
                    let db = app.state::<db::Db>();
                    db::get_setting(&db, "minimizeToTray").unwrap_or(None)
                };
                if minimize_setting.as_deref() == Some("false") {
                    println!("[TRAY] 关闭到托盘已关闭，直接退出");
                    return; // 不拦截，窗口真正关闭
                }
                println!("[TRAY] 收到关闭请求，隐藏到托盘");
                let _ = window.hide();
                println!("[TRAY] hide 调用完成, 可见={}", window.is_visible().unwrap_or(false));
                let _ = app.emit("app-hidden", ());
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
