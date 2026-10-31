//! 物理内存状态探测。
//!
//! 设置页的内存占用条需要知道宿主机总内存与当前可用内存；该能力属于平台
//! 适配层（指南 §5.7），领域层只消费归一化后的字节数。
//!
//! 当前只有 macOS 后端（本机开发目标）。Windows / Linux 返回
//! [`rmccc_error::RmcccError::UnsupportedPlatform`]，由前端降级为只显示
//! 「实例分配」数值，不伪造系统占用。

use rmccc_error::Result;

/// 物理内存状态（字节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalMemory {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl PhysicalMemory {
    /// 已用字节数（饱和减法，避免异常探测值下溢）。
    pub fn used_bytes(self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }
}

/// 平台内存探测能力。
pub trait SystemMemory: Send + Sync {
    fn physical_memory(&self) -> Result<PhysicalMemory>;
}

/// 解析 macOS `vm_stat` 输出。
///
/// 输入形如：
/// ```text
/// Mach Virtual Memory Statistics: (page size of 16384 bytes)
/// Pages free:                               12345.
/// Pages inactive:                           23456.
/// ```
///
/// 可用内存 = free + inactive + speculative + purgeable。`total_bytes` 由
/// `sysctl -n hw.memsize` 提供。解析失败返回 `None`，由调用方转成明确错误，
/// 不静默给出 0。
pub fn parse_vm_stat(output: &str, total_bytes: u64) -> Option<PhysicalMemory> {
    if total_bytes == 0 {
        return None;
    }

    let page_size = output
        .lines()
        .find_map(|line| {
            let (_, rest) = line.split_once("page size of ")?;
            rest.split_whitespace().next()?.parse::<u64>().ok()
        })
        .filter(|size| *size > 0)?;

    let mut pages = 0_u64;
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if !matches!(
            key,
            "Pages free" | "Pages inactive" | "Pages speculative" | "Pages purgeable"
        ) {
            continue;
        }
        let count = value.trim().trim_end_matches('.').parse::<u64>().ok()?;
        pages = pages.saturating_add(count);
    }

    if pages == 0 {
        return None;
    }

    let available = pages.saturating_mul(page_size).min(total_bytes);
    Some(PhysicalMemory {
        total_bytes,
        available_bytes: available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vm_stat_counts() {
        let output = "\
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                               100.
Pages active:                             999.
Pages inactive:                           200.
Pages speculative:                         50.
Pages purgeable:                           25.
";
        let mem = parse_vm_stat(output, 16 * 1024 * 1024 * 1024).expect("parsed");
        assert_eq!(mem.available_bytes, 375 * 16384);
        assert_eq!(
            mem.used_bytes(),
            16 * 1024 * 1024 * 1024 - mem.available_bytes
        );
    }

    #[test]
    fn rejects_missing_or_broken_input() {
        assert!(parse_vm_stat("", 1024).is_none());
        assert!(parse_vm_stat(
            "Mach Virtual Memory Statistics: (page size of 0 bytes)",
            1024
        )
        .is_none());
        // 没有可识别的页计数。
        assert!(parse_vm_stat(
            "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages active: 10.",
            1024
        )
        .is_none());
    }

    #[test]
    fn clamps_available_to_total() {
        let output = "page size of 16384 bytes\nPages free: 1000000.";
        let mem = parse_vm_stat(output, 4096).expect("parsed");
        assert_eq!(mem.available_bytes, 4096);
        assert_eq!(mem.used_bytes(), 0);
    }
}
