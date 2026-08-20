// 数据库模块（rusqlite）
// 数据库文件位于可执行文件同级目录下的 config/ 文件夹（用户需求：配置统一持久化到安装目录 config/）
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

/// 全局数据库连接
pub struct Db(pub Mutex<Connection>);

/// 播放进度信息
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProgressInfo {
    pub url: String,
    pub title: Option<String>,
    pub position: f64,
    pub duration: Option<f64>,
    pub updated_at: i64,
    pub completed: bool,
}

/// 获取数据库文件路径：{exe_dir}/config/screencast.db
pub fn get_db_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let config_dir = exe_dir.join("config");
    let _ = std::fs::create_dir_all(&config_dir); // 创建 config 目录
    config_dir.join("screencast.db")
}

/// 初始化数据库连接与表结构
pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let path = get_db_path();
    let conn = Connection::open(&path).map_err(|e| format!("打开数据库失败: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS playback_history (
            url TEXT PRIMARY KEY,
            title TEXT,
            last_position REAL NOT NULL,
            duration REAL,
            updated_at INTEGER NOT NULL,
            completed INTEGER DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("初始化数据库表失败: {e}"))?;
    app.manage(Db(Mutex::new(conn)));
    Ok(())
}

/// 当前时间戳（Unix 秒）
fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 保存播放进度（不存在则插入，存在则更新）
pub fn save_progress(
    db: &Db,
    url: &str,
    title: Option<&str>,
    position: f64,
    duration: Option<f64>,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO playback_history (url, title, last_position, duration, updated_at, completed)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)
         ON CONFLICT(url) DO UPDATE SET
            title = ?2, last_position = ?3, duration = ?4, updated_at = ?5, completed = 0",
        params![url, title, position, duration, now()],
    )
    .map_err(|e| format!("保存进度失败: {e}"))?;
    Ok(())
}

/// 获取播放进度。
/// - completed=1 或 进度超过 95% 视为播放完成 → 清除记录并返回 None（从头播放）
pub fn get_progress(db: &Db, url: &str) -> Result<Option<ProgressInfo>, String> {
    // 先查记录（锁在作用域内自动释放，避免嵌套锁死）
    let info = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT url, title, last_position, duration, updated_at, completed
                 FROM playback_history WHERE url = ?1",
            )
            .map_err(|e| format!("查询进度失败: {e}"))?;
        let row = stmt
            .query_row(params![url], |r| {
                Ok(ProgressInfo {
                    url: r.get(0)?,
                    title: r.get(1)?,
                    position: r.get(2)?,
                    duration: r.get(3)?,
                    updated_at: r.get(4)?,
                    completed: r.get::<_, i64>(5)? != 0,
                })
            })
            .optional()
            .map_err(|e| format!("查询进度失败: {e}"))?;
        row
    };

    match info {
        Some(prog) if prog.completed => Ok(None), // 已完成，从头播放
        Some(prog) => {
            // 进度超过 95% 视为播放完成，清除记录
            if let Some(d) = prog.duration {
                if d > 0.0 && prog.position / d >= 0.95 {
                    clear_progress(db, &prog.url)?;
                    return Ok(None);
                }
            }
            Ok(Some(prog))
        }
        None => Ok(None),
    }
}

/// 清除播放进度
pub fn clear_progress(db: &Db, url: &str) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM playback_history WHERE url = ?1", params![url])
        .map_err(|e| format!("清除进度失败: {e}"))?;
    Ok(())
}

/// 获取播放历史（最多 limit 条，按更新时间倒序）
pub fn list_progress(db: &Db, limit: i64) -> Result<Vec<ProgressInfo>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT url, title, last_position, duration, updated_at, completed
             FROM playback_history ORDER BY updated_at DESC LIMIT ?1",
        )
        .map_err(|e| format!("查询历史失败: {e}"))?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(ProgressInfo {
                url: r.get(0)?,
                title: r.get(1)?,
                position: r.get(2)?,
                duration: r.get(3)?,
                updated_at: r.get(4)?,
                completed: r.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|e| format!("查询历史失败: {e}"))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| format!("读取历史失败: {e}"))?);
    }
    Ok(result)
}

/// 裁剪播放历史，只保留最近 N 条（超出淘汰最旧）
pub fn trim_progress(db: &Db, keep: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM playback_history WHERE url IN (
            SELECT url FROM playback_history
            ORDER BY updated_at DESC LIMIT -1 OFFSET ?1
        )",
        params![keep],
    )
    .map_err(|e| format!("裁剪历史失败: {e}"))?;
    Ok(())
}

/// 读写设置值
pub fn set_setting(db: &Db, key: &str, value: &str) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, value],
    )
    .map_err(|e| format!("保存设置失败: {e}"))?;
    Ok(())
}

/// 读取设置值
pub fn get_setting(db: &Db, key: &str) -> Result<Option<String>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let result: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("读取设置失败: {e}"))?;
    Ok(result)
}

/// 获取或创建持久化设备 UUID（DLNA 设备 UUID 每次安装生成一次，保证客户端缓存稳定）
pub fn get_or_create_device_uuid(db: &Db) -> Result<String, String> {
    if let Some(u) = get_setting(db, "dlna_device_uuid")? {
        if !u.trim().is_empty() {
            return Ok(u);
        }
    }
    let u = uuid::Uuid::new_v4().to_string();
    set_setting(db, "dlna_device_uuid", &u)?;
    println!("[DLNA] 已生成并持久化设备 UUID: {}", u);
    Ok(u)
}
