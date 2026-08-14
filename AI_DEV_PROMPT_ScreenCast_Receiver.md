# AI 软件开发任务指令文档 —— Windows 桌面投屏接收端

> **版本**：v1.0  
> **目标平台**：Windows 10 / 11（x86_64 优先，兼容 x86）  
> **技术栈**：Rust + Tauri 2 + MPV + React/TypeScript  
> **阅读对象**：执行本项目的 AI 编程助手  
> **核心原则**：**禁止幻觉、禁止偷懒、所有功能必须经过真实测试后才能交付。**

---

## 第一节：AI 角色与行为铁律

### 1.1 你的角色

你是一名 **执行型高级软件工程师**。你的任务不是"讨论方案"，而是**按照本文档的规格，逐行写出可编译、可运行、可测试通过的代码**。

### 1.2 铁律（不可违反）

| 编号 | 铁律内容 |
|------|---------|
| **R1** | 每个功能模块写完后，**必须**在 Windows 环境下编译并通过功能测试，才能标记为"完成" |
| **R2** | **禁止编造**任何 API、函数签名、库版本、命令行参数。如果不确定某个 API 是否存在，必须先查证（查看官方文档/crates.io/docs.rs），确认存在后再使用 |
| **R3** | **禁止**用"理论上可行""应该可以"等模糊表述来冒充测试通过。要么有真实的编译/运行日志，要么明确说"未验证" |
| **R4** | 所有外部依赖（crate、npm 包、dll、exe）必须明确写出**具体版本号**，不得使用 `latest` 或省略版本 |
| **R5** | 代码中出现任何 `unsafe` 块，必须附带**安全说明注释**，解释为什么安全、不变量是什么 |
| **R6** | 测试失败时，**必须先修复代码**，不得修改测试去凑通过。如果测试本身有 bug，需明确说明并修正测试 |
| **R7** | 每次交付前，必须逐项填写本文档末尾的 **「交付检查清单」**（第九节），不得跳过 |

### 1.3 禁止行为清单

- ❌ 禁止声称"已安装依赖"但没真正执行安装命令
- ❌ 禁止声称"编译通过"但没有实际编译输出
- ❌ 禁止用 `// TODO` 或 `// 省略...` 来跳过关键实现代码
- ❌ 禁止在没运行的情况下断言"功能正常"
- ❌ 禁止编造测试数据来冒充真实测试结果
- ❌ 禁止忽略编译器 warning——所有 warning 必须处理为 0
- ❌ 禁止假设用户环境已有某个工具链，必须在文档中写明安装步骤

---

## 第二节：项目信息

### 2.1 项目概述

| 项目属性 | 值 |
|---------|---|
| 项目名称 | ScreenCast Receiver（投屏接收端） |
| 项目类型 | Windows 桌面应用（仅接收端，不做发送端） |
| 主要用途 | 接收网络视频流并播放（投屏场景） |
| 目标架构 | x86_64（必须），x86 32位（兼容） |
| 最低系统要求 | Windows 10 1909+ |
| 许可证 | MIT |

### 2.2 技术选型（已确定，不得擅自更改）

| 层级 | 技术选型 | 版本要求 | 说明 |
|------|---------|---------|------|
| 应用框架 | **Tauri 2** | `^2.0` | 桌面应用外壳，管理窗口/托盘/系统交互 |
| 前端框架 | **React 18 + TypeScript** | React `^18.3`，TS `^5.4` | UI 渲染层 |
| 构建工具 | **Vite** | `^5.4` | 前端构建 |
| 样式方案 | **Tailwind CSS** | `^3.4` | 清爽扁平化 UI |
| UI 组件 | **shadcn/ui**（基于 Radix UI） | 最新 | 无障碍、可定制 |
| 状态管理 | **Zustand** | `^4.5` | 轻量全局状态 |
| 后端语言 | **Rust（stable）** | `≥ 1.75` | 系统级逻辑 |
| 媒体引擎 | **libmpv（通过 tauri-plugin-libmpv）** | `tauri-plugin-libmpv ^0.3` | 视频解码/渲染核心 |
| libmpv 动态库 | **libmpv-2.dll** | `≥ 0.35`（zhongfly 构建版） | Windows 视频解码核心 |
| 持久化 | **rusqlite** | `^0.31` | 设置/播放记录存储 |
| 全局快捷键 | **tauri-plugin-global-shortcut** | `^2.0` | 自定义快捷键 |
| 系统托盘 | **Tauri 2 内置 tray-icon** | Tauri 2 自带 | 最小化到托盘 |
| 投屏协议 | **DLNA/UPnP（接收端 Renderer 角色）** | — | 第一阶段先支持 DLNA 推送播放 |
| 网络流协议 | **HTTP / HLS / RTSP** | — | MPV 原生支持的网络流格式 |
| 打包工具 | **NSIS（Tauri bundle）** | Tauri 默认 | 生成 `.exe` 安装包 |

### 2.3 项目目录结构（必须遵循）

```
screencast-receiver/
├── src/                          # 前端源码（React + TypeScript）
│   ├── components/                # UI 组件
│   │   ├── Player/               # 播放器相关组件
│   │   │   ├── VideoSurface.tsx  # MPV 挂载区域
│   │   │   ├── ControlBar.tsx    # 播放控制栏
│   │   │   ├── ProgressBar.tsx   # 进度条
│   │   │   └── VolumeSlider.tsx  # 音量控制
│   │   ├── Settings/             # 设置面板
│   │   │   ├── GeneralSettings.tsx
│   │   │   ├── PlaybackSettings.tsx
│   │   │   ├── ShortcutSettings.tsx
│   │   │   └── NetworkSettings.tsx
│   │   ├── TitleBar/             # 自定义标题栏
│   │   │   └── TitleBar.tsx
│   │   └── Toast/               # 通知提示
│   │       └── Toast.tsx
│   ├── pages/
│   │   ├── PlayerPage.tsx        # 主播放页面
│   │   └── SettingsPage.tsx     # 设置页面
│   ├── stores/                   # Zustand 状态管理
│   │   ├── playerStore.ts
│   │   ├── settingsStore.ts
│   │   └── shortcutStore.ts
│   ├── lib/                      # 工具函数
│   │   ├── mpv.ts               # MPV 控制封装
│   │   ├── shortcuts.ts         # 快捷键管理
│   │   └── db.ts               # 数据库操作封装
│   ├── styles/
│   │   └── globals.css          # 全局样式（Tailwind 入口）
│   ├── App.tsx
│   └── main.tsx
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── main.rs              # 入口、Tauri 初始化
│   │   ├── tray.rs              # 系统托盘逻辑
│   │   ├── window_manager.rs    # 窗口管理（全屏/最小化/自适应）
│   │   ├── settings.rs          # 设置持久化（SQLite）
│   │   ├── playback.rs          # 播放记录/进度记忆
│   │   ├── shortcuts.rs         # 全局快捷键注册与管理
│   │   ├── dlna_renderer.rs     # DLNA 接收端（UPnP Renderer）
│   │   ├── network_stream.rs    # 网络流接收与缓冲
│   │   ├── mirror.rs            # 镜像翻转控制
│   │   └── error.rs             # 统一错误处理
│   ├── lib/                      # 第三方动态库
│   │   └── mpv/
│   │       ├── libmpv-2.dll     # MPV 核心库（需手动放入）
│   │       └── libmpv-wrapper.dll  # Tauri MPV 桥接库
│   ├── capabilities/
│   │   └── default.json         # Tauri 权限配置
│   ├── tauri.conf.json          # Tauri 主配置
│   ├── Cargo.toml
│   └── build.rs
├── package.json
├── tsconfig.json
├── vite.config.ts
├── tailwind.config.js
├── postcss.config.js
└── README.md
```

