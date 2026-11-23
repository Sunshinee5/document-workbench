//! 最小日志设施（Infrastructure，不是 Feature）。
//!
//! 冻结约束：只做四档分级（INFO / WARN / ERROR / TRACE）、按天滚动、保留 7 天；
//! 不引入第三方 crate（`00` §10 技术栈冻结），不做 ELK、不做异步队列、不做配置系统。
//! 全仓其他模块只调这里的函数，禁止直接用 `OpenOptions` 另写一套日志。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
#[cfg(not(windows))]
use std::time::{SystemTime, UNIX_EPOCH};

/// 日志级别：数值越大越严重。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// 追踪：调试细节，默认写入但可被运维清理。
    Trace,
    /// 常规信息。
    Info,
    /// 警告：不影响本次操作但需关注。
    Warn,
    /// 错误：本次操作失败。
    Error,
}

impl Level {
    /// 日志行中的英文标记：固定四值，便于检索。
    pub fn label(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// 日志根目录：进程内只初始化一次，重复调用忽略后续值。
static LOG_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// 默认保留天数。
pub const RETENTION_DAYS: i64 = 7;

/// 初始化日志设施：`root` 是绿色版根目录，日志文件写到 `root/logs/`。
pub fn init(root: impl Into<PathBuf>) {
    let root = root.into();
    let _ = LOG_ROOT.set(root);
    if let Some(directory) = log_directory() {
        let _ = fs::create_dir_all(directory);
        retain(RETENTION_DAYS);
    }
}

/// 日志目录；未初始化时返回 `None`，调用方静默跳过。
fn log_directory() -> Option<PathBuf> {
    LOG_ROOT.get().map(|root| root.join("logs"))
}

/// 写一条日志。写失败静默忽略——日志设施不能成为业务失败的原因。
pub fn log(level: Level, target: &str, message: &str) {
    let Some(directory) = log_directory() else {
        return;
    };
    let _ = fs::create_dir_all(&directory);
    let (date, time) = now_parts();
    let path = directory.join(format!("{}.log", date));
    let line = format!("[{}] [{}] [{}] {}\n", time, level.label(), target, message);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// INFO 级别。
pub fn info(target: &str, message: &str) {
    log(Level::Info, target, message);
}

/// WARN 级别。
pub fn warn(target: &str, message: &str) {
    log(Level::Warn, target, message);
}

/// ERROR 级别。
pub fn error(target: &str, message: &str) {
    log(Level::Error, target, message);
}

/// TRACE 级别。
pub fn trace(target: &str, message: &str) {
    log(Level::Trace, target, message);
}

/// 删除超过 `days` 天的日志文件；按文件名 `YYYY-MM-DD.log` 字典序比较即可。
pub fn retain(days: i64) {
    let Some(directory) = log_directory() else {
        return;
    };
    let Ok(entries) = fs::read_dir(&directory) else {
        return;
    };
    let (today, _) = now_parts();
    let cutoff = date_to_days(&today) - days;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".log") || name.len() != 14 {
            continue;
        }
        let date = &name[..10];
        if date_to_days(date) < cutoff {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// 当前**本地**日期与 HH:MM:SS；日志是给人看的，不能显示 UTC 差 8 小时的时间。
///
/// Windows 直接调 `kernel32!GetLocalTime`（系统 API，不是第三方 crate）；
/// 非 Windows 回退到 UTC，仅用于本地开发机上的单元测试。
fn now_parts() -> (String, String) {
    #[cfg(windows)]
    {
        windows_local_parts()
    }
    #[cfg(not(windows))]
    {
        utc_parts()
    }
}

/// Windows 本地时间：系统 API 调用点集中在这里。
#[cfg(windows)]
fn windows_local_parts() -> (String, String) {
    unsafe {
        let mut time = Systemtime::default();
        GetLocalTime(&mut time);
        (
            format!("{:04}-{:02}-{:02}", time.w_year, time.w_month, time.w_day),
            format!(
                "{:02}:{:02}:{:02}",
                time.w_hour, time.w_minute, time.w_second
            ),
        )
    }
}

/// 非 Windows 回退：UTC，仅供单元测试。
#[cfg(not(windows))]
fn utc_parts() -> (String, String) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let remainder = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;
    (
        format!("{:04}-{:02}-{:02}", year, month, day),
        format!("{:02}:{:02}:{:02}", hour, minute, second),
    )
}

/// Windows 本地时间结构：`GetLocalTime` 的输出，字段顺序不可改。
#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct Systemtime {
    /// 年。
    w_year: u16,
    /// 月。
    w_month: u16,
    /// 星期几。
    w_day_of_week: u16,
    /// 日。
    w_day: u16,
    /// 时。
    w_hour: u16,
    /// 分。
    w_minute: u16,
    /// 秒。
    w_second: u16,
    /// 毫秒。
    w_milliseconds: u16,
}

#[cfg(windows)]
extern "system" {
    /// 取系统本地时间；无第三方依赖。
    fn GetLocalTime(time: *mut Systemtime);
}

/// `YYYY-MM-DD` → 距 1970-01-01 的天数；非法字符串返回 0。
fn date_to_days(text: &str) -> i64 {
    let parts: Vec<i64> = text
        .split('-')
        .filter_map(|part| part.parse().ok())
        .collect();
    if parts.len() != 3 {
        return 0;
    }
    days_from_civil(parts[0], parts[1] as u32, parts[2] as u32)
}

/// 公历日期 → 天数（Howard Hinnant 算法，无第三方依赖）。
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i64::from(month);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 天数 → 公历日期（Howard Hinnant 算法，非 Windows 回退路径使用）。
#[cfg(not(windows))]
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_labels_are_frozen() {
        assert_eq!(Level::Trace.label(), "TRACE");
        assert_eq!(Level::Info.label(), "INFO");
        assert_eq!(Level::Warn.label(), "WARN");
        assert_eq!(Level::Error.label(), "ERROR");
    }

    #[test]
    fn date_to_days_is_monotonic() {
        assert_eq!(date_to_days("1970-01-01"), 0);
        assert!(date_to_days("2026-11-23") > date_to_days("2000-02-29"));
        assert_eq!(date_to_days("not-a-date"), 0);
    }
}
