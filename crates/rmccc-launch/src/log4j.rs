//! Log4j configuration extraction (HMCL `extractLog4jConfigurationFile`).
//!
//! Minecraft 1.7+ relies on a `log4j2.xml` placed next to the version. HMCL
//! ships four variants (1.7 / 1.12 × normal / debug) and writes the matching
//! one into `{instance_root}/log4j2.xml` before launch. The launcher then
//! injects `-Dlog4j.configurationFile=<path>` so the game uses it.

use rmccc_error::Result;
use std::path::Path;

/// `log4j2.xml` written into the instance root (HMCL `getLog4jConfigurationFile`).
pub fn log4j_config_file(instance_root: &Path) -> std::path::PathBuf {
    instance_root.join("log4j2.xml")
}

/// Whether the Minecraft version uses log4j (1.7+).
pub fn is_using_log4j(mc_version: &str) -> bool {
    version_minor(mc_version).map(|m| m >= 7).unwrap_or(false)
}

/// Extract the matching log4j2 configuration into `{instance_root}/log4j2.xml`.
///
/// `mc_version` is the actual Minecraft version (e.g. `1.20.4`), used to pick
/// the 1.7 or 1.12 configuration. Returns the written file path.
pub fn extract_log4j_configuration_file(
    instance_root: &Path,
    mc_version: &str,
    debug: bool,
) -> Result<std::path::PathBuf> {
    let target = log4j_config_file(instance_root);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = if is_1_12_or_newer(mc_version) {
        if debug {
            LOG4J2_1_12_DEBUG
        } else {
            LOG4J2_1_12
        }
    } else if debug {
        LOG4J2_1_7_DEBUG
    } else {
        LOG4J2_1_7
    };

    std::fs::write(&target, content)?;
    let is_debug = debug;
    tracing::debug!(
        "Log4j configuration extracted: {} (version {}, debug {})",
        target.display(),
        mc_version,
        is_debug
    );
    Ok(target)
}

/// Parse the minor version of a Minecraft version string ("1.20.4" → 20).
fn version_minor(v: &str) -> Option<u32> {
    let mut parts = v.split('.');
    parts.next()?; // major (assumed 1 for modern MC)
    let minor = parts.next()?.parse::<u32>().ok()?;
    Some(minor)
}

fn is_1_12_or_newer(v: &str) -> bool {
    version_minor(v).map(|m| m >= 12).unwrap_or(false)
}

/// Whether the Minecraft version is older than `1.{minor}` (e.g. `19` → `< 1.19`).
pub fn mc_version_before(v: &str, minor: u32) -> bool {
    version_minor(v).map(|m| m < minor).unwrap_or(false)
}

// ── Embedded Mojang log4j2 configurations ──

/// 1.12+ standard configuration (mirrors Mojang's log4j2.xml).
const LOG4J2_1_12: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Configuration status="WARN" packages="com.mojang.util">
    <Appenders>
        <Console name="SysOut" target="SYSTEM_OUT">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
        </Console>
        <Queue name="ServerGuiConsole">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
        </Queue>
        <RollingRandomAccessFile name="File" fileName="logs/latest.log" filePattern="logs/%d{yyyy-MM-dd}-%i.log.gz">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
            <Policies>
                <TimeBasedTriggeringPolicy />
                <OnStartupTriggeringPolicy />
            </Policies>
        </RollingRandomAccessFile>
    </Appenders>
    <Loggers>
        <Root level="info">
            <filters>
                <MarkerFilter marker="NETWORK_PACKETS" onMatch="DENY" onMismatch="NEUTRAL" />
            </filters>
            <AppenderRef ref="SysOut"/>
            <AppenderRef ref="File"/>
            <AppenderRef ref="ServerGuiConsole"/>
        </Root>
        <Logger name="com.mojang.authlib" level="debug" />
        <Logger name="com.mojang.util.QueueLogAppender" level="info" />
        <Logger name="com.mojang.launcher" level="debug" />
    </Loggers>
</Configuration>
"#;