---

## 第三节：功能需求清单

> 每个功能（F编号）都必须独立实现、独立测试、独立验证。  
> **未标记 ✅ 测试通过的功能，不得视为完成。**

### F1：MPV 媒体引擎集成

**描述**：通过 `tauri-plugin-libmpv` 将 libmpv 嵌入 Tauri 窗口，作为视频渲染核心。

**验收标准**：
- [ ] 应用启动后，MPV 成功初始化并挂载到指定 DOM 元素上
- [ ] 能通过前端命令加载并播放本地视频文件（MP4/MKV/AVI/MOV/FLV/WebM）
- [ ] 能通过前端命令加载并播放网络视频流（HTTP/HLS/RTSP URL）
- [ ] 播放画面无花屏、无绿屏、无黑屏
- [ ] libmpv-2.dll 和 libmpv-wrapper.dll 正确打包进安装包，运行时能找到

**技术要点**：
- 使用 `tauri-plugin-libmpv` v0.3+，通过 JSON IPC 控制 MPV 进程
- MPV 初始化参数：`--vo=gpu-next --hwdec=d3d11va --keep-open=yes --force-window`
- Windows 硬件解码优先使用 `d3d11va`（Direct3D 11 Video Acceleration），兼容性好、支持 H.264/HEVC/VP9/AV1
- 窗口必须设置 `"transparent": true`，HTML/Body 背景设为 `transparent`，否则 MPV 画面无法显示
- 网络流缓冲设置：`--network-timeout=10 --demuxer-max-bytes=50M --cache=yes --cache-secs=30`

**测试要求**：
- 测试m3u8网络地址：https://sf1-cdn-tos.huoshanstatic.com/obj/media-fe/xgplayer_doc_video/hls/xgplayer-demo.m3u8
- 测试MP4本地路径：D:\ClaudeCode_Work\Screencast-Receiver\test\mov_bbb.mp4
1. **正常路径**：加载一个本地 MP4 文件 → 播放 → 画面正常 → ✅
2. **正常路径**：加载一个 HLS 流地址（`.m3u8`）→ 播放 → 画面正常 → ✅
3. **正常路径**：加载一个 RTSP 流地址 → 播放 → 画面正常 → ✅
4. **异常路径**：加载一个不存在的 URL → 前端收到错误事件 → 显示错误提示 → ✅
5. **异常路径**：网络中断后 → MPV 触发缓冲/超时 → 前端显示"网络异常"提示 → ✅

---

### F2：播放控制（暂停/播放/停止/倍速/进度）

**描述**：完整的播放控制功能，包括空格暂停、进度跳转、倍速调节。

**验收标准**：
- [ ] 空格键切换播放/暂停状态
- [ ] 点击进度条跳转（精确到秒）
- [ ] 倍速调节：0.5x / 0.75x / 1.0x / 1.25x / 1.5x / 2.0x，可扩展
- [ ] 倍速切换时音质不异常（不变调或可选变调）
- [ ] 快进/快退按钮（±10 秒、±30 秒）
- [ ] 音量调节（0-100），支持静音切换
- [ ] 记忆播放进度：关闭视频后重新打开，从上次停止位置继续

**技术要点**：
- 通过 `tauri-plugin-libmpv` 的 `command()` API 发送 MPV 命令：
  - 暂停/播放：`cycle pause`
  - 跳转：`seek <seconds> absolute`
  - 相对跳转：`seek <seconds> relative`
  - 倍速：`set speed <value>`
  - 音量：`set volume <0-100>`
  - 静音：`cycle mute`
- 进度记忆：使用 `rusqlite` 存储 `video_url → last_position` 映射，下次加载时自动 seek
- 倍速不变调：MPV 默认 `pitch-correction=yes`，保持不变即可

**测试要求**：
1. **正常路径**：播放视频 → 按空格 → 暂停 → 再按空格 → 恢复播放 → ✅
2. **正常路径**：拖拽进度条到 50% 位置 → 画面跳转到对应位置 → ✅
3. **正常路径**：设置倍速 2.0x → 视频加速播放，音频不变调 → ✅
4. **正常路径**：关闭视频 → 重新打开同一 URL → 自动从上次位置继续 → ✅
5. **异常路径**：视频未加载时点击暂停按钮 → 不崩溃，按钮无响应或显示禁用状态 → ✅

---

### F3：全屏与窗口自适应

**描述**：双击切换全屏/退出全屏，窗口大小自适应视频分辨率。

**验收标准**：
- [ ] 双击视频画面 → 进入全屏 → ✅
- [ ] 全屏状态下双击 → 退出全屏 → ✅
- [ ] 按 `Esc` 键 → 退出全屏 → ✅
- [ ] 窗口大小改变时，视频画面等比缩放自适应（保持宽高比，不拉伸变形）
- [ ] 打开视频时，窗口可自动调整为视频原始分辨率大小（可选功能，在设置中可开关）
- [ ] 全屏状态下播放控制栏自动隐藏，鼠标移动时显示

**技术要点**：
- 全屏使用 Tauri 的 `window.setFullscreen(true/false)` API
- 双击事件监听：`dblclick` on VideoSurface 元素
- 窗口自适应视频大小：通过 MPV 的 `observeProperties` 监听 `width` 和 `height` 属性变化，获取视频原始分辨率，然后调用 `window.setSize(width, height)` 调整窗口
- 等比缩放：CSS `object-fit: contain` 等价效果由 MPV 自身处理（MPV 默认保持宽高比）
- 控制栏自动隐藏：`mousemove` 事件 + `setTimeout` 3 秒无操作自动隐藏

