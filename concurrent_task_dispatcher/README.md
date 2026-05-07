# Concurrent Task Dispatcher in Rust

## Project Description
This project simulates a concurrent task dispatcher in Rust. Tasks are generated over time, placed into a queue, and processed by a bounded worker pool. The program compares a FIFO scheduler with an optimized scheduler.

---

## How to Run

```bash
cargo run
```

For cleaner output:

```bash
cargo run | grep "simulation\|results\|total runtime\|tasks completed\|avg wait time\|avg CPU usage\|avg workers active\|monitor csv"
```

---

## Experiments

### FIFO Simulation
Uses a first-in, first-out scheduling policy.

### Optimized Simulation
Uses a CPU-aware scheduling policy that tries to keep CPU usage close to 100% without exceeding the cap.

---

## Metrics Collected
- Total runtime
- Makespan
- Tasks completed
- Average wait time
- Average turnaround time
- Max wait time
- Average CPU usage
- Average workers active
- Monitor samples
- CSV monitor logs

---

## Design Summary
The system uses:
- a generator thread
- a shared task queue
- worker threads
- a monitor thread
- mutexes
- condition variables
- atomic flags

---

## Tool Use Disclosure
I used ChatGPT to help debug Rust and Cargo errors and to help organize the overall structure of the project. The AI assistance was mainly used for troubleshooting concurrency issues,improving output formatting, and planning the architecture of the  system.