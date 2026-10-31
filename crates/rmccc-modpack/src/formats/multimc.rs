//! MultiMC / Prism modpack (`instance.cfg` + `mmc-pack.json`).
//!
//! HMCL resolves MultiMC's JSON-Patch components through meta.multimc.org and
//! then rebuilds an official version manifest. RMCL instead maps the known
//! component ids onto its existing loader installers (the same outcome for all
//! mainstream packs) and imports `instance.cfg` launch settings.

use super::{finish, non_empty, normalize_loader_version};
use crate::archive::Archive;
use crate::model::{
    LoaderKind, MemorySettings, Modpack, ModpackKind, PackLoader, PackSettings, WindowSettings,
};
use rmccc_error::Result;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

const INSTANCE_CFG: &str = "instance.cfg";
const MMC_PACK: &str = "mmc-pack.json";

#[derive(Debug, Deserialize)]
struct MmcPack {
    #[serde(rename = "formatVersion", default)]
    format_version: i64,
    #[serde(default)]
    components: Vec<MmcComponent>,
}

#[derive(Debug, Deserialize)]
struct MmcComponent {
    #[serde(default)]
    uid: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(rename = "cachedName", default)]
    cached_name: Option<String>,
}

/// Detect and parse a MultiMC/Prism pack, if present.
pub(crate) fn detect(archive: &mut Archive) -> Result<Option<Modpack>> {
    let Some(prefix) = root_prefix(archive) else {
        return Ok(None);
    };
    let cfg_name = format!("{prefix}{INSTANCE_CFG}");
    let Some(cfg_text) = archive.read_text_opt(&cfg_name) else {
        return Ok(None);
    };
    let cfg = parse_properties(&cfg_text);

    let pack_name = format!("{prefix}{MMC_PACK}");
    let (pack_text, pack): (String, Option<MmcPack>) = match archive.read_text_opt(&pack_name) {
        Some(text) => {
            let parsed = serde_json::from_str::<MmcPack>(&text).ok();
            (text, parsed)
        }
        None => (String::new(), None),
    };
    if !pack_text.is_empty() && pack.is_none() {
        return Ok(None);
    }

    let mut warnings = Vec::new();
    let mut loaders = Vec::new();
    let mut game_version = None;
    if let Some(pack) = &pack {
        let _ = pack.format_version;
        for component in &pack.components {
            let uid = component.uid.trim();
            if uid.is_empty() {
                continue;
            }
            if matches!(uid, "net.minecraft" | "minecraft") {
                game_version = component
                    .version
                    .clone()
                    .map(|version| version.trim().to_string())
                    .filter(|version| !version.is_empty());
                continue;
            }
            let Some(kind) = component_loader(uid) else {
                if component.cached_name.is_some() {
                    tracing::debug!(
                        component = %uid,
                        "Ignoring unknown MultiMC component"
                    );
                }
                continue;
            };
            let Some(raw_version) = component.version.as_deref().map(str::trim) else {
                warnings.push(format!("MultiMC component {uid} has no version"));
                continue;
            };
            let game = game_version.as_deref().unwrap_or_default();
            let version = normalize_loader_version(kind, raw_version, game);
            if version.is_empty() {
                warnings.push(format!("MultiMC component {uid} has no version"));
                continue;
            }
            loaders.push(PackLoader::new(kind, version, false));
        }
    }
    if game_version.is_none() {
        game_version = non_empty(cfg.get("IntendedVersion").cloned());
    }
    let game_version = game_version.ok_or_else(|| {
        rmccc_error::RmcccError::Validation(
            "MultiMC pack is missing the net.minecraft component".to_string(),
        )
    })?;

    let mut settings = PackSettings::default();
    if cfg.get("OverrideMemory").is_some_and(|v| parse_bool(v)) {
        settings.memory = Some(MemorySettings {
            min_mb: parse_u32(cfg.get("MinMemAlloc")),
            max_mb: parse_u32(cfg.get("MaxMemAlloc")),
            perm_gen_mb: parse_u32(cfg.get("PermGen")),
            require_at_least: false,
        });
    }
    if cfg
        .get("OverrideJavaLocation")
        .is_some_and(|v| parse_bool(v))
    {
        settings.java_path = non_empty(cfg.get("JavaPath").cloned());
    }
    if cfg.get("OverrideJavaArgs").is_some_and(|v| parse_bool(v)) {
        let args = cfg
            .get("JvmArgs")
            .map(|raw| shlex::split(raw).unwrap_or_default())
            .unwrap_or_default();
        settings.jvm_args = Some(args);
    }
    if cfg.get("OverrideCommands").is_some_and(|v| parse_bool(v)) {
        settings.wrapper = non_empty(cfg.get("WrapperCommand").cloned());
        settings.pre_launch_command = non_empty(cfg.get("PreLaunchCommand").cloned());
        settings.post_exit_command = non_empty(cfg.get("PostExitCommand").cloned());
    }
    if cfg.get("OverrideConsole").is_some_and(|v| parse_bool(v)) {
        settings.show_logs = Some(cfg.get("ShowConsole").is_some_and(|v| parse_bool(v)));
    }
    if cfg.get("OverrideWindow").is_some_and(|v| parse_bool(v)) {
        settings.window = Some(WindowSettings {
            fullscreen: cfg.get("LaunchMaximized").is_some_and(|v| parse_bool(v)),
            width: parse_u32(cfg.get("MinecraftWinWidth")),
            height: parse_u32(cfg.get("MinecraftWinHeight")),
        });
    }

    let mc_dir = format!("{prefix}.minecraft");
    let mut override_dirs = Vec::new();
    if archive
        .entries()
        .iter()
        .any(|entry| entry.name == mc_dir || entry.name.starts_with(&format!("{mc_dir}/")))
    {
        override_dirs.push(mc_dir);
    }

    let mut icon = None;
    if let Some(icon_key) = non_empty(cfg.get("iconKey").cloned()) {
        let icon_name = format!("{prefix}{icon_key}.png");
        if let Some(entry) = archive.entry(&icon_name) {
            if !entry.is_dir {
                if let Ok(bytes) = archive.read_index(entry.index, 8 * 1024 * 1024) {
                    icon = Some(crate::model::PackIcon {
                        ext: "png".to_string(),
                        bytes,
                    });
                }
            }
        }
    }

    let name = prefix
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("")
        .to_string();
    let name = if name.is_empty() {
        non_empty(cfg.get("name").cloned()).unwrap_or_default()
    } else {
        name
    };

    let mut raw_manifest: Value = serde_json::from_str(&pack_text).unwrap_or(Value::Null);
    if raw_manifest.is_null() {
        raw_manifest = Value::Object(Default::default());
    }

    let mut metadata = Vec::new();
    metadata.push(crate::model::MetadataFile {
        path: INSTANCE_CFG.to_string(),
        content: cfg_text,
    });
    if !pack_text.is_empty() {
        metadata.push(crate::model::MetadataFile {
            path: MMC_PACK.to_string(),
            content: pack_text,
        });
    }

    let pack = Modpack {
        kind: ModpackKind::MultiMc,
        name,
        version: None,
        author: None,
        description: non_empty(cfg.get("notes").cloned()).unwrap_or_default(),
        game_version: Some(game_version),
        loaders,
        files: Vec::new(),
        override_dirs,
        settings,
        extra_libraries: Vec::new(),
        metadata_files: metadata,
        icon,
        raw_manifest,
        archive: std::path::PathBuf::new(),
        manual_root: None,
        warnings,
    };
    Ok(Some(finish(pack, archive)?))
}

