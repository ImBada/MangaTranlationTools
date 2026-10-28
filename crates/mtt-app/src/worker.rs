//! Background work: a small pool for parallel jobs (image decoding, rendering) and one
//! serial thread for writes, so saves land on disk in the order they were requested.

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
                        if tx.send(task()).is_err() {
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
                    if result_tx.send(task()).is_err() {
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

    pub fn drain(&self) -> Vec<R> {
        self.results.try_iter().collect()
    }
}
