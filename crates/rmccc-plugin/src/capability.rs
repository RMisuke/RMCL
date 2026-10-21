//! 能力清单与权限声明的静态校验（ADR-004 决策 4、指南 §8.1）。
//!
//! 规则：**未知能力一律拒绝**；风险等级决定"安装时自动授予"还是"必须显式授权"。

use rmccc_error::{PluginErrorCode, Result, RmcccError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "types.ts")]
pub enum CapabilityRisk {
    /// 声明式、只读、无副作用：安装时可自动授予。
    Low,
    /// 有网络/任务副作用：需要显式授予。
    Medium,
    /// 可写文件、启动游戏、读取凭据：需要显式授予 + 执行时确认（后续阶段）。
    High,
}

impl CapabilityRisk {
    pub fn is_low(self) -> bool {
        matches!(self, CapabilityRisk::Low)
    }

    pub fn label(self) -> &'static str {
        match self {
            CapabilityRisk::Low => "low",
            CapabilityRisk::Medium => "medium",
            CapabilityRisk::High => "high",
        }
    }
}

/// 一项能力的静态描述。
#[derive(Debug, Clone, Copy)]
pub struct CapabilitySpec {
    pub name: &'static str,
    pub description: &'static str,
    pub risk: CapabilityRisk,
    pub requires_hosts: bool,
    /// 该能力是否接受 `instances` 作用域（空 = 全部实例）。
    pub instance_scoped: bool,
    /// Phase 2 是否已有执行通道；false 表示"只可声明，不可调用"。
    pub executable: bool,
}

/// 第一版能力清单（与 ADR-004 表格逐条对应）。
pub const CAPABILITIES: &[CapabilitySpec] = &[
    CapabilitySpec {
        name: "dashboard.cards",
        description: "注册自己的 Dashboard 卡片",
        risk: CapabilityRisk::Low,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "ui.pages",
        description: "注册插件页面和菜单项",
        risk: CapabilityRisk::Low,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "themes.apply",
        description: "提供并应用主题",
        risk: CapabilityRisk::Low,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "settings.read",
        description: "读取公开的应用/实例设置子集",
        risk: CapabilityRisk::Low,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "instance.read",
        description: "读取用户授权的实例概要",
        risk: CapabilityRisk::Low,
        requires_hosts: false,
        instance_scoped: true,
        executable: false,
    },
    CapabilitySpec {
        name: "mods.read",
        description: "读取授权实例的模组元数据",
        risk: CapabilityRisk::Low,
        requires_hosts: false,
        instance_scoped: true,
        executable: false,
    },
    CapabilitySpec {
        name: "mods.search",
        description: "调用已注册 Mod Source 查询",
        risk: CapabilityRisk::Medium,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "mods.plan_install",
        description: "生成安装计划（不写文件）",
        risk: CapabilityRisk::Medium,
        requires_hosts: false,
        instance_scoped: true,
        executable: false,
    },
    CapabilitySpec {
        name: "mods.install",
        description: "请求执行安装计划",
        risk: CapabilityRisk::High,
        requires_hosts: false,
        instance_scoped: true,
        executable: false,
    },
    CapabilitySpec {
        name: "mods.manage",
        description: "启用、禁用、移除 Mod",
        risk: CapabilityRisk::High,
        requires_hosts: false,
        instance_scoped: true,
        executable: false,
    },
    CapabilitySpec {
        name: "downloads.submit",
        description: "创建受控下载任务",
        risk: CapabilityRisk::Medium,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "downloads.manage",
        description: "查询/暂停/恢复/取消自己或授权的任务",
        risk: CapabilityRisk::Medium,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "game.launch",
        description: "启动指定实例",
        risk: CapabilityRisk::High,
        requires_hosts: false,
        instance_scoped: true,
        executable: false,
    },
    CapabilitySpec {
        name: "logs.read",
        description: "读取脱敏、限量的日志",
        risk: CapabilityRisk::Medium,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "network.request",
        description: "经宿主 HTTP client 对声明域名请求",
        risk: CapabilityRisk::High,
        requires_hosts: true,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "agent.tools.register",
        description: "注册 Agent 工具",
        risk: CapabilityRisk::Medium,
        requires_hosts: false,
        instance_scoped: false,
        executable: false,
    },
    CapabilitySpec {
        name: "agent.context.read",
        description: "读取授权的游戏环境上下文",
        risk: CapabilityRisk::High,
        requires_hosts: false,
        instance_scoped: true,
        executable: false,
    },
];

const CREDENTIALS_PREFIX: &str = "credentials.use:";

/// 解析后的能力引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub name: String,
    pub risk: CapabilityRisk,
    pub requires_hosts: bool,
    pub instance_scoped: bool,
    /// `credentials.use:<service>` 的服务名。
    pub service: Option<String>,
}