**测试要求**：
1. **正常路径**：播放视频 → 双击画面 → 进入全屏 → 画面充满屏幕 → ✅
2. **正常路径**：全屏状态 → 按 Esc → 退出全屏 → 窗口恢复之前大小 → ✅
3. **正常路径**：拖拽调整窗口大小 → 视频画面等比缩放不拉伸 → ✅
4. **正常路径**：打开 1920x1080 视频 → 窗口自动调整为 1920x1080（设置开启时）→ ✅
5. **正常路径**：全屏播放 → 移动鼠标 → 控制栏出现 → 停止移动 3 秒 → 控制栏隐藏 → ✅

---

### F4：镜像翻转

**描述**：支持水平镜像和垂直镜像翻转视频画面。

**验收标准**：
- [ ] 水平镜像（左右翻转）功能正常
- [ ] 垂直镜像（上下翻转）功能正常
- [ ] 可同时水平和垂直翻转（等效 180 度旋转）
- [ ] 翻转状态在设置面板中有开关，可随时切换
- [ ] 翻转状态随视频记忆（可选）

**技术要点**：
- 使用 MPV 的 `--vf=lavfi=hflip` 实现水平翻转
- 使用 MPV 的 `--vf=lavfi=vflip` 实现垂直翻转
- 组合：`--vf=lavfi=[hflip,vflip]` 实现同时翻转
- 运行时动态切换：通过 `set_property("vf", "lavfi=hflip")` 或 `command("vf toggle", ["lavfi=hflip"])`
- 注意：MPV 的 vf 滤镜在 `gpu-next` 模式下可能需要使用 `lavfi` 前缀

**测试要求**：
1. **正常路径**：播放视频 → 开启水平镜像 → 画面左右翻转 → ✅
2. **正常路径**：播放视频 → 开启垂直镜像 → 画面上下翻转 → ✅
3. **正常路径**：同时开启水平和垂直 → 画面 180 度旋转 → ✅
4. **正常路径**：关闭镜像 → 画面恢复正常 → ✅
5. **异常路径**：硬件解码模式下开启镜像 → 不崩溃，正常翻转（D3D11VA 需验证兼容性）→ ✅

---

### F5：系统托盘与最小化行为

**描述**：关闭窗口时最小化到托盘（可选），点击托盘图标恢复窗口。

**验收标准**：
- [ ] 点击窗口关闭按钮 → 窗口隐藏到托盘（不退出进程）→ ✅
- [ ] 托盘图标左键单击 → 显示/隐藏窗口切换 → ✅
- [ ] 托盘右键菜单 → 包含"显示主窗口"和"退出"两项 → ✅
- [ ] 点击"退出" → 完全关闭应用 → ✅
- [ ] 设置项"关闭后最小化到托盘"可开关，关闭此选项后点关闭直接退出 → ✅
- [ ] 设置项"最小化到托盘时暂停播放"可开关 → ✅
- [ ] 从托盘恢复窗口时，如果之前在播放则继续播放 → ✅

**技术要点**：
- Tauri 2 托盘实现：使用 `tauri::tray::{TrayIconBuilder, TrayIconEvent}`
- 菜单：`tauri::menu::{MenuBuilder, MenuItemBuilder}`
- 关键 API：`window.on_close_requested` 中调用 `event.prevent_close()` + `window.hide()`
- 使用 `AtomicBool` 存储最小化到托盘的设置，避免每次关闭都查数据库
- 参考实现模式（已验证可行）：
  ```rust
  // main.rs setup 中
  let minimize_to_tray = Arc::new(AtomicBool::new(true)); // 从数据库加载
  window.on_close_requested(move |_window| {
      if minimize_to_tray.load(Ordering::SeqCst) {
          _window.hide().unwrap();
          // 如果开启了"最小化暂停"
          if pause_on_minimize.load(Ordering::SeqCst) {
              // 发送暂停命令
          }
      }
      tauri::CloseRequestResponse::PreventClose
  });
  ```

**测试要求**：
1. **正常路径**：播放视频 → 点击关闭按钮 → 窗口消失 → 托盘图标可见 → ✅
2. **正常路径**：托盘图标右键 → 点击"退出" → 应用完全关闭 → ✅
3. **正常路径**：托盘图标左键单击 → 窗口重新显示 → 视频继续播放 → ✅
4. **正常路径**：设置关闭"最小化到托盘" → 点击关闭按钮 → 应用直接退出 → ✅
5. **正常路径**：设置开启"最小化暂停" → 最小化到托盘 → 播放暂停 → 恢复窗口 → 播放恢复 → ✅

---

### F6：自定义快捷键

**描述**：用户可自定义全局/窗口内快捷键，支持常用播放操作。

**验收标准**：
- [ ] 设置面板可查看和修改所有快捷键绑定
- [ ] 默认快捷键：
  - `Space` → 播放/暂停
  - `←` → 后退 5 秒
  - `→` → 前进 5 秒
  - `↑` → 音量 +5
  - `↓` → 音量 -5
  - `M` → 静音切换
  - `F` → 全屏切换
  - `Esc` → 退出全屏
  - `C` → 镜像翻转切换
- [ ] 快捷键冲突检测：用户设置的快捷键与系统/其他应用冲突时给出提示
- [ ] 快捷键设置持久化（存 SQLite），下次启动自动恢复
- [ ] 全局快捷键（应用不在前台时也生效）和窗口快捷键可选切换

**技术要点**：
- Tauri 2 全局快捷键插件：`tauri-plugin-global-shortcut` v2
- 注册方式：在 `main.rs` 的 `setup` 中通过 `app.global_shortcut().register(shortcut)?`
- 快捷键存储格式：`{ "action": "play_pause", "shortcut": "Space", "global": false }`
- 冲突检测：注册前先检查 `register` 返回值，失败时提示用户
- Windows 上全局快捷键不需要额外权限（不像 macOS 需要辅助功能权限）
- 使用 `tauri-plugin-global-shortcut` 的 `Shortcut::new(Modifiers, Code)` 或字符串解析 `"Alt+Shift+P"`

**测试要求**：
1. **正常路径**：播放视频 → 按空格 → 暂停 → 再按空格 → 恢复 → ✅
2. **正常路径**：按左箭头 → 后退 5 秒 → 按右箭头 → 前进 5 秒 → ✅
3. **正常路径**：进入设置 → 修改"播放/暂停"快捷键为 `Ctrl+P` → 保存 → 按 `Ctrl+P` 生效 → ✅
4. **正常路径**：重启应用 → 自定义快捷键仍然生效 → ✅
5. **异常路径**：设置已被其他应用占用的快捷键 → 显示"快捷键冲突"提示 → ✅

