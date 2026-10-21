/**
 * English UI strings — the reference locale.
 *
 * Every other bundle is typed against this object, so a missing or misspelled
 * key is a compile error. Keys are flat and dotted (`domain.section.item`) to
 * keep `keyof typeof en` a plain string-literal union.
 *
 * Placeholders use `{name}` and are substituted by `t(key, { name: value })`.
 */
export const en = {
  // ── Shared ──
  "common.back": "← Back",
  "common.cancel": "Cancel",
  "common.loading": "Loading...",

  // ── Title bar / window controls ──
  "titlebar.minimize": "Minimize",
  "titlebar.maximize": "Maximize",
  "titlebar.close": "Close",

  // ── Login ──
  "login.subtitle": "Minecraft Launcher",
  "login.microsoft.submit": "Sign in with Microsoft",
  "login.microsoft.busy": "Connecting...",
  "login.deviceCode.title": "Open the URL below and enter the code",
  "login.deviceCode.codeLabel": "Code",
  "login.divider": "or",
  "login.offline.placeholder": "Enter username",
  "login.offline.submit": "Offline Login",
  "login.offline.busy": "Logging in...",
  "login.error.usernameRequired": "Please enter a username",

  // ── Settings: page chrome ──
  "settings.title": "Settings",
  "settings.loading": "Loading settings...",
  "settings.saved": "Settings saved successfully",
  "settings.save": "Save Settings",
  "settings.saving": "Saving...",

  // ── Settings: language ──
  "settings.language.title": "Language",
  "settings.language.desc":
    "Interface language. Choose “Follow system” to match your operating system.",
  "settings.language.system": "Follow system",
  "settings.language.systemWith": "Follow system ({language})",
  "settings.language.loading": "Loading languages...",

  // ── Settings: game directory ──
  "settings.gameDir.title": "Game Directory",
  "settings.gameDir.desc":
    "Root directory for all Minecraft data — versions, assets, libraries, and mods. Follows Mojang standard layout.",
  "settings.gameDir.label": "Game Root (.minecraft equivalent)",
  "settings.gameDir.hint": "Absolute path. Leave empty for default ({default}).",
  "settings.gameDir.placeholder": "/path/to/minecraft",

  // ── Settings: Java runtime ──
  "settings.java.title": "Java Runtime",
  "settings.java.redetect": "Re-detect",
  "settings.java.detecting": "Detecting...",
  "settings.java.empty":
    "No Java installations detected. Install Java 8+ to run Minecraft.",
  "settings.java.badge": "Java {version}",
  "settings.java.arch64": " · 64-bit",
  "settings.java.arch32": " · 32-bit",
  "settings.java.customLabel": "Custom Java Path",
  "settings.java.customHint": "Or manually specify the Java executable",
  "settings.java.customPlaceholder": "/usr/bin/java",

  // ── Settings: JVM memory ──
  "settings.memory.title": "JVM Memory",
  "settings.memory.desc":
    "Default heap size for all profiles. Can be overridden per profile.",
  "settings.memory.min": "Min Heap (-Xms)",
  "settings.memory.max": "Max Heap (-Xmx)",
  "settings.memory.unit": "MB",

  // ── Dashboard: account panel ──
  "dashboard.account.loadingSession": "Loading session...",
  "dashboard.account.redirecting": "Redirecting to login...",
  "dashboard.account.providerMicrosoft": "Microsoft",
  "dashboard.account.providerOffline": "Offline",
  "dashboard.account.newProfile": "+ New Profile",
  "dashboard.account.settings": "⚙ Settings",
  "dashboard.account.logout": "Log out",

  // ── Dashboard: profiles ──
  "dashboard.profile.title": "Profiles",
  "dashboard.profile.gameVersion": "MC {version}",
  "dashboard.profile.add": "+ Add",
  "dashboard.profile.loading": "Loading profiles...",
  "dashboard.profile.empty": "No profiles yet",
  "dashboard.profile.emptyAction": "Create your first profile to get started",
  "dashboard.profile.emptyButton": "Create Profile",
  "dashboard.profile.delete": "Delete profile",
  "dashboard.profile.create.title": "Create Profile",
  "dashboard.profile.create.placeholder": "Profile name (e.g. Survival)",
  "dashboard.profile.create.versionLabel": "Game version",
  "dashboard.profile.create.versionPlaceholder": "Select a version…",
  "dashboard.profile.create.includeSnapshots": "Include snapshots",
  "dashboard.profile.create.submit": "Create & download",
  "dashboard.profile.create.busy": "Creating...",

  // ── Dashboard: profile timestamps ──
  "dashboard.time.never": "Never",
  "dashboard.time.justNow": "Just now",
  "dashboard.time.minutesAgo": "{count}m ago",
  "dashboard.time.hoursAgo": "{count}h ago",
  "dashboard.time.unknown": "Unknown",

  // ── Dashboard: versions ──
  "dashboard.version.title": "Versions",
  "dashboard.version.loading": "Loading versions...",
  "dashboard.version.empty": "No versions found",
  "dashboard.version.downloading": "Downloading version...",
  "dashboard.version.installing": "Installing...",
  /** Toast after a successful install. */
  "dashboard.version.installed": "{version} installed",
  /** Row label in the version list, shown under the version name already. */
  "dashboard.version.installedBadge": "Installed",
  "dashboard.version.downloadFailed": "Download failed",
  "dashboard.version.releases": "Releases",
  "dashboard.version.snapshots": "Snapshots",
  "dashboard.version.all": "All",
  "dashboard.version.release": "Release",
  "dashboard.version.snapshot": "Snapshot",
  "dashboard.version.old": "Old",

  // ── Dashboard: launch bar ──
  "dashboard.launch.profile": "Profile",
  "dashboard.launch.noProfile": "No profile selected",
  "dashboard.launch.profileDetail": "{name} · MC {version}",
  "dashboard.launch.memory": "Memory",
  "dashboard.launch.memoryValue": "{size} MB",
  "dashboard.launch.start": "LAUNCH",
  "dashboard.launch.stop": "STOP",

  // ── Dashboard: game output ──
  "dashboard.output.title": "Game Output",
  "dashboard.output.waiting": "Waiting for game output...",
  "dashboard.output.exited": "[Game exited with code {code}]",
  "dashboard.profile.settings": "Instance settings",

  // ── Instance settings page ──
  "instanceSettings.effective": "Effective: {value}",
  "instanceSettings.title": "Instance settings",
  "instanceSettings.subtitle": "Overrides for {name}",
  "instanceSettings.back": "← Back",
  "instanceSettings.save": "Save settings",
  "instanceSettings.saving": "Saving…",
  "instanceSettings.saved": "Instance settings saved",
  "instanceSettings.notFound": "Instance not found",
  "instanceSettings.loading": "Loading instance…",
  "instanceSettings.globalDefaultsHint":
    "Fields left inheriting use the global game defaults, which you can edit on the settings page.",

  "instanceSettings.section.general": "General",
  "instanceSettings.field.name": "Instance name",
  "instanceSettings.field.gameVersion": "Game version",
  "instanceSettings.field.icon": "Icon",

  "instanceSettings.section.isolation": "Instance isolation",
  "instanceSettings.isolation.subtitle":
    "Give this instance its own run directory instead of sharing the game root",
  "instanceSettings.isolation.runDir": "Run directory: {path}",
  "instanceSettings.isolation.never": "Never — share the game root",
  "instanceSettings.isolation.modded": "Modded only — own folder once a loader is installed",
  "instanceSettings.isolation.always": "Always — own folder",

  "instanceSettings.isolation.inherit": "Follow the global default",
  "instanceSettings.loader.loadVersions": "Load versions",
  "instanceSettings.loader.stable": "Stable release",
  "instanceSettings.loader.prerelease": "Pre-release",
  "instanceSettings.section.java": "Java",
  "instanceSettings.java.path": "Java executable",
  "instanceSettings.java.auto": "Auto-detect",
  "instanceSettings.java.detected": "Detected installations",

  "instanceSettings.section.memory": "Memory & arguments",
  "instanceSettings.memory.max": "Maximum heap (-Xmx)",
  "instanceSettings.memory.min": "Minimum heap (-Xms)",
  "instanceSettings.memory.metaspace": "Metaspace",
  "instanceSettings.args.jvm": "JVM arguments",
  "instanceSettings.args.game": "Game arguments",
  "instanceSettings.args.overrideJvm": "Forced JVM arguments",
  "instanceSettings.args.agents": "Java agents",
  "instanceSettings.args.unit": "MB",
  "instanceSettings.args.onePerLine": "One per line",

  "instanceSettings.section.window": "Window & process",
  "instanceSettings.window.width": "Window width",
  "instanceSettings.window.height": "Window height",
  "instanceSettings.window.fullscreen": "Start fullscreen",
  "instanceSettings.window.priority": "Process priority",
  "instanceSettings.window.priority.low": "Low",
  "instanceSettings.window.priority.below_normal": "Below normal",
  "instanceSettings.window.priority.normal": "Normal",
  "instanceSettings.window.priority.above_normal": "Above normal",
  "instanceSettings.window.priority.high": "High",

  "instanceSettings.section.loader": "Mod loader",
  "instanceSettings.loader.current": "Installed loader",
  "instanceSettings.loader.none": "Vanilla (no loader)",
  "instanceSettings.loader.select": "Loader",
  "instanceSettings.loader.version": "Loader version",
  "instanceSettings.loader.install": "Install loader",
  "instanceSettings.loader.installing": "Installing…",
  "instanceSettings.loader.installed": "Mod loader installed",
  "instanceSettings.loader.loading": "Loading versions…",
  "instanceSettings.loader.noneAvailable": "No versions available",

  "instanceSettings.section.mods": "Mods",
  "instanceSettings.mods.empty": "No mods installed",
  "instanceSettings.mods.enabled": "Enabled",
  "instanceSettings.mods.disabled": "Disabled",
  "instanceSettings.mods.enable": "Enable",
  "instanceSettings.mods.disable": "Disable",
  "instanceSettings.mods.remove": "Remove",
  "instanceSettings.mods.removed": "Mod removed",
  "instanceSettings.mods.toggled": "Mod updated",
  "instanceSettings.mods.loading": "Loading mods…",
  "instanceSettings.mods.folder": "Mods folder: {path}",

  "instanceSettings.section.advanced": "Advanced",
  "instanceSettings.advanced.envVars": "Environment variables",
  "instanceSettings.advanced.envVars.hint": "One KEY=VALUE per line",
  "instanceSettings.advanced.wrapper": "Wrapper command",
  "instanceSettings.advanced.preLaunch": "Pre-launch command",
  "instanceSettings.advanced.postExit": "Post-exit command",
  "instanceSettings.advanced.debugLog": "Enable debug log output",
  "instanceSettings.advanced.customNatives": "Use custom natives",
  "instanceSettings.advanced.nativeGlfw": "Use system GLFW",
  "instanceSettings.advanced.nativeOpenal": "Use system OpenAL",
  "instanceSettings.advanced.noGeneratedJvmArgs": "Skip generated JVM arguments",
  "instanceSettings.advanced.noOptimizingJvmArgs": "Skip generated optimizing JVM arguments",

  "instanceSettings.inherit": "Inheriting the global default — click to override",
  "instanceSettings.overridden": "Overridden for this instance — click to inherit",

  // ── Global game defaults (Settings page) ──
  "settings.globalGame.title": "Global game defaults",
  "settings.globalGame.desc":
    "Fallbacks for instances that inherit. Every instance can override any of these on its own settings page.",
  "settings.globalGame.jvmArgs": "Default JVM arguments",
  "settings.globalGame.gameArgs": "Default game arguments",
  "settings.globalGame.isolation": "Default isolation",
  "settings.globalGame.windowWidth": "Default window width",
  "settings.globalGame.windowHeight": "Default window height",
  "settings.globalGame.fullscreen": "Start fullscreen",
  "settings.globalGame.priority": "Default process priority",
  "settings.globalGame.autoMemory": "Let the launcher choose memory automatically",
  "settings.globalGame.onePerLine": "One per line",

  // ── Mod browser ──
  "mods.title": "Mod Browser",
  "mods.instance": "Install to",
  "mods.searchPlaceholder": "Search Modrinth…",
  "mods.search": "Search",
  "mods.noResults": "No mods found",
  "mods.install": "Install",
  "mods.installing": "Installing…",
  "mods.installed": "Mod installed",
  "mods.selectInstanceFirst": "Create an instance first",

  // ── Settings: accounts & logs ──
  "settings.accounts.title": "Accounts",
  "settings.accounts.desc": "Manage the accounts saved in this launcher.",
  "settings.accounts.refresh": "Refresh",
  "settings.accounts.empty": "No accounts yet",
  "settings.accounts.current": "Current",
  "settings.accounts.switch": "Switch",
  "settings.accounts.remove": "Remove",
  "settings.accounts.refreshToken": "Refresh tokens",
  "settings.accounts.switched": "Account switched",
  "settings.accounts.removed": "Account removed",
  "settings.accounts.refreshed": "Tokens refreshed",

  "settings.logs.title": "Logs",
  "settings.logs.desc": "Tail of the latest launcher log (up to 200 lines).",
  "settings.logs.load": "Load",
  "settings.logs.empty": "No log content",
  "settings.logs.clear": "Clear logs",

  // ── Instance integrity (0.5.6) ──
  "instanceSettings.integrity.title": "Integrity",
  "instanceSettings.integrity.desc": "Check for missing game files and repair them.",
  "instanceSettings.integrity.check": "Check",
  "instanceSettings.integrity.repair": "Repair",
  "instanceSettings.integrity.repaired": "Repair completed",
  "instanceSettings.integrity.missing": "{count} file(s) missing",

  // ── Instance mods: local install (0.5.4) ──
  "instanceSettings.mods.installLocal": "Install from file…",
  "instanceSettings.mods.localInstalled": "Mod installed from file",

  // ── Plugins (Phase 2) ──
  "plugins.title": "Plugins",
  "plugins.loading": "Loading plugins…",
  "plugins.install": "Install plugin…",
  "plugins.installing": "Installing…",
  "plugins.refresh": "Refresh",
  "plugins.empty": "No plugins installed",
  "plugins.emptyHint": "Install a .rmcpkg package. Phase 2 only supports declarative plugins (themes, cards); no third-party code runs.",
  "plugins.state.installed_disabled": "Installed · disabled",
  "plugins.state.enabled": "Enabled",
  "plugins.state.disabled": "Disabled",
  "plugins.state.failed": "Failed",
  "plugins.state.removed": "Removed",
  "plugins.trust.development": "Unsigned · development",
  "plugins.unsignedHint": "Phase 2 does not verify signatures. Only install packages you trust.",
  "plugins.version": "v{version}",
  "plugins.enable": "Enable",
  "plugins.disable": "Disable",
  "plugins.remove": "Uninstall",
  "plugins.removeConfirm": "Uninstall {name}? Plugin data is kept unless you choose to delete it.",
  "plugins.removeDeleteData": "Also delete plugin data",
  "plugins.rollback": "Roll back",
  "plugins.details": "Details",
  "plugins.hideDetails": "Hide details",
  "plugins.permissions.title": "Permissions",
  "plugins.permissions.declared": "Declared",
  "plugins.permissions.none": "No permissions requested",
  "plugins.permissions.granted": "Granted",
  "plugins.permissions.pending": "Pending review",
  "plugins.permissions.grant": "Grant",
  "plugins.permissions.revoke": "Revoke",
  "plugins.permissions.reviewHint": "This plugin requests sensitive permissions. Grant them explicitly before enabling.",
  "plugins.risk.low": "low risk",
  "plugins.risk.medium": "medium risk",
  "plugins.risk.high": "high risk",
  "plugins.deps.title": "Dependencies",
  "plugins.deps.none": "No dependencies",
  "plugins.deps.satisfied": "Satisfied",
  "plugins.deps.missing": "Missing",
  "plugins.deps.optional": "Optional",
  "plugins.contributions.title": "Contributions",
  "plugins.contributions.summary": "{cards} card(s) · {pages} page(s) · {themes} theme(s)",
  "plugins.config.title": "Plugin config",
  "plugins.config.desc": "Validated against config.schema.json on the backend.",
  "plugins.config.save": "Save config",
  "plugins.config.reset": "Reset to defaults",
  "plugins.config.saved": "Plugin config saved",
  "plugins.config.invalidJson": "Config must be valid JSON",
  "plugins.config.noSchema": "This plugin does not provide config.schema.json",
  "plugins.config.resetConfirm": "Reset the plugin config to schema defaults?",
  "plugins.diagnostics.title": "Plugin diagnostics",
  "plugins.installDone": "Plugin {id} installed",
  "plugins.pendingReview": "{count} permission(s) require review before enabling",
  "plugins.enabled": "Plugin enabled",
  "plugins.disabled": "Plugin disabled",
  "plugins.removed": "Plugin uninstalled",
  "plugins.rolledBack": "Rolled back",
  "plugins.permissionGranted": "Permission granted",
  "plugins.permissionRevoked": "Permission revoked",
  "plugins.actionFailed": "Plugin action failed",
  "plugins.installedAt": "Installed {date}",
  "plugins.requiredBy": "Required by",
} as const;
