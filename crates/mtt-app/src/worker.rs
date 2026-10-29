//! Background work: a small pool for parallel jobs (image decoding, rendering) and one
//! serial thread for writes, so saves land on disk in the order they were requested.
//! A panicking task loses its event but not its thread; jobs that must always report back
//! wrap their work in [`catch_panic`] and send the error as their own failure event.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use eframe::egui;

type Task<R> = Box<dyn FnOnce() -> R + Send>;

pub struct Worker<R: Send + 'static> {
    parallel: Sender<Task<R>>,
    serial: Sender<Task<R>>,
    results: Receiver<R>,
    emitter: Emitter<R>,
}

/// Lets a long-running job (on its own thread) report several events.
pub struct Emitter<R> {
    sender: Sender<R>,
    ctx: egui::Context,
}

impl<R> Clone for Emitter<R> {
    fn clone(&self) -> Self {
        Self { sender: self.sender.clone(), ctx: self.ctx.clone() }
    }
}

impl<R> Emitter<R> {
    pub fn emit(&self, event: R) {
        let _ = self.sender.send(event);
        self.ctx.request_repaint();
    }
}

impl<R: Send + 'static> Worker<R> {
    pub fn new(ctx: &egui::Context, threads: usize) -> Self {
        let (result_tx, results) = channel::<R>();
        let (parallel, parallel_rx) = channel::<Task<R>>();
        let parallel_rx = Arc::new(Mutex::new(parallel_rx));
        for index in 0..threads.max(1) {
            let rx = Arc::clone(&parallel_rx);
            let tx = result_tx.clone();
            let ctx = ctx.clone();
            std::thread::Builder::new()
                .name(format!("mtt-worker-{index}"))
                .spawn(move || {
                    loop {
                        // Hold the lock only while taking the next task.
                        let task = match rx.lock() {
                            Ok(rx) => rx.recv(),
                            Err(_) => break,
                        };
                        let Ok(task) = task else { break };
                        let Ok(result) = catch_panic(task) else { continue };
                        if tx.send(result).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                })
                .expect("spawn worker thread");
        }
        let emitter = Emitter { sender: result_tx.clone(), ctx: ctx.clone() };
        let (serial, serial_rx) = channel::<Task<R>>();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("mtt-writer".into())
            .spawn(move || {
                for task in serial_rx {
                    let Ok(result) = catch_panic(task) else { continue };
                    if result_tx.send(result).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn writer thread");
        Self { parallel, serial, results, emitter }
    }

    pub fn spawn(&self, task: impl FnOnce() -> R + Send + 'static) {
        let _ = self.parallel.send(Box::new(task));
    }

    /// Runs after every previously queued serial task.
    pub fn spawn_serial(&self, task: impl FnOnce() -> R + Send + 'static) {
        let _ = self.serial.send(Box::new(task));
    }

    pub fn emitter(&self) -> Emitter<R> {
        self.emitter.clone()
    }

    /// For tasks that build textures (egui textures can be created from any thread).
    pub fn ctx(&self) -> &egui::Context {
        &self.emitter.ctx
    }

    pub fn drain(&self) -> Vec<R> {
        self.results.try_iter().collect()
    }
}

/// Runs `task`, turning a panic into an error message for the job's failure event (the
/// panic hook in `logging` has written the details to the log).
pub fn catch_panic<T>(task: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(task)).map_err(|payload| panic_message(payload.as_ref()))
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or_default();
    if text.is_empty() { "내부 오류가 발생했습니다.".into() } else { format!("내부 오류: {text}") }
}

/// Runs its closure when dropped during a panic, so a job thread still sends its final
/// event (e.g. a translation's `Finished`) when it dies.
pub struct OnPanic<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> OnPanic<F> {
    pub fn new(on_panic: F) -> Self {
        Self(Some(on_panic))
    }
}

impl<F: FnOnce()> Drop for OnPanic<F> {
    fn drop(&mut self) {
        if std::thread::panicking()
            && let Some(on_panic) = self.0.take()
        {
            on_panic();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn wait_for<R: Send>(worker: &Worker<R>, count: usize) -> Vec<R> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut results = Vec::new();
        while results.len() < count && Instant::now() < deadline {
            results.extend(worker.drain());
            std::thread::sleep(Duration::from_millis(5));
        }
        results
    }

    #[test]
    fn writer_keeps_running_after_a_task_panics() {
        let worker: Worker<u32> = Worker::new(&egui::Context::default(), 1);
        worker.spawn_serial(|| panic!("boom"));
        worker.spawn_serial(|| 7);
        worker.spawn(|| panic!("boom"));
        worker.spawn(|| 8);
        let mut results = wait_for(&worker, 2);
        results.sort_unstable();
        assert_eq!(results, [7, 8], "later tasks on both the pool and the writer still run");
    }

    #[test]
    fn panics_become_error_messages() {
        assert_eq!(catch_panic(|| 1), Ok(1));
        assert_eq!(catch_panic(|| -> u32 { panic!("bad index") }), Err("내부 오류: bad index".into()));
        let owned = String::from("owned");
        assert_eq!(catch_panic(|| -> u32 { panic!("{owned}") }), Err("내부 오류: owned".into()));
    }

    #[test]
    fn on_panic_runs_only_when_unwinding() {
        let (sender, receiver) = channel();
        let quiet = sender.clone();
        std::thread::spawn(move || {
            let _guard = OnPanic::new(|| quiet.send("normal").unwrap());
        })
        .join()
        .unwrap();
        let joined = std::thread::spawn(move || {
            let _guard = OnPanic::new(|| sender.send("panicked").unwrap());
            panic!("job died");
        })
        .join();
        assert!(joined.is_err());
        assert_eq!(receiver.try_iter().collect::<Vec<_>>(), ["panicked"]);
    }
}
