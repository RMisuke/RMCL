/**
 * 简体中文语言包 / Simplified Chinese locale bundle.
 *
 * 键名与 `en.ts` 完全一致，缺少或多出键都会在 `pnpm typecheck` 时报错。
 * 占位符沿用 `{name}` 形式。
 */
import type { Messages } from "./messages";

export const zhCN: Messages = {
  // ── 通用 ──
  "common.back": "← 返回",
  "common.cancel": "取消",

  // ── 标题栏 / 窗口按钮 ──
  "titlebar.minimize": "最小化",
  "titlebar.maximize": "最大化",
  "titlebar.close": "关闭",

  // ── 登录 ──
  "login.subtitle": "Minecraft 启动器",
  "login.microsoft.submit": "使用 Microsoft 登录",
  "login.microsoft.busy": "正在连接…",
  "login.deviceCode.title": "打开下面的网址并输入验证码",
  "login.deviceCode.codeLabel": "验证码",
  "login.divider": "或",
  "login.offline.placeholder": "输入用户名",
  "login.offline.submit": "离线登录",
  "login.offline.busy": "正在登录…",
  "login.error.usernameRequired": "请输入用户名",

  // ── 设置页：页面框架 ──
  "settings.title": "设置",
  "settings.loading": "正在加载设置…",
  "settings.saved": "设置已保存",
  "settings.save": "保存设置",
  "settings.saving": "正在保存…",

  // ── 设置页：语言 ──
  "settings.language.title": "语言",
  "settings.language.desc": "界面语言。选择“跟随系统”即与操作系统语言保持一致。",
  "settings.language.system": "跟随系统",
  "settings.language.systemWith": "跟随系统（{language}）",
  "settings.language.loading": "正在加载语言列表…",

  // ── 设置页：游戏目录 ──
  "settings.gameDir.title": "游戏目录",
  "settings.gameDir.desc":
    "所有 Minecraft 数据的根目录——版本、资源、依赖库与模组。采用 Mojang 标准目录结构。",
  "settings.gameDir.label": "游戏根目录（相当于 .minecraft）",
  "settings.gameDir.hint": "绝对路径。留空则使用默认值（{default}）。",
  "settings.gameDir.placeholder": "/path/to/minecraft",

  // ── 设置页：Java 运行时 ──
  "settings.java.title": "Java 运行时",
  "settings.java.redetect": "重新检测",
  "settings.java.detecting": "正在检测…",
  "settings.java.empty": "未检测到 Java。请安装 Java 8 及以上版本以运行 Minecraft。",
  "settings.java.badge": "Java {version}",
  "settings.java.arch64": " · 64 位",
  "settings.java.arch32": " · 32 位",
  "settings.java.customLabel": "自定义 Java 路径",
  "settings.java.customHint": "或手动指定 Java 可执行文件",
  "settings.java.customPlaceholder": "/usr/bin/java",

  // ── 设置页：JVM 内存 ──
  "settings.memory.title": "JVM 内存",
  "settings.memory.desc": "所有实例的默认堆大小。单个实例可单独覆盖。",
  "settings.memory.min": "最小堆（-Xms）",
  "settings.memory.max": "最大堆（-Xmx）",
  "settings.memory.unit": "MB",

  // ── 仪表盘：账号面板 ──
  "dashboard.account.loadingSession": "正在加载账号信息…",
  "dashboard.account.redirecting": "正在跳转到登录页…",
  "dashboard.account.providerMicrosoft": "Microsoft",
  "dashboard.account.providerOffline": "离线",
  "dashboard.account.newProfile": "+ 新建实例",
  "dashboard.account.settings": "⚙ 设置",
  "dashboard.account.logout": "退出登录",

  // ── 仪表盘：实例 ──
  "dashboard.profile.title": "实例",
  "dashboard.profile.gameVersion": "MC {version}",
  "dashboard.profile.add": "+ 添加",
  "dashboard.profile.loading": "正在加载实例…",
  "dashboard.profile.empty": "还没有实例",
  "dashboard.profile.emptyAction": "创建第一个实例，开始游戏吧",
  "dashboard.profile.emptyButton": "新建实例",
  "dashboard.profile.delete": "删除实例",
  "dashboard.profile.create.title": "新建实例",
  "dashboard.profile.create.placeholder": "实例名称（例如：生存）",
  "dashboard.profile.create.versionLabel": "游戏版本",
  "dashboard.profile.create.versionPlaceholder": "请选择版本…",
  "dashboard.profile.create.includeSnapshots": "包含快照版本",
  "dashboard.profile.create.submit": "创建并下载",
  "dashboard.profile.create.busy": "正在创建…",

  // ── 仪表盘：实例时间 ──
  "dashboard.time.never": "从未",
  "dashboard.time.justNow": "刚刚",
  "dashboard.time.minutesAgo": "{count} 分钟前",
  "dashboard.time.hoursAgo": "{count} 小时前",
  "dashboard.time.unknown": "未知",

  // ── 仪表盘：版本 ──
  "dashboard.version.title": "版本",
  "dashboard.version.loading": "正在加载版本…",
  "dashboard.version.empty": "未找到版本",
  "dashboard.version.downloading": "正在下载版本…",
  "dashboard.version.installing": "正在安装…",
  "dashboard.version.installed": "已安装 {version}",
  "dashboard.version.installedBadge": "已安装",
  "dashboard.version.downloadFailed": "下载失败",
  "dashboard.version.releases": "正式版",
  "dashboard.version.snapshots": "快照",
  "dashboard.version.all": "全部",
  "dashboard.version.release": "正式版",
  "dashboard.version.snapshot": "快照",
  "dashboard.version.old": "旧版",

  // ── 仪表盘：启动栏 ──
  "dashboard.launch.profile": "实例",
  "dashboard.launch.noProfile": "未选择实例",
  "dashboard.launch.profileDetail": "{name} · MC {version}",
  "dashboard.launch.memory": "内存",
  "dashboard.launch.memoryValue": "{size} MB",
  "dashboard.launch.start": "启动游戏",
  "dashboard.launch.stop": "停止游戏",

  // ── 仪表盘：游戏输出 ──
  "dashboard.output.title": "游戏输出",
  "dashboard.output.waiting": "等待游戏输出…",
  "dashboard.output.exited": "[游戏已退出，退出码 {code}]",
};
