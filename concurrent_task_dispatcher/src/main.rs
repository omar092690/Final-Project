use rand::Rng;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

const TASK_COUNT: usize = 1000;
const WORKER_COUNT: usize = 8;
const ARRIVAL_INTERVAL_MS: u64 = 20;
const CPU_CAP: u32 = 100;

#[derive(Clone, Debug)]
enum TaskKind {
    IO,
    CPU,
}

#[derive(Clone, Debug)]
struct Task {
    id: usize,
    arrival_time: u128,
    kind: TaskKind,
    duration_ms: u64,
    cpu_cost: u32,
}

#[derive(Clone, Copy)]
enum Policy {
    FIFO,
    Optimized,
}

struct SharedState {
    queue: VecDeque<Task>,
    generator_done: bool,

    completed: usize,
    io_completed: usize,
    cpu_completed: usize,

    total_wait_time: u128,
    io_wait_time: u128,
    cpu_wait_time: u128,
    total_turnaround_time: u128,
    max_wait_time: u128,
    max_wait_task_id: usize,

    active_workers: usize,
    current_cpu_usage: u32,

    monitor_samples: usize,
    cpu_usage_sum: u128,
    active_workers_sum: u128,
}

fn create_task(id: usize, arrival_time: u128, io_ratio: f64) -> Task {
    let mut rng = rand::thread_rng();
    let random_value: f64 = rng.gen_range(0.0..1.0);

    let kind = if random_value < io_ratio {
        TaskKind::IO
    } else {
        TaskKind::CPU
    };

    let cpu_cost = match kind {
        TaskKind::IO => 10,
        TaskKind::CPU => 35,
    };

    Task {
        id,
        arrival_time,
        kind,
        duration_ms: 200,
        cpu_cost,
    }
}

fn choose_task(state: &mut SharedState, policy: Policy) -> Option<Task> {
    match policy {
        Policy::FIFO => {
            if let Some(task) = state.queue.front() {
                if state.current_cpu_usage + task.cpu_cost <= CPU_CAP {
                    return state.queue.pop_front();
                }
            }
            None
        }

        Policy::Optimized => {
            let available_cpu = CPU_CAP - state.current_cpu_usage;

            let mut best_index: Option<usize> = None;
            let mut best_cpu_cost = 0;

            for (index, task) in state.queue.iter().enumerate() {
                if task.cpu_cost <= available_cpu && task.cpu_cost > best_cpu_cost {
                    best_index = Some(index);
                    best_cpu_cost = task.cpu_cost;
                }
            }

            if let Some(index) = best_index {
                return state.queue.remove(index);
            }

            None
        }
    }
}

fn worker(
    worker_id: usize,
    shared: Arc<(Mutex<SharedState>, Condvar)>,
    simulation_start: Instant,
    policy: Policy,
) {
    loop {
        let task = {
            let (lock, cvar) = &*shared;
            let mut state = lock.lock().unwrap();

            loop {
                if let Some(task) = choose_task(&mut state, policy) {
                    state.active_workers += 1;
                    state.current_cpu_usage += task.cpu_cost;
                    break task;
                }

                if state.generator_done && state.queue.is_empty() {
                    return;
                }

                state = cvar.wait(state).unwrap();
            }
        };

        let start_time = simulation_start.elapsed().as_millis();
        let wait_time = start_time.saturating_sub(task.arrival_time);

        println!(
            "worker {} processing task {} ({:?})",
            worker_id, task.id, task.kind
        );

        thread::sleep(Duration::from_millis(task.duration_ms));

        let end_time = simulation_start.elapsed().as_millis();
        let turnaround_time = end_time.saturating_sub(task.arrival_time);

        let (lock, cvar) = &*shared;
        let mut state = lock.lock().unwrap();

        state.completed += 1;
        state.total_wait_time += wait_time;
        state.total_turnaround_time += turnaround_time;

        if wait_time > state.max_wait_time {
            state.max_wait_time = wait_time;
            state.max_wait_task_id = task.id;
        }

        match task.kind {
            TaskKind::IO => {
                state.io_completed += 1;
                state.io_wait_time += wait_time;
            }
            TaskKind::CPU => {
                state.cpu_completed += 1;
                state.cpu_wait_time += wait_time;
            }
        }

        state.active_workers -= 1;
        state.current_cpu_usage -= task.cpu_cost;

        cvar.notify_all();
    }
}

fn monitor(
    shared: Arc<(Mutex<SharedState>, Condvar)>,
    running: Arc<AtomicBool>,
    filename: String,
    simulation_start: Instant,
) {
    let mut file = File::create(filename).expect("Could not create monitor log file");

    writeln!(file, "time_ms,cpu_usage,active_workers")
        .expect("Could not write CSV header");

    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(10));

        let (lock, _) = &*shared;
        let mut state = lock.lock().unwrap();

        let time_ms = simulation_start.elapsed().as_millis();

        state.monitor_samples += 1;
        state.cpu_usage_sum += state.current_cpu_usage as u128;
        state.active_workers_sum += state.active_workers as u128;

        writeln!(
            file,
            "{},{},{}",
            time_ms,
            state.current_cpu_usage,
            state.active_workers
        )
        .expect("Could not write monitor data");
    }
}

