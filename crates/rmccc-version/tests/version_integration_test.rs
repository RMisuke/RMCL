//! Integration tests for the version management crate.
//!
//! Uses wiremock to simulate Mojang API responses, and drives the real
//! `MojangVersionProvider` through its overridable `version_manifest_url`
//! (P2-22：真正覆盖 provider，而非仅验证 wiremock 本身).

use rmccc_version::{MojangVersionProvider, VersionProvider};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Returns a valid mock Mojang version manifest JSON.
fn make_manifest(server: &MockServer) -> String {
    let base = server.uri();
    serde_json::json!({
        "latest": { "release": "1.21", "snapshot": "24w40a" },
        "versions": [
            { "id": "1.21", "type": "release", "url": format!("{base}/v1/packages/abc/1.21.json"), "releaseTime": "2024-06-12T00:00:00Z" },
            { "id": "1.20.1", "type": "release", "url": format!("{base}/v1/packages/def/1.20.1.json"), "releaseTime": "2023-06-07T00:00:00Z" },
            { "id": "24w40a", "type": "snapshot", "url": format!("{base}/v1/packages/ghi/24w40a.json"), "releaseTime": "2024-10-01T00:00:00Z" }
        ]
    })
    .to_string()
}

const VERSION_1_21_JSON: &str = r#"{
    "id": "1.21",
    "type": "release",
    "mainClass": "net.minecraft.client.main.Main",
    "downloads": {
        "client": { "sha1": "abc123", "size": 1000000, "url": "https://piston-data.mojang.com/v1/objects/abc/client.jar" }
    },
    "assetIndex": { "id": "17", "sha1": "def456", "size": 200000, "url": "https://piston-meta.mojang.com/v1/packages/def/17.json" },
    "javaVersion": { "component": "java-runtime-delta", "majorVersion": 21 },
    "libraries": []
}"#;

fn provider_for(server: &MockServer) -> MojangVersionProvider {
    let client = reqwest::Client::new();
    let mut provider = MojangVersionProvider::new(client);
    provider.version_manifest_url = format!("{}/mc/game/version_manifest.json", server.uri());
    provider
}

async fn mount_manifest(server: &MockServer) {
    let manifest_json = make_manifest(server);
    Mock::given(method("GET"))
        .and(path("/mc/game/version_manifest.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(manifest_json)
                .insert_header("content-type", "application/json"),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn test_fetch_versions_from_manifest() {
    let server = MockServer::start().await;
    mount_manifest(&server).await;

    // P2-22：真正构造 provider 指向 mock，验证 fetch_versions 解析
    let provider = provider_for(&server);
    let versions = provider
        .fetch_versions()
        .await
        .expect("provider should parse manifest");
    assert_eq!(versions.len(), 3);
    assert!(versions.iter().any(|v| v.id == "1.21"));
    assert!(versions
        .iter()
        .any(|v| v.release_type == rmccc_version::ReleaseType::Snapshot));
}

#[tokio::test]
async fn test_fetch_single_version() {
    let server = MockServer::start().await;
    mount_manifest(&server).await;

    Mock::given(method("GET"))
        .and(path("/v1/packages/abc/1.21.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(VERSION_1_21_JSON)
                .insert_header("content-type", "application/json"),
        )
        .mount(&server)
        .await;

    // P2-22：走 provider 的 fetch_version（内部先经 mock manifest 定位 URL）
    let provider = provider_for(&server);
    let gv = provider
        .fetch_version("1.21")
        .await
        .expect("provider should fetch version detail");
    assert_eq!(gv.id, "1.21");
    assert_eq!(gv.main_class, "net.minecraft.client.main.Main");
    assert_eq!(gv.java_version.map(|j| j.major_version), Some(21));
}