---

### F7：播放进度记忆

**描述**：自动记录每个视频的播放进度，下次打开时自动恢复。

**验收标准**：
- [ ] 播放视频时，每 5 秒自动保存当前进度到数据库
- [ ] 关闭视频或关闭应用后，进度不丢失
- [ ] 重新打开同一视频 URL → 弹出提示"是否从上次位置继续？"→ 选择"继续"则 seek 到记录位置 → ✅
- [ ] 播放完成后（自然结束）→ 清除进度记录 → 下次从头开始 → ✅
- [ ] 播放进度记录上限：最近 100 条，超出后淘汰最旧的

**技术要点**：
- 数据库表设计：
  ```sql
  CREATE TABLE IF NOT EXISTS playback_history (
      url TEXT PRIMARY KEY,
      title TEXT,
      last_position REAL NOT NULL,
      duration REAL,
      updated_at INTEGER NOT NULL,
      completed INTEGER DEFAULT 0
  );
  ```
- 定时保存：前端 `setInterval` 每 5 秒调用一次保存，或监听 MPV 的 `time-pos` 属性变化（节流 5 秒）
- 超过 95% 进度视为"完成"，自动清除记录

**测试要求**：
1. **正常路径**：播放视频到 30% → 关闭 → 重新打开 → 提示继续 → 从 30% 位置开始 → ✅
2. **正常路径**：播放至结尾 → 重新打开 → 从头开始播放（无提示）→ ✅
3. **正常路径**：播放 3 个不同视频 → 各自动保存进度 → 分别恢复正确 → ✅
4. **异常路径**：数据库文件损坏 → 应用不崩溃 → 重建数据库 → 提示"播放记录已重置" → ✅

---

### F8：参数设置面板

**描述**：完整的设置界面，覆盖所有可配置项。

**验收标准**：
- [ ] 设置面板以独立页面/弹窗形式呈现，分类清晰
- [ ] **通用设置**：
  - [ ] 开机自启动（开关）
  - [ ] 关闭窗口时最小化到托盘（开关）
  - [ ] 最小化到托盘时暂停播放（开关）
  - [ ] 语言选择（简体中文/English，至少支持中英文）
- [ ] **播放设置**：
  - [ ] 默认音量（0-100 滑块）
  - [ ] 默认倍速（下拉选择）
  - [ ] 打开视频时自动调整窗口大小（开关）
  - [ ] 硬件解码（开关，默认开）
  - [ ] 网络缓冲时间（1-60 秒滑块）
  - [ ] 循环播放（开关）
- [ ] **快捷键设置**：
  - [ ] 所有快捷键可查看、可修改、可重置为默认
- [ ] **网络设置**：
  - [ ] 投屏协议开关（DLNA 接收，后续可扩展）
  - [ ] 监听端口配置
  - [ ] 允许的来源 IP 列表（白名单，可选）
- [ ] 所有设置项即时保存，无需手动点"保存"按钮
- [ ] 设置面板 UI 清爽扁平，符合整体设计风格

**技术要点**：
- 设置存储：`rusqlite` 单表 `settings (key TEXT PRIMARY KEY, value TEXT)`
- 前端设置面板使用 React + Tailwind，开关用 `Switch` 组件，滑块用 `Slider`
- 设置变更 → 前端更新 Zustand store → 调用 Tauri command 写入数据库 → 触发相关模块重新读取配置
- 开机自启动：Tauri 2 内置 `tauri-plugin-autostart`

**测试要求**：
1. **正常路径**：打开设置 → 修改"默认音量"为 50 → 关闭设置 → 重新打开设置 → 值仍为 50 → ✅
2. **正常路径**：开启"开机自启动" → 重启电脑 → 应用自动启动 → ✅
3. **正常路径**：关闭"硬件解码" → 播放视频 → 使用软解播放（CPU 占用升高但画面正常）→ ✅
4. **正常路径**：修改网络缓冲时间为 30 秒 → 播放网络流 → 缓冲更稳定 → ✅
5. **异常路径**：设置数据库写入失败（磁盘满/只读）→ 显示错误提示"设置保存失败" → ✅

---

### F9：DLNA 投屏接收端

**描述**：作为 DLNA Media Renderer，接收来自手机/平板的投屏推送并播放。

**验收标准**：
- [ ] 应用启动后自动在局域网广播 DLNA Renderer 存在
- [ ] 手机端（如微信、爱奇艺、B站等）能发现本应用作为投屏目标
- [ ] 手机端点击投屏 → 视频 URL 推送到本应用 → 自动开始播放 → ✅
- [ ] 手机端可控制播放/暂停/停止/进度跳转 → ✅
- [ ] 手机端断开连接 → 应用停止播放，回到空闲状态 → ✅
- [ ] 支持同时只有一个活跃投屏会话（不支持多路并发）

**技术要点**：
- DLNA Renderer 核心实现：
  - **SSDP 广播**：监听 UDP 1900 端口，响应 `M-SEARCH` 请求，宣告自身为 `urn:schemas-upnp-org:device:MediaRenderer:1`
  - **HTTP 控制服务**：实现 UPnP AVTransport v3 和 RenderingControl v1 服务
  - **事件通知**：通过 `Event Subscribe (GENA)` 向控制器推送状态变化
- Rust crate 参考：`crab-dlna`（作为 DLNA 客户端参考其协议实现）、`ssdp`（Rust SSDP 库）
- 也可考虑使用 `rustupnp` 或自行实现最小化 Renderer（仅需响应关键 SOAP 动作）
- 必需实现的 SOAP 动作：
  - `SetAVTransportURI` — 接收视频 URL
  - `Play` / `Pause` / `Stop` / `Seek`
  - `GetPositionInfo` / `GetTransportInfo`
  - `SetVolume` / `GetVolume` / `SetMute`
- 收到 `SetAVTransportURI` 后，调用 MPV 的 `loadfile` 命令加载 URL

**测试要求**：
1. **正常路径**：手机（同一局域网）→ 打开视频 App → 点击投屏按钮 → 发现本应用 → 点击推送 → 视频开始播放 → ✅
2. **正常路径**：手机端点击暂停 → 播放暂停 → 手机端点击播放 → 恢复播放 → ✅
3. **正常路径**：手机端断开 → 应用停止播放 → ✅
4. **正常路径**：用 `crab-dlna` CLI 作为控制器测试 → `crab-dlna play video.mp4 -d <本机地址>` → 播放成功 → ✅
5. **异常路径**：收到不支持的格式 URL → 返回错误响应 → 前端显示"不支持的格式" → ✅

---

### F10：UI 设计与交互

**描述**：清爽、扁平化、交互丝滑的用户界面。

