use crossbeam::channel::{self, Receiver, Sender};
use crossbeam::deque::{Injector, Steal, Stealer, Worker as WorkerQueue};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::sync::atomic::{AtomicBool, Ordering};

type Job = Box<dyn FnOnce() + Send + 'static>;
type LocalQueue = WorkerQueue<Job>;

pub struct WorkerPool {
    workers: Vec<Worker>,
    injector: Arc<Injector<Job>>,
    stealers: Vec<Stealer<Job>>,
    shutdown: Arc<AtomicBool>,
}

struct Worker {
    id: usize,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(
        id: usize,
        local_queue: LocalQueue,
        injector: Arc<Injector<Job>>,
        stealers: Vec<Stealer<Job>>,
        shutdown: Arc<AtomicBool>,
    ) -> Worker {
        let thread = thread::spawn(move || {
            // 设置 CPU 亲和性
            if let Err(e) = crate::server::performance::set_thread_affinity(id) {
                eprintln!("Warning: Failed to set CPU affinity for worker {}: {:?}", id, e);
            }
            
            // Work-stealing 主循环
            while !shutdown.load(Ordering::Relaxed) {
                // 1. 优先处理本地队列
                if let Some(job) = local_queue.pop() {
                    job();
                    continue;
                }
                
                // 2. 从全局注入器获取任务
                match injector.steal() {
                    Steal::Success(job) => {
                        job();
                        continue;
                    }
                    _ => {}
                }
                
                // 3. 尝试从其他工作线程窃取任务
                let mut found_work = false;
                for stealer in &stealers {
                    match stealer.steal() {
                        Steal::Success(job) => {
                            job();
                            found_work = true;
                            break;
                        }
                        _ => {}
                    }
                }
                
                if !found_work {
                    // 没有找到任务，短暂休眠避免忙等待
                    std::thread::yield_now();
                }
            }
        });
        
        Worker {
            id,
            thread: Some(thread),
        }
    }
}

impl WorkerPool {
    pub fn new(size: usize) -> WorkerPool {
        assert!(size > 0);
        
        let injector = Arc::new(Injector::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        
        // 创建本地队列和窃取器
        let mut local_queues = Vec::with_capacity(size);
        let mut stealers = Vec::with_capacity(size);
        
        for _ in 0..size {
            let queue = WorkerQueue::new_fifo();
            stealers.push(queue.stealer());
            local_queues.push(queue);
        }
        
        // 创建工作线程
        let mut workers = Vec::with_capacity(size);
        for (id, local_queue) in local_queues.into_iter().enumerate() {
            let worker = Worker::new(
                id,
                local_queue,
                Arc::clone(&injector),
                stealers.clone(),
                Arc::clone(&shutdown),
            );
            workers.push(worker);
        }
        
        WorkerPool {
            workers,
            injector,
            stealers,
            shutdown,
        }
    }
    
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        
        // 将任务推送到全局注入器
        self.injector.push(job);
    }
    
    pub fn active_count(&self) -> usize {
        self.workers.len()
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        // 设置关闭标志
        self.shutdown.store(true, Ordering::Relaxed);
        
        // 等待所有工作线程完成
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}