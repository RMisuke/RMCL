//! Embedded frontend bundle integrity check.
//!
//! The SolidJS bundle is compiled into the executable by `tauri::generate_context!`.
//! That embedding happens at **compile time**, while `dist/` is produced by
//! `pnpm build` beforehand. Two things can leave the binary with a bundle that
//! does not match what the page asks for:
//!
//! - a `cargo build` / `cargo test` that ran when `dist/` did not exist yet
//!   (embeds an empty asset map), followed by a `tauri build` that cargo does
//!   not need to recompile, and
//! - a `dist/` rebuilt after the last compile (old asset names, new document).
//!
//! Both end the same way: `index.html` is served, the `<script type="module">`
//! it points at is not in the asset map, nothing executes, and the window shows
//! a transparent rectangle with no error anywhere. `build.rs` now reruns on
//! `dist` changes to prevent the first case; this module makes any remaining
//! mismatch **loud** — it is logged and painted into the window, so the next
//! report says "asset missing, rebuild" instead of "UI is transparent".

/// Outcome of inspecting the embedded bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleReport {
    /// Whether the document Tauri serves at `/` was embedded at all.
    pub index_present: bool,
    /// Asset paths referenced by the document that the bundle does not contain.
    pub missing: Vec<String>,
}

impl BundleReport {
    /// True when the document and every asset it references were embedded.
    pub fn is_ok(&self) -> bool {
        self.index_present && self.missing.is_empty()
    }

    /// Human-readable summary for the log line / error panel.
    pub fn describe(&self) -> String {
        if !self.index_present {
            return "index.html is missing from the embedded bundle".to_string();
        }
        if self.missing.is_empty() {
            return "embedded frontend bundle is complete".to_string();
        }
        format!(
            "the embedded bundle is missing {} asset(s) referenced by index.html: {}",
            self.missing.len(),
            self.missing.join(", ")
        )
    }
}

/// Inspect the assets embedded in `app`.
///
/// In dev builds `tauri` reads `frontendDist` from disk instead of embedding it,
/// so the check only runs for release builds (`tauri build`).
pub fn inspect<R: tauri::Runtime>(app: &tauri::App<R>) -> Option<BundleReport> {
    // `dev` builds serve the bundle from `devUrl` / disk; an absent `dist/`
    // there is normal and must not be reported as a broken bundle.
    if cfg!(debug_assertions) {
        return None;
    }

    let resolver = app.asset_resolver();
    let Some(index) = resolver.get("/index.html".to_string()) else {
        return Some(BundleReport {
            index_present: false,
            missing: Vec::new(),
        });
    };

    let html = String::from_utf8_lossy(&index.bytes);
    let missing = missing_asset_refs(&html, |path| resolver.get(path.to_string()).is_some());

    Some(BundleReport {
        index_present: true,
        missing,
    })
}

