//! 收键同步路径的进程内延迟统计：热路径只碰原子计数，停用 TSF 时才写一条汇总日志。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static COUNT: AtomicU64 = AtomicU64::new(0);
static TOTAL_US: AtomicU64 = AtomicU64::new(0);
static MAX_US: AtomicU64 = AtomicU64::new(0);
static LT_1_MS: AtomicU64 = AtomicU64::new(0);
static LT_2_MS: AtomicU64 = AtomicU64::new(0);
static LT_4_MS: AtomicU64 = AtomicU64::new(0);
static LT_8_MS: AtomicU64 = AtomicU64::new(0);
static LT_16_MS: AtomicU64 = AtomicU64::new(0);
static LT_32_MS: AtomicU64 = AtomicU64::new(0);
static GE_32_MS: AtomicU64 = AtomicU64::new(0);

/// 记一次输入法接管的 `OnKeyDown` 同步路径；不格式化文本、不碰文件。
pub(crate) fn record(elapsed: Duration) {
    let micros = elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
    COUNT.fetch_add(1, Ordering::Relaxed);
    TOTAL_US.fetch_add(micros, Ordering::Relaxed);
    MAX_US.fetch_max(micros, Ordering::Relaxed);
    bucket(micros).fetch_add(1, Ordering::Relaxed);
}

/// 取走本进程自上次汇总后的统计；没有收键就不产生日志。
pub(crate) fn drain_summary() -> Option<String> {
    let count = COUNT.swap(0, Ordering::Relaxed);
    if count == 0 {
        return None;
    }
    let total = TOTAL_US.swap(0, Ordering::Relaxed);
    let maximum = MAX_US.swap(0, Ordering::Relaxed);
    let lt_1 = LT_1_MS.swap(0, Ordering::Relaxed);
    let lt_2 = LT_2_MS.swap(0, Ordering::Relaxed);
    let lt_4 = LT_4_MS.swap(0, Ordering::Relaxed);
    let lt_8 = LT_8_MS.swap(0, Ordering::Relaxed);
    let lt_16 = LT_16_MS.swap(0, Ordering::Relaxed);
    let lt_32 = LT_32_MS.swap(0, Ordering::Relaxed);
    let ge_32 = GE_32_MS.swap(0, Ordering::Relaxed);
    Some(format!(
        "收键同步耗时汇总 keys={count} avg_us={} max_us={maximum} buckets=[<1ms:{lt_1},1-2ms:{lt_2},2-4ms:{lt_4},4-8ms:{lt_8},8-16ms:{lt_16},16-32ms:{lt_32},>=32ms:{ge_32}]",
        total / count
    ))
}

fn bucket(micros: u64) -> &'static AtomicU64 {
    match micros {
        0..1_000 => &LT_1_MS,
        1_000..2_000 => &LT_2_MS,
        2_000..4_000 => &LT_4_MS,
        4_000..8_000 => &LT_8_MS,
        8_000..16_000 => &LT_16_MS,
        16_000..32_000 => &LT_32_MS,
        _ => &GE_32_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::{drain_summary, record};
    use std::time::Duration;

    #[test]
    fn summarizes_without_recording_key_contents() {
        let _ = drain_summary();
        record(Duration::from_micros(500));
        record(Duration::from_micros(1_500));
        record(Duration::from_micros(40_000));

        let summary = drain_summary().expect("三次收键应有汇总");
        assert!(summary.contains("keys=3"));
        assert!(summary.contains("avg_us=14000"));
        assert!(summary.contains("max_us=40000"));
        assert!(summary.contains("<1ms:1"));
        assert!(summary.contains("1-2ms:1"));
        assert!(summary.contains(">=32ms:1"));
        assert!(drain_summary().is_none());
    }
}
