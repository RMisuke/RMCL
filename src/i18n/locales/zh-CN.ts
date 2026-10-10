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
  "dashboard.profile.settings": "实例设置",

  // ── 实例设置页 ──
  "instanceSettings.title": "实例设置",
  "instanceSettings.subtitle": "{name} 的独立设置",
  "instanceSettings.back": "← 返回",
  "instanceSettings.save": "保存设置",
  "instanceSettings.saving": "正在保存…",
  "instanceSettings.saved": "实例设置已保存",
  "instanceSettings.notFound": "未找到该实例",
  "instanceSettings.loading": "正在加载实例…",
  "instanceSettings.globalDefaultsHint":
    "保持“继承”的项使用全局游戏默认值，可在设置页修改。",

  "instanceSettings.section.general": "常规",
  "instanceSettings.field.name": "实例名称",
  "instanceSettings.field.gameVersion": "游戏版本",
  "instanceSettings.field.icon": "图标",

  "instanceSettings.section.isolation": "版本隔离",
  "instanceSettings.isolation.subtitle": "让该实例使用独立的运行目录，而不是共享游戏根目录",
  "instanceSettings.isolation.runDir": "运行目录：{path}",
  "instanceSettings.isolation.never": "从不——共享游戏根目录",
  "instanceSettings.isolation.modded": "仅模组——装了模组加载器才独立",
  "instanceSettings.isolation.always": "始终——使用独立目录",

  "instanceSettings.isolation.inherit": "跟随全局默认值",
  "instanceSettings.loader.loadVersions": "加载版本",
  "instanceSettings.loader.stable": "正式版",
  "instanceSettings.loader.prerelease": "预发布版",
  "instanceSettings.section.java": "Java",
  "instanceSettings.java.path": "Java 可执行文件",
  "instanceSettings.java.auto": "自动检测",
  "instanceSettings.java.detected": "已检测到的 Java",

  "instanceSettings.section.memory": "内存与参数",
  "instanceSettings.memory.max": "最大堆内存（-Xmx）",
  "instanceSettings.memory.min": "最小堆内存（-Xms）",
  "instanceSettings.memory.metaspace": "Metaspace",
  "instanceSettings.args.jvm": "JVM 参数",
  "instanceSettings.args.game": "游戏参数",
  "instanceSettings.args.overrideJvm": "强制 JVM 参数",
  "instanceSettings.args.agents": "Java Agent",
  "instanceSettings.args.unit": "MB",
  "instanceSettings.args.onePerLine": "每行一条",

  "instanceSettings.section.window": "窗口与进程",
  "instanceSettings.window.width": "窗口宽度",
  "instanceSettings.window.height": "窗口高度",
  "instanceSettings.window.fullscreen": "以全屏启动",
  "instanceSettings.window.priority": "进程优先级",
  "instanceSettings.window.priority.low": "低",
  "instanceSettings.window.priority.below_normal": "低于正常",
  "instanceSettings.window.priority.normal": "正常",
  "instanceSettings.window.priority.above_normal": "高于正常",
  "instanceSettings.window.priority.high": "高",

  "instanceSettings.section.loader": "模组加载器",
  "instanceSettings.loader.current": "已安装的加载器",
  "instanceSettings.loader.none": "原版（无加载器）",
  "instanceSettings.loader.select": "加载器",
  "instanceSettings.loader.version": "加载器版本",
  "instanceSettings.loader.install": "安装加载器",
  "instanceSettings.loader.installing": "正在安装…",
  "instanceSettings.loader.installed": "模组加载器已安装",
  "instanceSettings.loader.loading": "正在加载版本…",
  "instanceSettings.loader.noneAvailable": "没有可用版本",

  "instanceSettings.section.mods": "模组",
  "instanceSettings.mods.empty": "尚未安装模组",
  "instanceSettings.mods.enabled": "已启用",
  "instanceSettings.mods.disabled": "已禁用",
  "instanceSettings.mods.enable": "启用",
  "instanceSettings.mods.disable": "禁用",
  "instanceSettings.mods.remove": "删除",
  "instanceSettings.mods.removed": "模组已删除",
  "instanceSettings.mods.toggled": "模组状态已更新",
  "instanceSettings.mods.loading": "正在加载模组…",
  "instanceSettings.mods.folder": "模组目录：{path}",

  "instanceSettings.section.advanced": "高级",
  "instanceSettings.advanced.envVars": "环境变量",
  "instanceSettings.advanced.envVars.hint": "每行一条 KEY=VALUE",
  "instanceSettings.advanced.wrapper": "包装命令",
  "instanceSettings.advanced.preLaunch": "启动前执行命令",
  "instanceSettings.advanced.postExit": "退出后执行命令",
  "instanceSettings.advanced.debugLog": "输出调试日志",
  "instanceSettings.advanced.customNatives": "使用自定义本地库",
  "instanceSettings.advanced.nativeGlfw": "使用系统 GLFW",
  "instanceSettings.advanced.nativeOpenal": "使用系统 OpenAL",
  "instanceSettings.advanced.noGeneratedJvmArgs": "不添加默认 JVM 参数",
  "instanceSettings.advanced.noOptimizingJvmArgs": "不添加默认优化 JVM 参数",

  "instanceSettings.inherit": "当前继承全局默认值——点击改为独立设置",
  "instanceSettings.overridden": "当前使用实例独立设置——点击改为继承",

  // ── 全局游戏默认值（设置页）──
  "settings.globalGame.title": "全局游戏默认值",
  "settings.globalGame.desc": "供“继承”的实例作为回退值。每个实例都可以在自己的设置页覆盖任意一项。",
  "settings.globalGame.jvmArgs": "默认 JVM 参数",
  "settings.globalGame.gameArgs": "默认游戏参数",
  "settings.globalGame.isolation": "默认隔离策略",
  "settings.globalGame.windowWidth": "默认窗口宽度",
  "settings.globalGame.windowHeight": "默认窗口高度",
  "settings.globalGame.fullscreen": "以全屏启动",
  "settings.globalGame.priority": "默认进程优先级",
  "settings.globalGame.autoMemory": "由启动器自动选择内存",
  "settings.globalGame.onePerLine": "每行一条",

};