**验收标准**：
- [ ] 整体风格：扁平化、无多余装饰、配色克制（深色主题为主，可选浅色）
- [ ] 自定义标题栏（无系统默认边框），包含拖拽区域、最小化/最大化/关闭按钮
- [ ] 播放器页面：视频区域居中，控制栏悬浮于底部，半透明毛玻璃效果
- [ ] 控制栏按钮：播放/暂停、进度条、音量、全屏、镜像、设置，图标清晰
- [ ] 所有交互动画：按钮 hover 效果、控制栏淡入淡出、页面切换过渡，时长 ≤ 200ms
- [ ] 窗口 resize 时 UI 元素平滑重排，无闪烁无卡顿
- [ ] Toast 通知：操作反馈（如"已复制到剪贴板""设置已保存"），3 秒自动消失
- [ ] 加载状态：视频缓冲时显示加载动画（spinner），非阻塞 UI
- [ ] 错误状态：网络错误、解码错误等有明确的中文错误提示
- [ ] 高 DPI 支持：在 125%/150%/200% 缩放下 UI 不模糊、不溢出

**技术要点**：
- Tailwind CSS 主题色：深色 `#1a1a2e` 背景、`#eaeaea` 前景、`#6366f1` 强调色
- 毛玻璃效果：`backdrop-blur-md bg-black/40`
- 图标库：`lucide-react`（轻量、风格统一）
- 动画：`framer-motion`（页面切换）+ CSS transition（按钮/ hover）
- 自定义标题栏：Tauri 配置 `"decorations": false`，前端实现标题栏
- 高 DPI：`tauri.conf.json` 中 `"dpiAware": "perMonitor"`

**测试要求**：
1. **视觉检查**：启动应用 → 界面清爽无杂乱元素 → 配色协调 → ✅
2. **交互检查**：鼠标悬停在按钮上 → 有 hover 效果 → 点击有反馈动画 → ✅
3. **全屏检查**：进入全屏 → 控制栏自动隐藏 → 移动鼠标 → 淡入显示 → ✅
4. **DPI 检查**：系统缩放置 150% → 应用重启 → UI 元素清晰不模糊 → ✅
5. **窗口调整**：拖拽窗口边缘调整大小 → UI 平滑重排无闪烁 → ✅

---

### F11：网络流播放稳定性

**描述**：针对网络视频流的播放优化，确保稳定不卡顿。

**验收标准**：
- [ ] 支持 HTTP 渐进式下载播放（边下边播）
- [ ] 支持 HLS（.m3u8）直播和点播
- [ ] 支持 RTSP 流（TCP 模式优先，避免 UDP 丢包）
- [ ] 网络波动时自动缓冲，缓冲完成后无缝恢复播放
- [ ] 缓冲超时（默认 15 秒）后显示"网络异常"提示，提供"重试"按钮
- [ ] 长时间播放（≥ 2 小时）不内存泄漏、不崩溃

**技术要点**：
- MPV 网络参数优化：
  ```
  --network-timeout=15
  --http-header-fields="User-Agent: Mozilla/5.0"
  --rtsp-transport=tcp
  --cache=yes
  --cache-secs=60
  --demuxer-max-bytes=100M
  --demuxer-readahead-bytes=50M
  ```
- 内存泄漏检测：开发模式下用 `cargo run` 观察内存增长，长时间播放后不超过初始值的 2 倍
- 断线重连：监听 MPV 的 `end-of-file` 事件，如果是网络错误则自动重试 3 次（间隔递增 2s/4s/8s）

**测试要求**：
1. **正常路径**：播放 HTTP 视频 URL → 边缓冲边播放 → 画面流畅 → ✅
2. **正常路径**：播放 HLS 直播流 → 持续接收并播放 → 不卡顿 → ✅
3. **正常路径**：播放 RTSP 流（TCP）→ 画面正常 → ✅
4. **异常路径**：播放中拔掉网线 → 5 秒后显示"网络异常" → 插回网线 → 点击重试 → 恢复播放 → ✅
5. **稳定性路径**：连续播放 2 小时 → 内存增长 < 100MB → 无崩溃 → ✅

---

### F12：安装与打包

**描述**：生成 Windows 安装包，一键安装即可使用。

**验收标准**：
- [ ] 使用 Tauri bundle 生成 `.exe` 安装包（NSIS 格式）
- [ ] 安装包内含 libmpv-2.dll 和 libmpv-wrapper.dll，安装后无需额外配置
- [ ] 安装路径默认 `C:\Program Files\ScreenCast Receiver\`
- [ ] 安装时创建桌面快捷方式和开始菜单项
- [ ] 卸载时清理所有文件（含用户数据可选保留）
- [ ] 支持 x86_64 架构（必须），x86 32 位（兼容）
- [ ] 安装包体积 ≤ 80MB（含 MPV 动态库）

**技术要点**：
- `tauri.conf.json` 中配置 bundle 信息：`productName`、`version`、`identifier`、`category`
- Windows 打包配置：`"targets": ["nsis"]`，NSIS 选项配置安装路径
- 动态库打包：在 `tauri.conf.json` 的 `bundle.externalBin` 中声明 dll 路径
- 或使用 Tauri 的 `resources` 配置将 dll 复制到运行目录
- 交叉编译 x86 32 位：`rustup target add i686-pc-windows-msvc`，构建时指定 `--target i686-pc-windows-msvc`

**测试要求**：
1. **正常路径**：双击安装包 → 安装向导 → 选择安装路径 → 安装完成 → 桌面出现快捷方式 → ✅
2. **正常路径**：双击快捷方式 → 应用正常启动 → 可播放视频 → ✅
3. **正常路径**：通过开始菜单卸载 → 应用被完全移除 → ✅
4. **正常路径**：在 x86 32 位 Windows 上安装运行 → 功能正常 → ✅
5. **异常路径**：安装路径包含中文/空格 → 应用正常运行（路径处理正确）→ ✅

---

## 第四节：架构设计

### 4.1 系统架构图

```
┌─────────────────────────────────────────────────────┐
│                  前端 (React/TS)                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │
│  │ Player   │  │ Settings │  │ TitleBar/Toast   │   │
│  │ Page     │  │ Page     │  │                  │   │
│  └────┬─────┘  └────┬─────┘  └────────┬─────────┘   │
│       │              │                 │             │
│  ┌────┴──────────────┴─────────────────┴─────────┐   │
│  │         Zustand Stores (状态管理)               │   │
│  │  playerStore | settingsStore | shortcutStore    │   │
│  └────┬───────────────────────────────────────────┘   │
│       │ Tauri IPC (invoke / event / listen)            │
├───────┼─────────────────────────────────────────────────┤
│       ▼ Rust 后端 (Tauri Commands)                     │
│  ┌──────────────────────────────────────────────────┐  │
│  │  Tauri Runtime (main.rs / setup)                 │  │
│  │  ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐   │  │
│  │  │ MPV    │ │ Tray   │ │ Window │ │ Short- │   │  │
│  │  │ Engine │ │ Mgr    │ │ Mgr    │ │ cuts   │   │  │
│  │  └───┬────┘ └────────┘ └────────┘ └────────┘   │  │
│  │      │                                           │  │
│  │  ┌────────┐ ┌──────────┐ ┌────────────────┐     │  │
│  │  │ DLNA   │ │ Settings │ │ Network Stream │     │  │
│  │  │ Renderer│ │ (SQLite) │ │ Manager        │     │  │
│  │  └────────┘ └──────────┘ └────────────────┘     │  │
│  └──────────────────┬───────────────────────────────┘  │
│                     │                                   │
│  ┌──────────────────┼───────────────────────────────┐  │
│  │           libmpv (FFI)                            │  │
│  │  libmpv-wrapper.dll ←→ libmpv-2.dll              │  │
│  └──────────────────┬───────────────────────────────┘  │
└─────────────────────┼─────────────────────────────────┘
                      ▼
        ┌─────────────────────────────┐
        │   Windows 图形子系统        │
        │   D3D11 VA (硬件解码)       │
        │   Direct3D 11 (渲染)       │
        └─────────────────────────────┘
