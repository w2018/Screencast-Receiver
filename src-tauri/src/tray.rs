// 系统托盘逻辑（F5）
// 功能：托盘图标 + 左键切换显示/隐藏 + 右键菜单（显示主窗口 / 退出）
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

/// 创建系统托盘
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    // 托盘右键菜单
    let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show_item, &quit_item])
        .build()?;

    // 托盘图标：显式从打包的 icons 加载（默认图标会显示占位符）
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false) // 左键不弹菜单，用于切换窗口
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 左键单击：切换显示/隐藏窗口
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// 显示主窗口（从托盘恢复）
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        // 通知前端重新初始化 MPV（关闭到托盘时实例会被销毁）
        let _ = app.emit("tray-show", ());
    }
}

/// 切换主窗口显示/隐藏
fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                let _ = window.hide();
            }
            _ => show_window(app),
        }
    }
}
