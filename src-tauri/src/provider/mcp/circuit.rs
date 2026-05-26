//! 熔断器 — 防止反复重连已宕机的服务

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Closed = 0,
    Open = 1,
    HalfOpen = 2,
}

impl From<u8> for State {
    fn from(v: u8) -> Self {
        match v {
            0 => State::Closed,
            1 => State::Open,
            2 => State::HalfOpen,
            _ => State::Closed,
        }
    }
}

/// 熔断器配置
pub struct CircuitBreakerConfig {
    /// 连续失败多少次后打开熔断器
    pub failure_threshold: u32,
    /// 熔断器打开后的冷却时间
    pub cooldown: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            cooldown: Duration::from_secs(30),
        }
    }
}

/// 熔断器
///
/// 状态机：
/// ```text
/// Closed ──(fail_count >= threshold)──> Open
/// Open   ──(cooldown elapsed)─────────> HalfOpen
/// HalfOpen ──(success)─────────────────> Closed
/// HalfOpen ──(failure)─────────────────> Open
/// ```
pub struct CircuitBreaker {
    state: AtomicU8,
    fail_count: AtomicU32,
    last_failure: Mutex<Option<Instant>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: AtomicU8::new(State::Closed as u8),
            fail_count: AtomicU32::new(0),
            last_failure: Mutex::new(None),
            config,
        }
    }

    /// 是否允许发起请求
    ///
    /// - Closed: 允许
    /// - Open: 检查冷却期是否已过，若已过则切换到 HalfOpen 并允许
    /// - HalfOpen: 允许（只允许一次探测）
    pub fn allow_request(&self) -> bool {
        let state: State = self.state.load(Ordering::Acquire).into();

        match state {
            State::Closed => true,
            State::HalfOpen => true,
            State::Open => {
                // 检查冷却期
                let last = *self.last_failure.lock().unwrap();
                if let Some(instant) = last {
                    if instant.elapsed() >= self.config.cooldown {
                        // 冷却期满，切换到半开
                        self.state.store(State::HalfOpen as u8, Ordering::Release);
                        return true;
                    }
                } else {
                    // 没有记录过失败，允许
                    self.state.store(State::HalfOpen as u8, Ordering::Release);
                    return true;
                }
                false
            }
        }
    }

    /// 记录成功，重置熔断器
    pub fn record_success(&self) {
        self.fail_count.store(0, Ordering::Release);
        self.state.store(State::Closed as u8, Ordering::Release);
    }

    /// 记录失败，累计失败次数
    pub fn record_failure(&self) {
        let count = self.fail_count.fetch_add(1, Ordering::AcqRel) + 1;
        *self.last_failure.lock().unwrap() = Some(Instant::now());

        if count >= self.config.failure_threshold {
            self.state.store(State::Open as u8, Ordering::Release);
        }
    }

    /// 重置熔断器到初始状态
    pub fn reset(&self) {
        self.fail_count.store(0, Ordering::Release);
        self.state.store(State::Closed as u8, Ordering::Release);
        *self.last_failure.lock().unwrap() = None;
    }

    /// 当前失败计数
    pub fn failure_count(&self) -> u32 {
        self.fail_count.load(Ordering::Acquire)
    }

    /// 当前是否打开（不可用）
    pub fn is_open(&self) -> bool {
        self.state.load(Ordering::Acquire) == State::Open as u8
    }
}