```

### 4.2 模块依赖关系

```
main.rs
  ├── tray.rs (依赖: settings.rs 读取托盘配置)
  ├── window_manager.rs (依赖: MPV Engine 暂停/恢复)
  ├── settings.rs (依赖: rusqlite)
  ├── playback.rs (依赖: rusqlite, MPV Engine)
  ├── shortcuts.rs (依赖: settings.rs, MPV Engine)
  ├── dlna_renderer.rs (依赖: MPV Engine, network_stream.rs)
  ├── network_stream.rs (依赖: tokio, MPV Engine)
  └── mirror.rs (依赖: MPV Engine)
```

### 4.3 数据流向

**播放控制流**（用户操作 → 画面变化）：
```
用户按键/点击 → React 事件 → Zustand action → Tauri invoke
→ Rust command → MPV IPC command → libmpv → D3D11 渲染 → 屏幕
```

**状态反馈流**（播放变化 → UI 更新）：
```
MPV 属性变化 → libmpv 事件 → Rust event loop → Tauri emit
→ 前端 listen → Zustand update → React 重新渲染
```

**DLNA 投屏流**（手机推送 → 播放）：
```
手机 SOAP 请求 → DLNA HTTP 服务 → dlna_renderer.rs
→ 解析 SetAVTransportURI → MPV loadfile → 播放
→ 定时上报 GetPositionInfo → SOAP 响应 → 手机更新进度
```

---

## 第五节：开发流程（必须严格按顺序执行）

### Step 1：环境准备与项目初始化
1. 确认 Rust 版本 ≥ 1.75（`rustc --version`）
2. 确认 Node.js ≥ 18（`node --version`）
3. 确认 pnpm 已安装（`pnpm --version`）
4. 确认 Visual Studio C++ Build Tools 已安装（Windows）
5. 使用 `pnpm create tauri-app@latest` 初始化项目，选择 React + TypeScript 模板
6. 安装所有依赖（见 `package.json` 和 `Cargo.toml`）
7. **验证**：`pnpm tauri:dev` 能启动空白窗口 → ✅

### Step 2：集成 tauri-plugin-libmpv
1. 执行 `pnpm tauri add libmpv`
2. 下载 libmpv-wrapper.dll 和 libmpv-2.dll 放入 `src-tauri/lib/mpv/`
3. 配置 `tauri.conf.json` 的 `resources` 确保 dll 被打包
4. 编写最小测试：前端调用 `init()` 初始化 MPV，加载一个测试视频文件
5. **验证**：能播放本地测试视频 → ✅

### Step 3：实现基础播放控制（F2）
1. 封装 MPV 命令层（`src/lib/mpv.ts`）
2. 实现播放/暂停/停止/进度/音量/倍速
3. 构建 ControlBar UI 组件
4. **验证**：所有控制功能正常 → ✅

### Step 4：实现全屏与窗口自适应（F3）
1. 双击全屏切换
2. 窗口 resize 监听
3. 视频分辨率自适应窗口
4. **验证**：全屏/退出/自适应全部通过 → ✅

### Step 5：实现镜像翻转（F4）
1. MPV vf 滤镜控制
2. 设置面板开关
3. **验证**：水平/垂直/组合翻转正常 → ✅

### Step 6：实现系统托盘（F5）
1. 托盘图标创建
2. 关闭到托盘逻辑
3. 托盘菜单（显示/退出）
4. **验证**：最小化到托盘/恢复/退出全部通过 → ✅

### Step 7：实现快捷键系统（F6）
1. 集成 `tauri-plugin-global-shortcut`
2. 默认快捷键注册
3. 设置面板快捷键编辑
4. 持久化存储
5. **验证**：所有快捷键正常、可修改、可恢复 → ✅

### Step 8：实现播放进度记忆（F7）
1. SQLite 数据库初始化
2. 播放进度定时保存
3. 恢复播放提示
4. **验证**：进度记忆/恢复/清除全部通过 → ✅

### Step 9：实现设置面板（F8）
1. 通用设置 / 播放设置 / 网络设置 UI
2. 所有设置项持久化
3. 开机自启动集成
4. **验证**：所有设置项保存/读取/生效 → ✅

### Step 10：实现 DLNA 接收端（F9）
1. SSDP 广播与发现响应
2. UPnP AVTransport 服务实现
3. SOAP 请求解析
4. 与 MPV 引擎对接
5. **验证**：手机投屏推送/控制/断开全部通过 → ✅

### Step 11：网络流稳定性优化（F11）
1. MPV 网络参数调优
2. 缓冲超时与重试机制
3. 长时间播放内存检测
4. **验证**：各种网络流稳定播放 → ✅

### Step 12：UI 打磨（F10）
1. 自定义标题栏
2. 深色/浅色主题
3. 动画与过渡效果
4. 高 DPI 适配
5. **验证**：视觉/交互/DPI 全部通过 → ✅

### Step 13：打包与安装（F12）
1. 配置 Tauri bundle
2. 生成 x86_64 安装包
3. 生成 x86 32 位安装包
4. 安装/卸载测试
5. **验证**：安装包正常安装运行 → ✅

---

## 第六节：测试规范（强制执行）

### 6.1 测试层次

| 层次 | 工具 | 覆盖内容 |
|------|------|---------|
| Rust 单元测试 | `cargo test` | 工具函数、数据库操作、协议解析 |
| Rust 集成测试 | `cargo test --test` | 模块间协作、DLNA 协议交互 |
| 前端单元测试 | `vitest` | 组件渲染、状态管理逻辑 |
| 端到端测试 | 手动 + 脚本 | 完整用户场景 |
| 性能测试 | 长时间运行 + 内存监控 | 内存泄漏、CPU 占用 |

### 6.2 测试报告格式（每次交付必须包含）

```markdown
## 测试报告 - [功能名称]

