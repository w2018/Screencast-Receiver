# 生成投屏助手应用图标（1024x1024 PNG）
# 设计：深色渐变圆角背景 + 显示器 + 播放三角 + 投屏信号弧线
from PIL import Image, ImageDraw

SIZE = 1024
img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# 圆角背景（深蓝紫渐变）
BG_TOP = (26, 26, 46, 255)
BG_BOT = (49, 46, 129, 255)
for y in range(SIZE):
    t = y / SIZE
    r = int(BG_TOP[0] + (BG_BOT[0] - BG_TOP[0]) * t)
    g = int(BG_TOP[1] + (BG_BOT[1] - BG_TOP[1]) * t)
    b = int(BG_TOP[2] + (BG_BOT[2] - BG_TOP[2]) * t)
    d.line([(40, y), (SIZE - 40, y)], fill=(r, g, b, 255))

# 用圆角遮罩裁剪背景
mask = Image.new("L", (SIZE, SIZE), 0)
md = ImageDraw.Draw(mask)
md.rounded_rectangle([40, 40, SIZE - 40, SIZE - 40], radius=200, fill=255)
img.putalpha(mask)

# ===== 显示器 =====
# 外边框
d.rounded_rectangle([250, 220, 774, 640], radius=40, fill=(15, 15, 28, 255))
# 屏幕（内屏，渐变蓝）
for i, y in enumerate(range(272, 590)):
    t = (y - 272) / 318
    col = (int(99 + (79 - 99) * t), int(102 + (70 - 102) * t), int(241 + (229 - 241) * t), 255)
    d.line([(272, y), (752, y)], fill=col)
# 屏幕内高光
d.ellipse([280, 280, 420, 380], fill=(130, 140, 255, 40))

# 播放三角（白色）
tri = [(420, 380), (420, 500), (580, 440)]
d.polygon(tri, fill=(255, 255, 255, 255))

# ===== 支架 =====
d.rounded_rectangle([486, 640, 538, 700], radius=26, fill=(60, 60, 100, 255))
d.rounded_rectangle([360, 700, 664, 736], radius=18, fill=(60, 60, 100, 255))

# ===== 底部投屏信号弧线 =====
# 从显示器底座向下的 WIFI 弧线（信号波）
for wave, w, h_off in [(0, 180, 120), (1, 130, 190), (2, 78, 250)]:
    cx, cy, r = SIZE // 2, 830, 60 + wave * 55
    # 画下半圆弧
    d.arc([cx - r, cy - r, cx + r, cy + r], start=25, end=155, fill=(99, 102, 241, 255), width=26)

# 保存
img.save(r"D:\ClaudeCode_Work\Screencast-Receiver\src-tauri\app-icon.png")
print("图标已生成: src-tauri/app-icon.png")
