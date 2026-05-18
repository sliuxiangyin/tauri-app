use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

use super::error::SchedulerError;

/// 调度策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingPolicy {
    /// 固定间隔：从上一次完成到下一次开始之间等待固定时间
    FixedDelay,
    /// 固定速率：严格按照固定周期开始执行，允许任务重叠
    FixedRate,
}

/// 任务句柄
#[allow(dead_code)]
struct TaskHandle {
    cancel: Arc<AtomicBool>,
    join: JoinHandle<()>,
    active_count: Arc<AtomicUsize>, // 当前正在运行的任务实例数
}

/// 轻量级调度器
#[allow(dead_code)]
pub struct Scheduler {
    tasks: Mutex<HashMap<String, TaskHandle>>,
    stopped: Arc<AtomicBool>,
}

/// 检查是否应该停止（cancel 或全局 stopped）
#[allow(dead_code)]
fn should_stop(cancel: &AtomicBool, stopped: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed) || stopped.load(Ordering::Relaxed)
}

#[allow(dead_code)]
impl Scheduler {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 添加周期性任务（默认：固定延迟模式）
    pub fn add_periodic<F, Fut>(
        &self,
        name: &str,
        interval: Duration,
        f: F,
    ) -> Result<(), SchedulerError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.add_periodic_with_policy(name, interval, SchedulingPolicy::FixedDelay, f)
    }

    /// 添加周期性任务（可指定调度策略）
    pub fn add_periodic_with_policy<F, Fut>(
        &self,
        name: &str,
        interval: Duration,
        policy: SchedulingPolicy,
        f: F,
    ) -> Result<(), SchedulerError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut tasks = self.tasks.lock().unwrap();
        if tasks.contains_key(name) {
            return Err(SchedulerError::TaskAlreadyExists(name.to_string()));
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        let stopped = self.stopped.clone();
        let active_count = Arc::new(AtomicUsize::new(0));
        let active_count_clone = active_count.clone();

        let join = tokio::spawn(async move {
            // 将 f 包在 Arc 中，供 FixedRate 模式下多个子任务共享
            let f = Arc::new(f);
            match policy {
                SchedulingPolicy::FixedDelay => {
                    // 固定延迟模式：每次执行完成后等待固定时间
                    loop {
                        if should_stop(&cancel_clone, &stopped) {
                            break;
                        }

                        let start = tokio::time::Instant::now();

                        // 增加活跃计数
                        active_count_clone.fetch_add(1, Ordering::Relaxed);
                        f().await;
                        active_count_clone.fetch_sub(1, Ordering::Relaxed);

                        if should_stop(&cancel_clone, &stopped) {
                            break;
                        }

                        let elapsed = start.elapsed();
                        if elapsed < interval {
                            tokio::time::sleep(interval - elapsed).await;
                        }
                    }
                }
                SchedulingPolicy::FixedRate => {
                    // 固定速率模式：严格按照固定周期开始执行，不等待任务完成
                    let start = tokio::time::Instant::now();
                    let mut ticker = tokio::time::interval_at(start + interval, interval);

                    loop {
                        ticker.tick().await;

                        // tick 触发后立即检查是否需要停止，避免在取消后仍然派发子任务
                        if should_stop(&cancel_clone, &stopped) {
                            break;
                        }

                        // 增加活跃计数
                        active_count_clone.fetch_add(1, Ordering::Relaxed);
                        let f = f.clone();
                        let active_count = active_count_clone.clone();

                        // 不等待任务完成，让它在后台运行
                        tokio::spawn(async move {
                            f().await;
                            active_count.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                }
            }
        });

        tasks.insert(
            name.to_string(),
            TaskHandle {
                cancel,
                join,
                active_count,
            },
        );
        Ok(())
    }

    /// 添加固定速率任务（便捷方法）
    pub fn add_fixed_rate<F, Fut>(
        &self,
        name: &str,
        interval: Duration,
        f: F,
    ) -> Result<(), SchedulerError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.add_periodic_with_policy(name, interval, SchedulingPolicy::FixedRate, f)
    }

    /// 添加固定延迟任务（便捷方法）
    pub fn add_fixed_delay<F, Fut>(
        &self,
        name: &str,
        interval: Duration,
        f: F,
    ) -> Result<(), SchedulerError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.add_periodic_with_policy(name, interval, SchedulingPolicy::FixedDelay, f)
    }

    /// 添加固定速率任务，并限制最大并发数
    pub fn add_fixed_rate_with_limit<F, Fut>(
        &self,
        name: &str,
        interval: Duration,
        max_concurrent: usize,
        f: F,
    ) -> Result<(), SchedulerError>
    where
        F: Fn() -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));

        self.add_fixed_rate(name, interval, move || {
            let f = f.clone();
            let semaphore = semaphore.clone();
            async move {
                let _permit = semaphore.acquire().await.unwrap();
                f().await;
            }
        })
    }

    /// 移除单个任务
    pub fn remove(&self, name: &str) -> Result<(), SchedulerError> {
        let mut tasks = self.tasks.lock().unwrap();
        let handle = tasks
            .remove(name)
            .ok_or_else(|| SchedulerError::TaskNotFound(name.to_string()))?;
        handle.cancel.store(true, Ordering::Relaxed);
        drop(handle.join); // 分离，让任务自然结束
        Ok(())
    }

    /// 停止所有任务（不等待）
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }

    /// 优雅关闭，等待所有任务完成
    pub async fn shutdown(&self) {
        self.stopped.store(true, Ordering::Relaxed);

        // 1. 取消所有任务，排出 join handles 和 active_count 监视器
        let (joins, active_counts): (Vec<JoinHandle<()>>, Vec<Arc<AtomicUsize>>) = {
            let mut tasks = self.tasks.lock().unwrap();
            tasks
                .drain()
                .map(|(_, h)| {
                    h.cancel.store(true, Ordering::Relaxed);
                    (h.join, h.active_count)
                })
                .unzip()
        };

        // 2. 等待所有主循环退出（它们会在下次检查 cancel/stopped 时退出）
        for join in joins {
            let _ = join.await;
        }

        // 3. 等待所有已派发的子任务完成
        for ac in &active_counts {
            while ac.load(Ordering::Relaxed) > 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }

    /// 获取任务当前活跃实例数（仅对 FixedRate 模式有效）
    pub fn get_active_count(&self, name: &str) -> Option<usize> {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .get(name)
            .map(|h| h.active_count.load(Ordering::Relaxed))
    }

    /// 是否已停止
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    /// 任务数量
    pub fn task_count(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