**测试日期**：YYYY-MM-DD
**测试环境**：Windows 11 x64 / Rust 1.xx / Node.js xx
**测试人员**：AI 执行 + 用户验证

### 测试结果汇总
| 测试项 | 通过/失败 | 说明 |
|--------|----------|------|
| [测试1] | ✅ PASS | 详细描述 |
| [测试2] | ❌ FAIL | 失败原因 + 修复方案 |

**总计**：X / Y 通过
**状态**：全部通过 ✅ / 存在失败项 ❌（不得交付）
```

### 6.3 测试强制规则

- **FAIL 不允许交付**：任何功能测试未通过，必须在文档中明确标注，不得标记为完成
- **禁止修改测试凑通过**：如果测试失败，必须修复代码，不得修改测试逻辑去适配 bug
- **异常路径必须测试**：每个功能至少包含 1 条异常路径测试
- **网络功能需真实网络测试**：DLNA、网络流播放必须在真实局域网环境中测试，不得仅用 mock

---

## 第七节：反幻觉机制（AI 自检清单）

> 每次声明"完成"前，AI 必须逐项回答以下问题。任何一项回答"不确定"或"否"，**不得交付**。

### 7.1 代码真实性检查

| 编号 | 检查项 | AI 回答 |
|------|--------|---------|
| C1 | 我使用的每个 Rust crate 是否确认在 crates.io 上存在且版本正确？ | |
| C2 | 我调用的每个 MPV 命令/属性是否确认在 MPV 文档中有记录？ | |
| C3 | 我使用的每个 Tauri 2 API 是否确认在 Tauri 2 官方文档中有记录？ | |
| C4 | 我引用的每个前端 npm 包是否确认在 npm registry 上存在？ | |
| C5 | 代码中的 `unsafe` 块是否都有安全说明？ | |

### 7.2 构建真实性检查

| 编号 | 检查项 | AI 回答 |
|------|--------|---------|
| B1 | 我是否实际执行了 `cargo build` 并得到 0 error 0 warning？ | |
| B2 | 我是否实际执行了 `pnpm build` 并得到成功输出？ | |
| B3 | 我是否实际运行了 `cargo test` 并看到测试通过？ | |
| B4 | 如果涉及 DLL/动态库，我是否确认文件存在且版本正确？ | |
| B5 | 我是否验证了 `tauri-plugin-libmpv` 的 API 与实际安装的版本匹配？ | |

### 7.3 功能真实性检查

| 编号 | 检查项 | AI 回答 |
|------|--------|---------|
| F1 | 我是否实际播放了一个真实视频文件来验证播放功能？ | |
| F2 | 我是否实际测试了 DLNA 投屏（用真实手机或 crab-dlna CLI）？ | |
| F3 | 我是否实际测试了最小化到托盘的完整流程？ | |
| F4 | 我是否实际验证了快捷键的注册和触发？ | |
| F5 | 我是否实际测试了网络流播放（非本地文件）？ | |

---

## 第八节：已知技术风险与应对

| 风险 | 影响 | 应对措施 |
|------|------|---------|
| libmpv DLL 版本不兼容 | MPV 初始化失败 | 锁定 libmpv ≥ 0.35，使用 zhongfly 构建版，在 README 中写明版本 |
| D3D11VA 在部分显卡上崩溃 | 硬件解码失败 | 捕获 MPV 错误事件，自动降级为软解（hwdec=no） |
| DLNA SSDP 被防火墙拦截 | 手机无法发现设备 | 首次运行时请求防火墙放行，README 写明手动配置方法 |
| Tauri 2 托盘 API 变更 | 托盘功能失效 | 使用 Tauri 2 稳定版（≥ 2.0.0），不在 rc/beta 版本上开发 |
| 全局快捷键被系统占用 | 注册失败 | 捕获错误，提示用户更换快捷键，提供默认备选方案 |
| 高 DPI 下 UI 模糊 | 用户体验差 | 配置 `dpiAware: perMonitor`，前端使用矢量图标 |
| 长时间播放内存增长 | 内存泄漏 | 定期监控，使用 `cargo-profiler` 分析，确保 MPV 事件监听正确释放 |
| Windows 11 深色标题栏 | 视觉不一致 | 使用 `SetTitleBarThemeColor` Windows API 或通过 Tauri 配置 |

---

## 第九节：交付检查清单

> **每次交付前，AI 必须逐项填写此清单。未填写或存在 ❌ 项的交付视为无效。**

### 9.1 代码完整性

- [ ] 所有源文件已创建，无 `// TODO` 或 `// 省略` 占位符
- [ ] `Cargo.toml` 依赖完整，版本号明确
- [ ] `package.json` 依赖完整，版本号明确
- [ ] `tauri.conf.json` 配置完整
- [ ] `capabilities/default.json` 权限配置正确
- [ ] `lib/mpv/` 目录包含所需的 DLL 文件（或提供下载脚本）

### 9.2 编译状态

- [ ] `cargo build --release` 0 error, 0 warning
- [ ] `pnpm build` 成功
- [ ] `cargo test` 全部通过
- [ ] `pnpm tauri:dev` 能正常启动

### 9.3 功能测试

| 功能 | 测试状态 | 备注 |
|------|---------|------|
| F1 MPV 引擎集成 | ⬜ 未测试 | |
| F2 播放控制 | ⬜ 未测试 | |
| F3 全屏与窗口自适应 | ⬜ 未测试 | |
| F4 镜像翻转 | ⬜ 未测试 | |
| F5 系统托盘 | ⬜ 未测试 | |
| F6 自定义快捷键 | ⬜ 未测试 | |
| F7 播放进度记忆 | ⬜ 未测试 | |
| F8 参数设置面板 | ⬜ 未测试 | |
| F9 DLNA 投屏接收 | ⬜ 未测试 | |
| F10 UI 设计与交互 | ⬜ 未测试 | |
| F11 网络流稳定性 | ⬜ 未测试 | |
| F12 安装与打包 | ⬜ 未测试 | |

### 9.4 文档完整性

