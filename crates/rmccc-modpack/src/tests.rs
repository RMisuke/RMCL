use super::*;
use std::io::Write;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rmccc-modpack-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn curse_zip() -> (PathBuf, PathBuf) {
    let dir = temp_dir("curse");
    let path = dir.join("pack.zip");
    let manifest = serde_json::json!({
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": "Test Curse Pack",
        "version": "1.2.3",
        "author": "Someone",
        "overrides": "overrides",
        "minecraft": {
            "version": "1.20.1",
            "modLoaders": [{"id": "forge-47.2.0", "primary": true}]
        },
        "files": [{"projectID": 238222, "fileID": 9362743, "required": true}]
    });
    write_zip(
        &path,
        &[
            ("manifest.json", manifest.to_string().as_bytes()),
            (
                "modlist.html",
                b"<html><body><ul><li>Just Enough Items</li></ul></body></html>",
            ),
            ("overrides/config/test.cfg", b"a=b\n"),
            ("icon.png", b"PNG"),
        ],
    );
    (dir, path)
}

#[test]
fn parses_curseforge_manifest() {
    let (dir, path) = curse_zip();
    let pack = read_modpack(&path).unwrap();
    assert_eq!(pack.kind, ModpackKind::Curse);
    assert_eq!(pack.name, "Test Curse Pack");
    assert_eq!(pack.game_version.as_deref(), Some("1.20.1"));
    assert_eq!(pack.loaders.len(), 1);
    assert_eq!(pack.loaders[0].kind, LoaderKind::Forge);
    assert_eq!(pack.loaders[0].version, "47.2.0");
    assert_eq!(pack.override_dirs, vec!["overrides"]);
    assert_eq!(pack.files.len(), 1);
    assert_eq!(pack.files[0].path, None);
    assert!(pack.description.contains("Just Enough Items"));
    assert!(pack.icon.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parses_modrinth_manifest_and_skips_server_unsupported() {
    let dir = temp_dir("modrinth");
    let path = dir.join("pack.mrpack");
    let manifest = serde_json::json!({
        "game": "minecraft",
        "formatVersion": 1,
        "versionId": "v1",
        "name": "Test Mrpack",
        "summary": "Summary",
        "files": [
            {"path": "mods/client.jar", "hashes": {"sha1": "a".repeat(40), "sha512": "b"}, "downloads": ["https://cdn.modrinth.com/client.jar"], "fileSize": 10},
            {"path": "mods/server.jar", "hashes": {"sha1": "c".repeat(40)}, "env": {"client": "unsupported"}, "downloads": ["https://cdn.modrinth.com/server.jar"], "fileSize": 10}
        ],
        "dependencies": {"minecraft": "1.20.1", "fabric-loader": "0.15.11"}
    });
    write_zip(
        &path,
        &[
            ("modrinth.index.json", manifest.to_string().as_bytes()),
            ("client-overrides/config/a.txt", b"x"),
            ("overrides/mods/extra.txt", b"y"),
        ],
    );
    let pack = read_modpack(&path).unwrap();
    assert_eq!(pack.kind, ModpackKind::Modrinth);
    assert_eq!(pack.files.len(), 1);
    assert_eq!(pack.files[0].path.as_deref(), Some("mods/client.jar"));
    assert_eq!(pack.override_dirs, vec!["client-overrides", "overrides"]);
    assert_eq!(pack.loaders[0].kind, LoaderKind::Fabric);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parses_mcbbs_manifest() {
    let dir = temp_dir("mcbbs");
    let path = dir.join("pack.zip");
    let manifest = serde_json::json!({
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": "Test MCBBS Pack",
        "version": "2",
        "author": "A",
        "description": "D",
        "fileApi": "https://example.com/api",
        "addons": [
            {"id": "game", "version": "1.12.2"},
            {"id": "forge", "version": "14.23.5.2860"}
        ],
        "libraries": [],
        "files": [
            {"type": "addon", "force": false, "path": "mods/a.jar", "hash": "0123456789abcdef0123456789abcdef01234567"},
            {"type": "curse", "force": false, "projectID": 1, "fileID": 2, "fileName": "b.jar", "url": "https://example.com/b.jar"}
        ],
        "settings": {"install_mods": true, "install_resourcepack": true},
        "launchInfo": {"minMemory": 4096, "javaArgument": ["-XX:+UseG1GC"], "launchArgument": ["-Dfoo=bar"]}
    });
    write_zip(
        &path,
        &[
            ("manifest.json", manifest.to_string().as_bytes()),
            ("overrides/config/x.txt", b"x"),
        ],
    );
    let pack = read_modpack(&path).unwrap();
    assert_eq!(pack.kind, ModpackKind::Mcbbs);
    assert_eq!(pack.game_version.as_deref(), Some("1.12.2"));
    assert_eq!(pack.loaders[0].kind, LoaderKind::Forge);
    assert_eq!(pack.files.len(), 2);
    assert_eq!(
        pack.settings.memory.and_then(|memory| memory.max_mb),
        Some(4096)
    );
    assert_eq!(
        pack.settings.jvm_args.as_deref(),
        Some(&["-XX:+UseG1GC".to_string()][..])
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parses_server_manifest() {
    let dir = temp_dir("server");
    let path = dir.join("pack.zip");
    let manifest = serde_json::json!({
        "name": "Server Pack",
        "author": "B",
        "version": "3",
        "description": "S",
        "fileApi": "https://example.com/files/",
        "files": [{"path": "mods/a.jar", "hash": "0123456789abcdef0123456789abcdef01234567"}],
        "addons": [{"id": "game", "version": "1.19.2"}, {"id": "fabric", "version": "0.15.0"}]
    });
    write_zip(
        &path,
        &[
            ("server-manifest.json", manifest.to_string().as_bytes()),
            ("overrides/mods/a.jar", b"jar"),
        ],
    );
    let pack = read_modpack(&path).unwrap();
    assert_eq!(pack.kind, ModpackKind::Server);
    assert_eq!(
        pack.files[0].urls[0],
        "https://example.com/files/overrides/mods/a.jar"
    );
    assert_eq!(pack.loaders[0].kind, LoaderKind::Fabric);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parses_multimc_nested_root() {
    let dir = temp_dir("multimc");
    let path = dir.join("pack.zip");
    let cfg = "InstanceType=OneSix\nname=MMC Pack\nIntendedVersion=1.20.1\nOverrideMemory=true\nMaxMemAlloc=4096\nOverrideWindow=true\nMinecraftWinWidth=1280\nMinecraftWinHeight=720\niconKey=mmc\n";
    let mmc = serde_json::json!({
        "formatVersion": 1,
        "components": [
            {"uid": "net.minecraft", "version": "1.20.1"},
            {"uid": "net.fabricmc.fabric-loader", "version": "0.15.11"}
        ]
    });
    write_zip(
        &path,
        &[
            ("MMC Pack/instance.cfg", cfg.as_bytes()),
            ("MMC Pack/mmc-pack.json", mmc.to_string().as_bytes()),
            ("MMC Pack/.minecraft/mods/a.jar", b"jar"),
            ("MMC Pack/mmc.png", b"PNG"),
        ],
    );
    let pack = read_modpack(&path).unwrap();
    assert_eq!(pack.kind, ModpackKind::MultiMc);
    assert_eq!(pack.name, "MMC Pack");
    assert_eq!(pack.override_dirs, vec!["MMC Pack/.minecraft"]);
    assert_eq!(pack.loaders[0].kind, LoaderKind::Fabric);
    assert_eq!(pack.settings.memory.and_then(|m| m.max_mb), Some(4096));
    assert!(pack.icon.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parses_hmcl_export() {
    let dir = temp_dir("hmcl");
    let path = dir.join("pack.zip");
    let manifest = serde_json::json!({
        "name": "HMCL Pack",
        "author": "C",
        "version": "1",
        "gameVersion": "1.20.1",
        "description": "H"
    });
    let pack_json = serde_json::json!({
        "id": "HMCL Pack",
        "type": "release",
        "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
        "assetIndex": {"id": "5", "url": "https://example.com/a.json", "sha1": "a", "size": 1, "totalSize": 1},
        "downloads": {},
        "libraries": [
            {"name": "net.fabricmc:fabric-loader:0.15.11", "downloads": {"artifact": {"url": "https://example.com/f.jar", "sha1": "a", "size": 1}}}
        ]
    });
    write_zip(
        &path,
        &[
            ("modpack.json", manifest.to_string().as_bytes()),
            ("minecraft/pack.json", pack_json.to_string().as_bytes()),
            ("minecraft/config/a.txt", b"a"),
        ],
    );
    let pack = read_modpack(&path).unwrap();
    assert_eq!(pack.kind, ModpackKind::Hmcl);
    assert_eq!(pack.game_version.as_deref(), Some("1.20.1"));
    assert_eq!(pack.loaders[0].kind, LoaderKind::Fabric);
    assert_eq!(pack.loaders[0].version, "0.15.11");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn detects_manual_minecraft_directory() {
    let dir = temp_dir("manual");
    let path = dir.join("manual.zip");
    write_zip(
        &path,
        &[
            ("MyPack/.minecraft/versions/1.20.1/1.20.1.json", b"{}"),
            ("MyPack/.minecraft/mods/a.jar", b"jar"),
        ],
    );
    let pack = read_modpack(&path).unwrap();
    assert_eq!(pack.kind, ModpackKind::Manual);
    assert_eq!(pack.manual_root.as_deref(), Some("MyPack/.minecraft"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn override_update_preserves_user_edits() {
    let dir = temp_dir("overrides");
    let old_zip = dir.join("old.zip");
    let new_zip = dir.join("new.zip");
    write_zip(
        &old_zip,
        &[
            ("overrides/config/keep.txt", b"old"),
            ("overrides/config/user.txt", b"old-user"),
            ("overrides/config/gone.txt", b"gone"),
        ],
    );
    let run = dir.join("run");
    let previous = extract_overrides(
        &old_zip,
        &run,
        &["overrides".to_string()],
        &[],
        &|_| false,
        None,
    )
    .unwrap();
    std::fs::write(run.join("config/user.txt"), b"edited").unwrap();
    std::fs::remove_file(run.join("config/keep.txt")).unwrap();

    write_zip(
        &new_zip,
        &[
            ("overrides/config/keep.txt", b"new"),
            ("overrides/config/user.txt", b"new-user"),
            ("overrides/config/added.txt", b"added"),
        ],
    );
    let next = extract_overrides(
        &new_zip,
        &run,
        &["overrides".to_string()],
        &previous,
        &|_| false,
        None,
    )
    .unwrap();
    assert!(
        !run.join("config/keep.txt").exists(),
        "user-deleted file stays missing"
    );
    assert_eq!(
        std::fs::read(run.join("config/user.txt")).unwrap(),
        b"edited",
        "user-edited file is preserved"
    );
    assert_eq!(
        std::fs::read(run.join("config/added.txt")).unwrap(),
        b"added"
    );
    assert!(
        !run.join("config/gone.txt").exists(),
        "removed pack file is deleted"
    );
    assert!(next.iter().any(|entry| entry.path == "config/added.txt"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn suggests_safe_instance_ids() {
    assert_eq!(suggest_instance_id("My Pack", "fallback"), "My Pack");
    assert_eq!(suggest_instance_id("a/b:c", "fallback"), "a-b-c");
    assert_eq!(suggest_instance_id("   ", "fallback"), "fallback");
    assert_eq!(suggest_instance_id("CON", "fallback"), "CON-pack");
    let long = "汉".repeat(100);
    assert!(suggest_instance_id(&long, "fallback").len() <= 128);
}
