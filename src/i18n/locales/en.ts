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
} as const;
