use crate::provider::scheduler::scheduler::SchedulingPolicy;
use crate::provider::scheduler::{Scheduler, SchedulerError};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_add_and_remove_task() {
    let scheduler = Scheduler::new();
    let counter = Arc::new(AtomicUsize::new(0));

    // 添加任务
    scheduler
        .add_fixed_rate("test_task", Duration::from_millis(100), {
            let counter = counter.clone();
            move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
        .unwrap();

    // 等待几次执行
    tokio::time::sleep(Duration::from_millis(350)).await;

    // 验证任务已执行
    let count = counter.load(Ordering::Relaxed);
    assert!(count >= 2 && count <= 4);

    // 移除任务
    scheduler.remove("test_task").unwrap();

    // 重置计数器
    counter.store(0, Ordering::Relaxed);

    // 等待一段时间
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 验证任务不再执行
    assert_eq!(counter.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_add_duplicate_task() {
    let scheduler = Scheduler::new();

    scheduler
        .add_fixed_rate("task", Duration::from_millis(100), || async {})
        .unwrap();

    // 添加同名任务应该失败
    let result = scheduler.add_fixed_rate("task", Duration::from_millis(100), || async {});
    assert!(matches!(result, Err(SchedulerError::TaskAlreadyExists(_))));
}

#[tokio::test]
async fn test_remove_nonexistent_task() {
    let scheduler = Scheduler::new();
    let result = scheduler.remove("nonexistent");
    assert!(matches!(result, Err(SchedulerError::TaskNotFound(_))));
}

#[tokio::test]
async fn test_fixed_delay_mode() {
    let scheduler = Scheduler::new();
    let counter = Arc::new(AtomicUsize::new(0));
    let start_time = tokio::time::Instant::now();

    scheduler
        .add_fixed_delay("delay_task", Duration::from_millis(100), {
            let counter = counter.clone();
            move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(350)).await;

    let elapsed = start_time.elapsed();
    let count = counter.load(Ordering::Relaxed);

    // 固定延迟模式：每次执行需要 50ms + 100ms = 150ms
    // 350ms 至少执行 2 次，最多 4 次（取决于调度精度）
    assert!(count >= 2 && count <= 4, "count was {}", count);
    assert!(elapsed > Duration::from_millis(200));
}

#[tokio::test]
async fn test_fixed_rate_mode() {
    let scheduler = Scheduler::new();
    let execution_times = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    scheduler
        .add_fixed_rate("rate_task", Duration::from_millis(100), {
            let execution_times = execution_times.clone();
            move || {
                let execution_times = execution_times.clone();
                async move {
                    let now = tokio::time::Instant::now();
                    execution_times.lock().await.push(now);
                    // 模拟耗时超过周期
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            }
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(350)).await;
    scheduler.stop();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let times = execution_times.lock().await;

    // 固定速率模式：严格按照 100ms 周期开始
    // 350ms 内应该触发约 4 次（0ms, 100ms, 200ms, 300ms）
    // 虽然任务耗时 150ms，但会在后台并发执行
    assert!(times.len() >= 3 && times.len() <= 5);

    // 验证执行间隔
    for i in 1..times.len() {
        let interval = times[i] - times[i - 1];
        // 间隔应该接近 100ms
        assert!(interval > Duration::from_millis(80));
        assert!(interval < Duration::from_millis(120));
    }
}

#[tokio::test]
async fn test_fixed_rate_with_limit() {
    let scheduler = Scheduler::new();
    let concurrent_count = Arc::new(AtomicUsize::new(0));
    let max_concurrent = Arc::new(AtomicUsize::new(0));

    scheduler
        .add_fixed_rate_with_limit(
            "limited_task",
            Duration::from_millis(50), // 50ms 周期
            3,                         // 最大 3 个并发
            {
                let concurrent_count = concurrent_count.clone();
                let max_concurrent = max_concurrent.clone();
                move || {
                    let concurrent_count = concurrent_count.clone();
                    let max_concurrent = max_concurrent.clone();
                    async move {
                        let current = concurrent_count.fetch_add(1, Ordering::Relaxed) + 1;

                        // 记录最大并发数
                        let mut max = max_concurrent.load(Ordering::Relaxed);
                        while current > max {
                            match max_concurrent.compare_exchange(
                                max,
                                current,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => break,
                                Err(x) => max = x,
                            }
                        }

                        // 模拟耗时 200ms 的任务
                        tokio::time::sleep(Duration::from_millis(200)).await;

                        concurrent_count.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            },
        )
        .unwrap();

    // 运行一段时间让任务积累
    tokio::time::sleep(Duration::from_millis(500)).await;
    scheduler.stop();
    tokio::time::sleep(Duration::from_millis(300)).await; // 等待现有任务完成

    // 验证并发数未超过限制
    let max = max_concurrent.load(Ordering::Relaxed);
    assert!(max <= 3, "Max concurrent was {}, expected <= 3", max);
}

#[tokio::test]
async fn test_shutdown_waits_for_tasks() {
    let scheduler = Scheduler::new();
    let task_completed = Arc::new(AtomicBool::new(false));
    let start_time = tokio::time::Instant::now();

    scheduler
        .add_fixed_delay("long_task", Duration::from_millis(100), {
            let task_completed = task_completed.clone();
            move || {
                let task_completed = task_completed.clone();
                async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    task_completed.store(true, Ordering::Relaxed);
                }
            }
        })
        .unwrap();

    // 等待确保至少有一个任务实例正在运行
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 立即关闭
    scheduler.shutdown().await;

    let elapsed = start_time.elapsed();
    assert!(elapsed >= Duration::from_millis(500));
    assert!(task_completed.load(Ordering::Relaxed));
}

#[tokio::test]
async fn test_stop_does_not_wait() {
    let scheduler = Scheduler::new();
    let task_started = Arc::new(AtomicBool::new(false));
    let task_completed = Arc::new(AtomicBool::new(false));

    scheduler
        .add_fixed_rate("task", Duration::from_millis(10), {
            let task_started = task_started.clone();
            let task_completed = task_completed.clone();
            move || {
                let task_started = task_started.clone();
                let task_completed = task_completed.clone();
                async move {
                    task_started.store(true, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    task_completed.store(true, Ordering::Relaxed);
                }
            }
        })
        .unwrap();

    // 等待任务启动
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 立即停止（不等待）
    scheduler.stop();

    // 等待一小段时间
    tokio::time::sleep(Duration::from_millis(100)).await;

    // stop() 后任务可能还在运行，但应该不会阻止程序退出
    // 这里只验证 stop() 不会等待
    assert!(task_started.load(Ordering::Relaxed));
    // 任务可能完成也可能未完成
}

#[tokio::test]
async fn test_multiple_schedulers_independent() {
    let scheduler1 = Scheduler::new();
    let scheduler2 = Scheduler::new();

    let counter1 = Arc::new(AtomicUsize::new(0));
    let counter2 = Arc::new(AtomicUsize::new(0));

    scheduler1
        .add_fixed_rate("task1", Duration::from_millis(50), {
            let counter1 = counter1.clone();
            move || {
                let counter1 = counter1.clone();
                async move {
                    counter1.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
        .unwrap();

    scheduler2
        .add_fixed_rate("task2", Duration::from_millis(50), {
            let counter2 = counter2.clone();
            move || {
                let counter2 = counter2.clone();
                async move {
                    counter2.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(counter1.load(Ordering::Relaxed) > 0);
    assert!(counter2.load(Ordering::Relaxed) > 0);

    // 停止第一个调度器
    scheduler1.stop();
    let count1_before = counter1.load(Ordering::Relaxed);

    tokio::time::sleep(Duration::from_millis(150)).await;

    // 第一个调度器停止工作
    assert_eq!(counter1.load(Ordering::Relaxed), count1_before);
    // 第二个调度器继续工作
    assert!(counter2.load(Ordering::Relaxed) > count1_before);
}

#[tokio::test]
async fn test_task_count() {
    let scheduler = Scheduler::new();

    assert_eq!(scheduler.task_count(), 0);

    scheduler
        .add_fixed_rate("task1", Duration::from_millis(100), || async {})
        .unwrap();
    assert_eq!(scheduler.task_count(), 1);

    scheduler
        .add_fixed_rate("task2", Duration::from_millis(100), || async {})
        .unwrap();
    assert_eq!(scheduler.task_count(), 2);

    scheduler.remove("task1").unwrap();
    assert_eq!(scheduler.task_count(), 1);

    scheduler.remove("task2").unwrap();
    assert_eq!(scheduler.task_count(), 0);
}

#[tokio::test]
async fn test_is_stopped() {
    let scheduler = Scheduler::new();

    assert!(!scheduler.is_stopped());

    scheduler.stop();
    assert!(scheduler.is_stopped());
}

#[tokio::test]
async fn test_get_active_count() {
    let scheduler = Scheduler::new();

    scheduler
        .add_fixed_rate("task", Duration::from_millis(50), || async {
            tokio::time::sleep(Duration::from_millis(200)).await;
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let active = scheduler.get_active_count("task");
    assert!(active.is_some());
    assert!(active.unwrap() > 0);

    let none_active = scheduler.get_active_count("nonexistent");
    assert!(none_active.is_none());
}

#[tokio::test]
async fn test_multiple_tasks_concurrent_execution() {
    let scheduler = Scheduler::new();
    let counter1 = Arc::new(AtomicUsize::new(0));
    let counter2 = Arc::new(AtomicUsize::new(0));

    scheduler
        .add_fixed_rate("task1", Duration::from_millis(50), {
            let counter1 = counter1.clone();
            move || {
                let counter1 = counter1.clone();
                async move {
                    counter1.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        })
        .unwrap();

    scheduler
        .add_fixed_rate("task2", Duration::from_millis(50), {
            let counter2 = counter2.clone();
            move || {
                let counter2 = counter2.clone();
                async move {
                    counter2.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(counter1.load(Ordering::Relaxed) > 0);
    assert!(counter2.load(Ordering::Relaxed) > 0);
}

#[tokio::test]
async fn test_task_error_handling() {
    let scheduler = Scheduler::new();
    let counter = Arc::new(AtomicUsize::new(0));

    scheduler
        .add_fixed_rate("task", Duration::from_millis(50), {
            let counter = counter.clone();
            move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    // 模拟恐慌，但不会传播到调度器
                    if counter.load(Ordering::Relaxed) == 2 {
                        panic!("Task panic!");
                    }
                }
            }
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // 任务应该继续执行，不会因为恐慌而停止
    assert!(counter.load(Ordering::Relaxed) >= 3);
}

#[tokio::test]
async fn test_shutdown_with_multiple_tasks() {
    let scheduler = Scheduler::new();
    let completed_count = Arc::new(AtomicUsize::new(0));

    // 添加多个需要长时间运行的任务
    for i in 0..5 {
        scheduler
            .add_fixed_rate(&format!("task_{}", i), Duration::from_millis(50), {
                let completed_count = completed_count.clone();
                move || {
                    let completed_count = completed_count.clone();
                    async move {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        completed_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .unwrap();
    }

    // 等待所有任务启动
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 优雅关闭
    let shutdown_start = tokio::time::Instant::now();
    scheduler.shutdown().await;
    let shutdown_duration = shutdown_start.elapsed();

    // 应该等待所有任务完成
    assert!(shutdown_duration >= Duration::from_millis(200));
    assert!(completed_count.load(Ordering::Relaxed) >= 5);
}

#[tokio::test]
async fn test_periodic_with_policy() {
    let scheduler = Scheduler::new();
    let counter = Arc::new(AtomicUsize::new(0));

    // 使用固定速率策略
    scheduler
        .add_periodic_with_policy(
            "rate_task",
            Duration::from_millis(100),
            SchedulingPolicy::FixedRate,
            {
                let counter = counter.clone();
                move || {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(250)).await;

    let rate_count = counter.load(Ordering::Relaxed);
    counter.store(0, Ordering::Relaxed);

    // 使用固定延迟策略
    scheduler
        .add_periodic_with_policy(
            "delay_task",
            Duration::from_millis(100),
            SchedulingPolicy::FixedDelay,
            {
                let counter = counter.clone();
                move || {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::Relaxed);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            },
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(250)).await;
    let delay_count = counter.load(Ordering::Relaxed);

    // 固定速率和固定延迟都应至少执行 1 次
    assert!(rate_count >= 1 && delay_count >= 1);
}

#[tokio::test]
async fn test_default_trait() {
    let scheduler = Scheduler::default();
    assert_eq!(scheduler.task_count(), 0);
    assert!(!scheduler.is_stopped());
}

#[tokio::test]
async fn test_task_removed_during_execution() {
    let scheduler = Scheduler::new();
    let task_started = Arc::new(AtomicBool::new(false));
    let task_removed = Arc::new(AtomicBool::new(false));

    scheduler
        .add_fixed_rate("task", Duration::from_millis(100), {
            let task_started = task_started.clone();
            let task_removed = task_removed.clone();
            move || {
                let task_started = task_started.clone();
                let task_removed = task_removed.clone();
                async move {
                    task_started.store(true, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    task_removed.store(true, Ordering::Relaxed);
                }
            }
        })
        .unwrap();

    // 等待任务启动
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(task_started.load(Ordering::Relaxed));

    // 在任务执行期间移除
    scheduler.remove("task").unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    // 任务应该能够完成执行
    assert!(task_removed.load(Ordering::Relaxed));
}