fn run_simulation(name: &str, policy: Policy, io_ratio: f64) {
    println!("\n== {} simulation ==", name);
    println!(
    "{} tasks, {:.0}% IO / {:.0}% CPU, {} workers, cap {}%",
    TASK_COUNT,
    io_ratio * 100.0,
    (1.0 - io_ratio) * 100.0,
    WORKER_COUNT,
    CPU_CAP
);

    let shared = Arc::new((
        Mutex::new(SharedState {
            queue: VecDeque::new(),
            generator_done: false,

            completed: 0,
            io_completed: 0,
            cpu_completed: 0,

            total_wait_time: 0,
            io_wait_time: 0,
            cpu_wait_time: 0,
            total_turnaround_time: 0,
            max_wait_time: 0,
            max_wait_task_id: 0,

            active_workers: 0,
            current_cpu_usage: 0,

            monitor_samples: 0,
            cpu_usage_sum: 0,
            active_workers_sum: 0,
        }),
        Condvar::new(),
    ));

    let simulation_start = Instant::now();
    let monitor_running = Arc::new(AtomicBool::new(true));

    let monitor_shared = Arc::clone(&shared);
let monitor_flag = Arc::clone(&monitor_running);

let log_filename = format!("logs/{}_monitor_log.csv", name.to_lowercase());

let monitor_handle = thread::spawn(move || {
    monitor(monitor_shared, monitor_flag, log_filename, simulation_start);
});

    let mut worker_handles = Vec::new();

    for worker_id in 0..WORKER_COUNT {
        let worker_shared = Arc::clone(&shared);

        let handle = thread::spawn(move || {
            worker(worker_id, worker_shared, simulation_start, policy);
        });

        worker_handles.push(handle);
    }

    let generator_shared = Arc::clone(&shared);
    let generator_handle = thread::spawn(move || {
        for id in 1..=TASK_COUNT {
            let arrival_time = simulation_start.elapsed().as_millis();
            let task = create_task(id, arrival_time, io_ratio);

            let (lock, cvar) = &*generator_shared;
            let mut state = lock.lock().unwrap();

            state.queue.push_back(task);
            cvar.notify_all();

            drop(state);
            thread::sleep(Duration::from_millis(ARRIVAL_INTERVAL_MS));
        }

        let (lock, cvar) = &*generator_shared;
        let mut state = lock.lock().unwrap();

        state.generator_done = true;
        cvar.notify_all();
    });

    generator_handle.join().unwrap();

    for handle in worker_handles {
        handle.join().unwrap();
    }

    monitor_running.store(false, Ordering::SeqCst);
    monitor_handle.join().unwrap();

    let total_runtime = simulation_start.elapsed().as_millis();

    let (lock, _) = &*shared;
    let state = lock.lock().unwrap();

    let avg_wait = state.total_wait_time as f64 / state.completed as f64;
    let avg_turnaround = state.total_turnaround_time as f64 / state.completed as f64;
    let avg_cpu = state.cpu_usage_sum as f64 / state.monitor_samples as f64;
    let avg_workers = state.active_workers_sum as f64 / state.monitor_samples as f64;

    println!("\n— results —");
    println!("{:<22}: {} ms", "total runtime", total_runtime);
    println!("{:<22}: {} ms", "makespan", total_runtime);
    println!(
        "{:<22}: {} (IO={}, CPU={})",
        "tasks completed", state.completed, state.io_completed, state.cpu_completed
    );
    println!("{:<22}: {:.2} ms", "avg wait time", avg_wait);

    if let Policy::Optimized = policy {
        let avg_io_wait = state.io_wait_time as f64 / state.io_completed as f64;
        let avg_cpu_wait = state.cpu_wait_time as f64 / state.cpu_completed as f64;

        println!("{:<22}: {:.2} ms", "avg wait (IO only)", avg_io_wait);
        println!("{:<22}: {:.2} ms", "avg wait (CPU only)", avg_cpu_wait);
    }

    println!("{:<22}: {:.2} ms", "avg turnaround time", avg_turnaround);
    println!(
        "{:<22}: {} ms (task #{})",
        "max wait time", state.max_wait_time, state.max_wait_task_id
    );
    println!("{:<22}: {:.2} %", "avg CPU usage", avg_cpu);
    println!(
        "{:<22}: {:.2} / {}",
        "avg workers active", avg_workers, WORKER_COUNT
    );
    println!("{:<22}: {}", "monitor samples", state.monitor_samples);
    println!(
    "{:<22}: logs/{}_monitor_log.csv",
    "monitor csv",
    name.to_lowercase()
);
}

fn main() {
    // Balanced workload
    run_simulation("FIFO balanced", Policy::FIFO, 0.70);
    run_simulation("Optimized balanced", Policy::Optimized, 0.70);

    run_simulation("FIFO stressed", Policy::FIFO, 0.20);
    run_simulation("Optimized stressed", Policy::Optimized, 0.20);
}