/// Locate the archive prefix that owns `instance.cfg` (HMCL `getRootEntryName`).
fn root_prefix(archive: &Archive) -> Option<String> {
    if archive.contains(INSTANCE_CFG) {
        return Some(String::new());
    }
    archive.entries().iter().find_map(|entry| {
        let (prefix, file) = entry.name.rsplit_once('/')?;
        (file == INSTANCE_CFG && !prefix.contains('/')).then(|| format!("{prefix}/"))
    })
}

fn component_loader(uid: &str) -> Option<LoaderKind> {
    match uid {
        "net.minecraftforge" => Some(LoaderKind::Forge),
        "net.neoforged" | "net.neoforged.neoforge" => Some(LoaderKind::NeoForge),
        "net.fabricmc.fabric-loader" => Some(LoaderKind::Fabric),
        "org.quiltmc.quilt-loader" => Some(LoaderKind::Quilt),
        "com.mumfrey.liteloader" => Some(LoaderKind::LiteLoader),
        "net.legacyfabric.legacy-fabric-loader" => Some(LoaderKind::LegacyFabric),
        "net.optifine" | "optifine" => Some(LoaderKind::OptiFine),
        _ => None,
    }
}

fn parse_bool(raw: &str) -> bool {
    raw.trim().eq_ignore_ascii_case("true")
}

fn parse_u32(value: Option<&String>) -> Option<u32> {
    value?.trim().parse().ok()
}

/// Minimal `java.util.Properties` reader (instance.cfg).
fn parse_properties(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let mut key = String::new();
        let mut value = String::new();
        let mut escaped = false;
        let mut in_value = false;
        for ch in line.chars() {
            if escaped {
                key.push(match ch {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '=' | ':' if !in_value => {
                    in_value = true;
                }
                _ if in_value => value.push(ch),
                _ => key.push(ch),
            }
        }
        let key = key.trim().to_string();
        if !key.is_empty() {
            out.insert(key, value.trim().to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_parse_common_keys() {
        let props = parse_properties(
            "# comment\nname=My Pack\nMaxMemAlloc=4096\nJvmArgs=-XX:+UseG1GC -Xmx4G\n",
        );
        assert_eq!(props.get("name").map(String::as_str), Some("My Pack"));
        assert_eq!(props.get("MaxMemAlloc").map(String::as_str), Some("4096"));
        assert_eq!(
            props.get("JvmArgs").map(String::as_str),
            Some("-XX:+UseG1GC -Xmx4G")
        );
    }

    #[test]
    fn unknown_components_are_ignored() {
        assert_eq!(component_loader("org.lwjgl3"), None);
        assert_eq!(
            component_loader("net.fabricmc.fabric-loader"),
            Some(LoaderKind::Fabric)
        );
    }
}