/// 1.12+ debug configuration (verbose logging enabled).
const LOG4J2_1_12_DEBUG: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Configuration status="WARN" packages="com.mojang.util">
    <Appenders>
        <Console name="SysOut" target="SYSTEM_OUT">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
        </Console>
        <Queue name="ServerGuiConsole">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
        </Queue>
        <RollingRandomAccessFile name="File" fileName="logs/latest.log" filePattern="logs/%d{yyyy-MM-dd}-%i.log.gz">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
            <Policies>
                <TimeBasedTriggeringPolicy />
                <OnStartupTriggeringPolicy />
            </Policies>
        </RollingRandomAccessFile>
    </Appenders>
    <Loggers>
        <Root level="debug">
            <filters>
                <MarkerFilter marker="NETWORK_PACKETS" onMatch="DENY" onMismatch="NEUTRAL" />
            </filters>
            <AppenderRef ref="SysOut"/>
            <AppenderRef ref="File"/>
            <AppenderRef ref="ServerGuiConsole"/>
        </Root>
        <Logger name="com.mojang.authlib" level="debug" />
        <Logger name="com.mojang.util.QueueLogAppender" level="info" />
        <Logger name="com.mojang.launcher" level="debug" />
    </Loggers>
</Configuration>
"#;

/// Pre-1.12 standard configuration.
const LOG4J2_1_7: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Configuration status="WARN" packages="com.mojang.util">
    <Appenders>
        <Console name="SysOut" target="SYSTEM_OUT">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
        </Console>
        <RollingRandomAccessFile name="File" fileName="logs/latest.log" filePattern="logs/%d{yyyy-MM-dd}-%i.log.gz">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
            <Policies>
                <TimeBasedTriggeringPolicy />
                <OnStartupTriggeringPolicy />
            </Policies>
        </RollingRandomAccessFile>
    </Appenders>
    <Loggers>
        <Root level="info">
            <AppenderRef ref="SysOut"/>
            <AppenderRef ref="File"/>
        </Root>
        <Logger name="com.mojang.authlib" level="debug" />
        <Logger name="com.mojang.launcher" level="debug" />
    </Loggers>
</Configuration>
"#;

/// Pre-1.12 debug configuration.
const LOG4J2_1_7_DEBUG: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Configuration status="WARN" packages="com.mojang.util">
    <Appenders>
        <Console name="SysOut" target="SYSTEM_OUT">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
        </Console>
        <RollingRandomAccessFile name="File" fileName="logs/latest.log" filePattern="logs/%d{yyyy-MM-dd}-%i.log.gz">
            <PatternLayout pattern="[%d{HH:mm:ss}] [%t/%level]: %msg%n" />
            <Policies>
                <TimeBasedTriggeringPolicy />
                <OnStartupTriggeringPolicy />
            </Policies>
        </RollingRandomAccessFile>
    </Appenders>
    <Loggers>
        <Root level="debug">
            <AppenderRef ref="SysOut"/>
            <AppenderRef ref="File"/>
        </Root>
        <Logger name="com.mojang.authlib" level="debug" />
        <Logger name="com.mojang.launcher" level="debug" />
    </Loggers>
</Configuration>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_minor() {
        assert_eq!(version_minor("1.20.4"), Some(20));
        assert_eq!(version_minor("1.7.10"), Some(7));
        assert_eq!(version_minor("1.12.2"), Some(12));
        assert_eq!(version_minor("1.5.2"), Some(5));
        assert_eq!(version_minor("abc"), None);
    }

    #[test]
    fn test_is_using_log4j() {
        assert!(is_using_log4j("1.7"));
        assert!(is_using_log4j("1.20.4"));
        assert!(!is_using_log4j("1.6.4"));
        assert!(!is_using_log4j(""));
    }

    #[test]
    fn test_extract_writes_file() {
        let root = std::env::temp_dir().join(format!("rmcl-log4j-{}", uuid::Uuid::new_v4()));
        let path = extract_log4j_configuration_file(&root, "1.20.4", false).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Configuration"));
        // 1.12 config has the Queue appender.
        assert!(content.contains("ServerGuiConsole"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_extract_1_7() {
        let root = std::env::temp_dir().join(format!("rmcl-log4j-{}", uuid::Uuid::new_v4()));
        let path = extract_log4j_configuration_file(&root, "1.7.10", true).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // 1.7 config has no Queue appender.
        assert!(!content.contains("ServerGuiConsole"));
        assert!(content.contains("debug"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