/// A standalone error page shown when the embedded bundle cannot render.
///
/// Rendering happens by navigating the WebView to a `data:` URL rather than by
/// `eval`-ing into the current document: when `index.html` itself is missing the
/// WebView has no script context to evaluate in, which is exactly the case that
/// used to leave a transparent window behind.
pub fn error_page_html(report: &BundleReport) -> String {
    let mut expected = String::from("      /index.html\n");
    for path in &report.missing {
        expected.push_str("      ");
        expected.push_str(path);
        expected.push('\n');
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>RMCCC — frontend bundle incomplete</title>
<style>
  html, body {{ margin: 0; height: 100%; background: #141a2e; color: #e6e9f2; }}
  body {{
    display: flex; flex-direction: column; justify-content: center; gap: 14px;
    padding: 32px; box-sizing: border-box; text-align: left;
    font: 13px/1.6 -apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif;
  }}
  h1 {{ margin: 0; font-size: 15px; font-weight: 600; max-width: 720px; }}
  p {{ margin: 0; max-width: 720px; color: #b9bfd0; }}
  pre {{
    margin: 0; max-width: 720px; color: #8b93a8; white-space: pre-wrap;
    word-break: break-word; font: 11px/1.7 ui-monospace, Menlo, monospace;
  }}
</style>
</head>
<body>
<h1>RMCCC — the embedded frontend bundle is incomplete / 内置前端资源不完整</h1>
<p>{summary}</p>
<p>The window cannot render because the UI files were not compiled into this binary.
Rebuild the application:</p>
<pre>    pnpm tauri build --bundles app</pre>
<p>Expected assets:</p>
<pre>{expected}</pre>
<p>This is a build artefact, not a runtime fault.</p>
</body>
</html>
"#,
        summary = escape_html(&report.describe()),
    )
}

/// `data:` URL that renders [`error_page_html`] in any WebView.
pub fn error_page_url(report: &BundleReport) -> String {
    format!(
        "data:text/html;charset=utf-8,{}",
        percent_encode(&error_page_html(report))
    )
}

/// Minimal HTML escaping for the interpolated summary line.
fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Percent-encode everything that is not safe inside a `data:` URL.
///
/// `#` must be encoded (it would start a fragment) and `%` must be encoded
/// (it would start an escape), so the safe set is deliberately conservative.
fn percent_encode(input: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(input.len() * 2);
    for byte in input.as_bytes() {
        let safe = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_'
                    | b'.'
                    | b'!'
                    | b'~'
                    | b'*'
                    | b'\''
                    | b'('
                    | b')'
                    | b';'
                    | b'/'
                    | b'?'
                    | b':'
                    | b'@'
                    | b'&'
                    | b'='
                    | b'+'
                    | b'$'
                    | b','
            );
        if safe {
            out.push(char::from(*byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Collect the local asset paths an HTML document references via `src=`/`href=`.
///
/// This is deliberately a scan rather than a full HTML parser: the document is
/// generated by Vite and only needs its `<script src>` / `<link href>` resolved.
/// External URLs, inline data URLs and fragments are ignored.
pub fn missing_asset_refs(html: &str, mut exists: impl FnMut(&str) -> bool) -> Vec<String> {
    let mut missing = Vec::new();

    for attr in ["src=\"", "href=\""] {
        let mut search_from = 0;
        while let Some(found) = html[search_from..].find(attr) {
            let value_start = search_from + found + attr.len();
            let Some(len) = html[value_start..].find('"') else {
                break;
            };
            let raw = &html[value_start..value_start + len];
            search_from = value_start + len;

            if is_external_ref(raw) {
                continue;
            }
            let path = normalize_asset_ref(raw);
            if !exists(&path) {
                missing.push(path);
            }
        }
    }

    missing.sort();
    missing.dedup();
    missing
}

/// True for references the bundle is not expected to contain.
fn is_external_ref(raw: &str) -> bool {
    let value = raw.trim();
    value.is_empty()
        || value.starts_with('#')
        || value.starts_with("data:")
        || value.starts_with("blob:")
        || value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("//")
}

/// Turn a document-relative reference (`./assets/app.js?v=1`) into the absolute
/// asset-map key the Tauri resolver expects (`/assets/app.js`).
pub fn normalize_asset_ref(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_prefix = trimmed.strip_prefix("./").unwrap_or(trimmed);
    let without_query = without_prefix
        .split(['?', '#'])
        .next()
        .unwrap_or(without_prefix);
    if without_query.starts_with('/') {
        without_query.to_string()
    } else {
        format!("/{without_query}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<!DOCTYPE html><html><head>
        <script type="module" crossorigin src="./assets/index-abc.js"></script>
        <link rel="stylesheet" crossorigin href="./assets/index-abc.css">
    </head><body><div id="root"></div></body></html>"#;

    #[test]
    fn complete_bundle_reports_nothing_missing() {
        let missing = missing_asset_refs(DOC, |path| {
            matches!(path, "/assets/index-abc.js" | "/assets/index-abc.css")
        });
        assert!(missing.is_empty(), "unexpected missing: {missing:?}");
    }

    #[test]
    fn stale_document_reports_the_assets_it_points_at() {
        // The exact failure that shipped a transparent window: the document
        // names a bundle the asset map does not contain.
        let missing = missing_asset_refs(DOC, |path| path == "/assets/index-abc.css");
        assert_eq!(missing, vec!["/assets/index-abc.js".to_string()]);
    }

    #[test]
    fn empty_bundle_reports_every_reference() {
        let missing = missing_asset_refs(DOC, |_| false);
        assert_eq!(
            missing,
            vec![
                "/assets/index-abc.css".to_string(),
                "/assets/index-abc.js".to_string()
            ]
        );
    }

    #[test]
    fn external_and_inline_references_are_ignored() {
        let html = r##"<a href="https://example.com/x">x</a>
            <img src="data:image/png;base64,AAA">
            <link rel="icon" href="#top">
            <script src="//cdn.example.com/a.js"></script>"##;
        let missing = missing_asset_refs(html, |_| false);
        assert!(missing.is_empty(), "unexpected missing: {missing:?}");
    }

    #[test]
    fn relative_references_are_normalized_to_asset_keys() {
        assert_eq!(normalize_asset_ref("./assets/app.js"), "/assets/app.js");
        assert_eq!(normalize_asset_ref("assets/app.js"), "/assets/app.js");
        assert_eq!(
            normalize_asset_ref("./assets/app.js?v=2#frag"),
            "/assets/app.js"
        );
    }

    #[test]
    fn report_describes_an_empty_bundle() {
        let report = BundleReport {
            index_present: false,
            missing: Vec::new(),
        };
        assert!(!report.is_ok());
        assert!(report.describe().contains("index.html"));
    }

    #[test]
    fn error_page_lists_the_missing_paths_and_the_fix() {
        let report = BundleReport {
            index_present: true,
            missing: vec!["/assets/index-abc.js".to_string()],
        };
        let html = error_page_html(&report);
        assert!(html.contains("/assets/index-abc.js"));
        assert!(html.contains("pnpm tauri build"));
        assert!(html.starts_with("<!DOCTYPE html>"));
        // The summary is interpolated through `format!`, so no placeholder may
        // survive into the page.
        assert!(!html.contains("{summary}"));
    }

    #[test]
    fn error_page_url_encodes_fragment_and_percent_characters() {
        let report = BundleReport {
            index_present: true,
            missing: vec!["/assets/a#b.js".to_string()],
        };
        let url = error_page_url(&report);
        assert!(url.starts_with("data:text/html;charset=utf-8,"));
        // A raw `#` would truncate the document into a fragment.
        assert!(!url.contains('#'), "unencoded fragment marker: {url}");
        assert!(url.contains("%23"), "expected escaped '#': {url}");
        assert!(url.contains("%0A"), "newlines must be escaped: {url}");
    }

    #[test]
    fn percent_encoding_keeps_paths_readable_and_escapes_the_rest() {
        assert_eq!(percent_encode("/assets/app.js"), "/assets/app.js");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("100%"), "100%25");
        assert_eq!(percent_encode("中文"), "%E4%B8%AD%E6%96%87");
    }
}