- [ ] README.md 包含：项目介绍、安装步骤、使用说明、已知问题
- [ ] 代码关键逻辑有注释说明
- [ ] API 接口有文档注释（Rust doc / TypeScript JSDoc）
- [ ] 测试报告已填写（见第六节格式）

### 9.5 最终声明

> AI 在交付时必须在此处明确写出：
> - 本次交付包含哪些功能模块
> - 哪些功能已通过真实测试
> - 哪些功能仅编译通过但未真实运行验证（必须诚实标注）
> - 是否存在已知问题或限制
> - 用户接下来需要做什么来验证

---

## 第十节：用户填写区

> 以下区域由用户（你）填写，AI 不得修改。

### 10.1 投屏协议优先级

- [x] DLNA/UPnP（第一阶段必须实现）
- [ ] AirPlay 接收（第二阶段可选）
- [ ] Miracast 接收（第二阶段可选）
- [ ] 自定义私有协议（如有，请描述）

### 10.2 视频格式优先级

- [x] MP4 (H.264 + AAC) — 最高优先级
- [x] MKV (H.264/H.265) — 高优先级
- [x] MOV (H.264) — 高优先级
- [x] AVI — 中优先级
- [x] FLV — 中优先级
- [x] WebM (VP9/AV1) — 中优先级
- [ ] 其他：______

### 10.3 网络流协议优先级

- [x] HTTP 渐进式下载 — 必须
- [x] HLS (.m3u8) — 必须
- [x] RTSP (TCP) — 必须
- [ ] RTMP — 可选
- [ ] WebRTC — 可选
- [ ] SRT — 可选

### 10.4 UI 偏好

- [x] 深色主题为主，可切换浅色
- [x] 中文界面（简体）
- [ ] 英文界面
- [x] 毛玻璃效果
- [x] 动画过渡（200ms 以内）
- [ ] 无边框设计

### 10.5 性能要求

- 最低内存占用：≤ 200MB（空闲状态）
- 最低 CPU 占用：≤ 5%（空闲）/ ≤ 15%（1080p 播放）
- 启动时间：≤ 3 秒（冷启动到可操作）
- 安装包体积：最小体积占用

### 10.6 其他需求

（用户可在此补充任何文档中未覆盖的需求）

---

## 附录 A：关键依赖版本锁定

```toml
# Cargo.toml 关键依赖（必须锁定版本）
[dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-libmpv = "0.3"
tauri-plugin-global-shortcut = "2"
tauri-plugin-autostart = "2"
rusqlite = { version = "0.31", features = ["bundled"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
log = "0.4"
env_logger = "0.11"
libmpv-wrapper = "0.1"  # 如需直接使用
```

```json
// package.json 关键依赖
{
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@tauri-apps/plugin-libmpv": "^0.3.0",
    "@tauri-apps/plugin-global-shortcut": "^2.0.0",
    "@tauri-apps/plugin-autostart": "^2.0.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "zustand": "^4.5.0",
    "lucide-react": "^0.400.0",
    "framer-motion": "^11.0.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "typescript": "^5.4.0",
    "vite": "^5.4.0",
    "@vitejs/plugin-react": "^4.3.0",
    "tailwindcss": "^3.4.0",
    "postcss": "^8.4.0",
    "autoprefixer": "^10.4.0",
    "vitest": "^1.6.0"
  }
}
```

## 附录 B：MPV 初始化推荐参数

```typescript
// src/lib/mpv.ts
export const MPV_CONFIG = {
  args: [
    '--vo=gpu-next',           // 使用 gpu-next 视频输出（更稳定）
    '--hwdec=d3d11va',        // Windows D3D11 硬件解码
    '--hwdec-codecs=all',     // 所有 codec 尝试硬件解码
    '--keep-open=yes',        // 播放完毕不关闭窗口
    '--force-window',         // 强制显示窗口
    '--network-timeout=15',   // 网络超时 15 秒
    '--rtsp-transport=tcp',   // RTSP 优先 TCP
    '--cache=yes',            // 开启缓存
    '--cache-secs=60',        // 缓存 60 秒
    '--demuxer-max-bytes=100M',  // 解复用器最大缓冲
    '--http-header-fields=User-Agent: Mozilla/5.0',  // 伪装 UA
    '--aid=auto',             // 自动选择音轨
    '--vid=auto',             // 自动选择视频轨
    '--osd-level=0',          // 关闭 OSD（用前端 UI 替代）
    '--no-input-default-bindings',  // 禁用 MPV 默认键绑定（用前端控制）
    '--no-osc',               // 禁用 MPV 内置控制条
  ],
  observedProperties: [
    'pause',
    'time-pos',
    'duration',
    'volume',
    'mute',
    'speed',
    'width',
    'height',
    'filename',
    'eof-reached',
    'cache-buffering-state',
  ] as const,
  ipcTimeoutMs: 5000,
};
```

## 附录 C：Tauri 权限配置模板

```json
// src-tauri/capabilities/default.json
{
  "identifier": "default",
  "description": "Default permissions for ScreenCast Receiver",
  "windows": ["main", "settings"],
  "permissions": [
    "core:window:default",
    "core:window:allow-close",
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-start-dragging",
    "core:window:allow-set-fullscreen",
    "core:window:allow-is-fullscreen",
    "core:window:allow-hide",
    "core:window:allow-show",
    "core:window:allow-set-size",
    "core:window:allow-internal-toggle-maximize",
    "core:webview:default",
    "libmpv:default",
    "global-shortcut:default",
    "autostart:default"
  ]
}
```

## 附录 D：DLNA Renderer 最小响应模板

```xml
<!-- 设备描述文档 (device-description.xml) -->
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion>
    <major>1</major>
    <minor>0</minor>
  </specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>ScreenCast Receiver</friendlyName>
    <manufacturer>YourName</manufacturer>
    <manufacturerURL>https://example.com</manufacturerURL>
    <modelName>ScreenCast Receiver</modelName>
    <modelNumber>1.0</modelNumber>
    <UDN>uuid:YOUR-UUID-HERE</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
        <controlURL>/upnp/control/AVTransport</controlURL>
        <eventSubURL>/upnp/event/AVTransport</eventSubURL>
        <SCPDURL>/upnp/scpd/AVTransport.xml</SCPDURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
        <controlURL>/upnp/control/RenderingControl</controlURL>
        <eventSubURL>/upnp/event/RenderingControl</eventSubURL>
        <SCPDURL>/upnp/scpd/RenderingControl.xml</SCPDURL>
      </service>
    </serviceList>
  </device>
</root>
```

---

**文档结束。AI 执行时从第一节开始逐节遵守，不得跳节、不得省略。**
