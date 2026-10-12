// @created 2026-10-13
// @description 采样 Linux 进程 CPU、驻留内存与容器 cgroup 限制。
// @author yunhungo
use serde::Serialize;
use std::{collections::VecDeque, fs, time::Instant};

#[derive(Clone, Default, Serialize)]
pub struct Performance {
    pub sampled_at: Option<i64>,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub container_memory_bytes: Option<u64>,
    pub memory_limit_bytes: Option<u64>,
    pub cpu_limit_cores: Option<f64>,
    pub history: VecDeque<Sample>,
}
#[derive(Clone, Serialize)]
pub struct Sample {
    pub time: i64,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
}
#[derive(Default)]
pub struct Sampler {
    previous: Option<(Instant, u64)>,
    pub current: Performance,
}
fn read(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}
fn cgroup(controller: &str, file: &str) -> Option<String> {
    // cgroup namespace 将当前容器放在根；旧版 QNAP 则可能暴露宿主层级。
    let entries = read("/proc/self/cgroup")?;
    let path = entries.lines().find_map(|line| {
        let parts: Vec<_> = line.splitn(3, ':').collect();
        (parts.len() == 3
            && (parts[1].is_empty() && controller.is_empty()
                || parts[1].split(',').any(|v| v == controller)))
        .then(|| parts[2].trim_start_matches('/').to_string())
    })?;
    let roots: Vec<String> = if controller.is_empty() {
        vec!["/sys/fs/cgroup".into()]
    } else if controller == "cpu" {
        vec![
            "/sys/fs/cgroup/cpu".into(),
            "/sys/fs/cgroup/cpu,cpuacct".into(),
        ]
    } else {
        vec![format!("/sys/fs/cgroup/{controller}")]
    };
    for root in roots {
        for candidate in [format!("{root}/{path}/{file}"), format!("{root}/{file}")] {
            if let Some(value) = read(&candidate) {
                return Some(value);
            }
        }
    }
    None
}
fn cg_number(controller: &str, file: &str) -> Option<u64> {
    cgroup(controller, file)?.trim().parse().ok()
}
pub(super) fn cpu_ticks(stat: &str) -> Option<u64> {
    // comm 可包含空格和括号，字段 14/15 位于最后一个右括号之后。
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
}
impl Sampler {
    pub fn sample(&mut self) {
        let now = Instant::now();
        let time = chrono::Utc::now().timestamp();
        let ticks = read("/proc/self/stat").and_then(|v| cpu_ticks(&v));
        // sysconf 返回每秒时钟滴答数；不硬编码 Linux 的 USER_HZ。
        let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        self.current.cpu_percent = match (self.previous, ticks) {
            (Some((last, before)), Some(after)) if ticks_per_second > 0 && now > last => Some(
                after.saturating_sub(before) as f64
                    / ticks_per_second as f64
                    / now.duration_since(last).as_secs_f64()
                    * 100.0,
            ),
            _ => None,
        };
        self.previous = ticks.map(|ticks| (now, ticks));
        self.current.memory_bytes = read("/proc/self/status").and_then(|v| {
            v.lines()
                .find_map(|line| {
                    line.strip_prefix("VmRSS:")?
                        .split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()
                })
                .map(|kb| kb * 1024)
        });
        self.current.container_memory_bytes = cg_number("", "memory.current")
            .or_else(|| cg_number("memory", "memory.usage_in_bytes"));
        self.current.memory_limit_bytes = cg_number("", "memory.max")
            .or_else(|| cg_number("memory", "memory.limit_in_bytes"))
            .filter(|v| *v > 0 && *v < (1 << 60));
        self.current.cpu_limit_cores = cgroup("", "cpu.max")
            .and_then(|v| {
                let mut fields = v.split_whitespace();
                let quota = fields.next()?.parse::<f64>().ok()?;
                let period = fields.next()?.parse::<f64>().ok()?;
                (quota > 0.0 && period > 0.0).then_some(quota / period)
            })
            .or_else(|| {
                let quota = cg_number("cpu", "cpu.cfs_quota_us")?;
                let period = cg_number("cpu", "cpu.cfs_period_us")?;
                (quota > 0 && period > 0).then_some(quota as f64 / period as f64)
            });
        self.current.sampled_at = Some(time);
        self.current.history.push_back(Sample {
            time,
            cpu_percent: self.current.cpu_percent,
            memory_bytes: self.current.memory_bytes,
        });
        if self.current.history.len() > 120 {
            self.current.history.pop_front();
        }
    }
}