impl Capability {
    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if let Some(service) = raw.strip_prefix(CREDENTIALS_PREFIX) {
            validate_service(service)?;
            return Ok(Self {
                name: format!("{CREDENTIALS_PREFIX}{service}"),
                risk: CapabilityRisk::High,
                requires_hosts: false,
                instance_scoped: false,
                service: Some(service.to_string()),
            });
        }
        let spec = CAPABILITIES.iter().find(|c| c.name == raw).ok_or_else(|| {
            plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("未知权限能力：{raw}（未知权限不能默认为允许）"),
            )
        })?;
        Ok(Self {
            name: spec.name.to_string(),
            risk: spec.risk,
            requires_hosts: spec.requires_hosts,
            instance_scoped: spec.instance_scoped,
            service: None,
        })
    }

    pub fn spec(&self) -> Option<&'static CapabilitySpec> {
        CAPABILITIES.iter().find(|c| c.name == self.name)
    }

    pub fn is_sensitive(&self) -> bool {
        !self.risk.is_low()
    }
}

fn validate_service(service: &str) -> Result<()> {
    let valid = (2..=32).contains(&service.len())
        && service.starts_with(|c: char| c.is_ascii_lowercase())
        && service
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(())
    } else {
        Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("credentials.use:<service> 的服务名非法：{service:?}"),
        ))
    }
}

/// 校验 `permissions[].hosts`：精确域名，或受控的 `*.example.com`。
pub fn validate_hosts(hosts: &[String]) -> Result<()> {
    if hosts.len() > 16 {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            "network.request 的 hosts 不能超过 16 条".to_string(),
        ));
    }
    for host in hosts {
        validate_host(host)?;
    }
    Ok(())
}

fn validate_host(host: &str) -> Result<()> {
    if host.is_empty() || host.len() > 253 {
        return Err(invalid_host(host));
    }
    if host != host.to_ascii_lowercase() {
        return Err(invalid_host(host));
    }
    if host.contains(':') || host.contains('/') || host.contains(' ') {
        return Err(invalid_host(host));
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let _ = ip;
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("hosts 不允许直接使用 IP 字面量：{host}"),
        ));
    }

    let domain = match host.strip_prefix("*.") {
        Some(rest) => rest,
        None => {
            if host.contains('*') {
                return Err(plugin_error(
                    PluginErrorCode::ManifestInvalid,
                    format!("hosts 只允许最左侧一个 `*.` 通配：{host}"),
                ));
            }
            host
        }
    };
    if domain.contains('*') {
        return Err(plugin_error(
            PluginErrorCode::ManifestInvalid,
            format!("hosts 不允许中间/多处通配：{host}"),
        ));
    }

    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return Err(invalid_host(host));
    }
    for label in &labels {
        if label.is_empty() || label.len() > 63 {
            return Err(invalid_host(host));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(invalid_host(host));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(invalid_host(host));
        }
    }
    // TLD 至少 2 个字母（避免 `*.1` 之类的地址）。
    let tld = labels.last().copied().unwrap_or_default();
    if tld.len() < 2 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(invalid_host(host));
    }

    // 默认策略：拒绝本地/私网/云 metadata（指南 §14.2 第 6 条）。
    let lower = domain.to_ascii_lowercase();
    const BLOCKED: &[&str] = &[
        "localhost",
        "localhost.localdomain",
        "metadata.google.internal",
        "metadata",
        "instance-data",
        "169.254.169.254",
        "fd00:ec2::254",
    ];
    for blocked in BLOCKED {
        if lower == *blocked || lower.ends_with(&format!(".{blocked}")) {
            return Err(plugin_error(
                PluginErrorCode::ManifestInvalid,
                format!("hosts 默认禁止本地/元数据地址：{host}"),
            ));
        }
    }

    Ok(())
}

fn invalid_host(host: &str) -> RmcccError {
    plugin_error(
        PluginErrorCode::ManifestInvalid,
        format!("hosts 条目不是合法域名：{host:?}"),
    )
}

pub(crate) fn plugin_error(code: PluginErrorCode, message: String) -> RmcccError {
    RmcccError::Plugin { code, message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_capabilities() {
        let cap = Capability::parse("dashboard.cards").unwrap();
        assert_eq!(cap.risk, CapabilityRisk::Low);
        assert!(!cap.is_sensitive());

        let cred = Capability::parse("credentials.use:modrinth").unwrap();
        assert_eq!(cred.risk, CapabilityRisk::High);
        assert_eq!(cred.service.as_deref(), Some("modrinth"));
    }

    #[test]
    fn rejects_unknown_capability() {
        assert!(Capability::parse("filesystem.write").is_err());
        assert!(Capability::parse("dashboard.card").is_err());
        assert!(Capability::parse("").is_err());
        assert!(Capability::parse("credentials.use:").is_err());
        assert!(Capability::parse("credentials.use:Bad").is_err());
    }

    #[test]
    fn accepts_exact_and_wildcard_hosts() {
        validate_hosts(&["status.example.net".into()]).unwrap();
        validate_hosts(&["*.example.com".into()]).unwrap();
        validate_hosts(&[]).unwrap();
    }

    #[test]
    fn rejects_dangerous_hosts() {
        for bad in [
            "*",
            "*.*.example.com",
            "status.*.example.com",
            "example.com:443",
            "https://example.com",
            "192.168.1.1",
            "[::1]",
            "localhost",
            "dev.localhost",
            "metadata.google.internal",
            "*.local",
            "example",
            "exa mple.com",
            "EXAMPLE.com",
        ] {
            assert!(
                validate_hosts(&[bad.to_string()]).is_err(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn host_list_is_bounded() {
        let hosts: Vec<String> = (0..17).map(|i| format!("h{i}.example.com")).collect();
        assert!(validate_hosts(&hosts).is_err());
    }
}
