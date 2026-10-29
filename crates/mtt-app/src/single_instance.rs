//! One app per data folder (`requestSingleInstanceLock` in `electron/main.cjs`): the first
//! instance holds `native-app.lock` and listens on a local port; a second launch asks it
//! to come to the front and exits.

use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

const FOCUS_MESSAGE: &[u8] = b"focus";
/// How long a second launch keeps trying the lock holder: it may still be starting and not
/// have written its port yet.
const ANSWER_WAIT: Duration = Duration::from_secs(2);

pub enum Instance {
    /// This process owns the lock; keep the file open for the app's lifetime.
    Primary { _lock: File, listener: Option<TcpListener> },
    /// Another instance was asked to focus itself.
    Secondary,
    /// Another process holds the lock but never answered. Running anyway would let two
    /// apps overwrite each other's chapters.
    Unresponsive,
}

pub fn acquire(data_root: &Path) -> Instance {
    acquire_within(data_root, ANSWER_WAIT)
}

fn acquire_within(data_root: &Path, wait: Duration) -> Instance {
    let lock_path = data_root.join("native-app.lock");
    let port_path = data_root.join("native-app.port");
    let Ok(lock) = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path) else {
        return Instance::Primary { _lock: tempfile_fallback(), listener: None };
    };
    let deadline = Instant::now() + wait;
    // The holder may also quit meanwhile, which makes this launch the primary.
    while lock.try_lock().is_err() {
        if ask_to_focus(&port_path) {
            return Instance::Secondary;
        }
        if Instant::now() >= deadline {
            log::warn!("another instance holds the lock but did not respond");
            return Instance::Unresponsive;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok();
    if let Some(port) = listener.as_ref().and_then(|l| l.local_addr().ok()).map(|addr| addr.port()) {
        let _ = std::fs::write(&port_path, port.to_string());
    }
    Instance::Primary { _lock: lock, listener }
}

/// Asks the app listening on the port written in `port_path` (read afresh each time) to
/// come to the front.
fn ask_to_focus(port_path: &Path) -> bool {
    std::fs::read_to_string(port_path)
        .ok()
        .and_then(|port| port.trim().parse::<u16>().ok())
        .and_then(|port| TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(500)).ok())
        .is_some_and(|mut stream| stream.write_all(FOCUS_MESSAGE).is_ok())
}

fn tempfile_fallback() -> File {
    tempfile::tempfile().expect("temporary file")
}

/// Calls `on_focus` whenever a second launch asks for the window.
pub fn serve(listener: TcpListener, on_focus: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("mtt-single-instance".into())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buffer = [0u8; 16];
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                if let Ok(read) = stream.read(&mut buffer)
                    && &buffer[..read] == FOCUS_MESSAGE
                {
                    on_focus();
                }
            }
        })
        .expect("spawn single-instance listener");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_launch_focuses_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let Instance::Primary { _lock, listener: Some(listener) } = acquire(dir.path()) else { panic!("primary") };
        let (sender, receiver) = std::sync::mpsc::channel();
        serve(listener, move || {
            let _ = sender.send(());
        });
        assert!(matches!(acquire(dir.path()), Instance::Secondary));
        receiver.recv_timeout(Duration::from_secs(2)).expect("focus request");
    }

    fn hold_lock(dir: &Path) -> File {
        let lock = File::create(dir.join("native-app.lock")).unwrap();
        lock.lock().unwrap();
        lock
    }

    #[test]
    fn a_silent_lock_holder_is_not_run_alongside() {
        let dir = tempfile::tempdir().unwrap();
        let _held = hold_lock(dir.path());
        // A stale port file: nothing listens there any more.
        let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
        std::fs::write(dir.path().join("native-app.port"), port.to_string()).unwrap();
        let started = Instant::now();
        assert!(matches!(acquire_within(dir.path(), Duration::from_millis(300)), Instance::Unresponsive));
        assert!(started.elapsed() >= Duration::from_millis(300), "kept trying for the whole wait");
    }

    #[test]
    fn a_starting_instance_gets_time_to_write_its_port() {
        let dir = tempfile::tempdir().unwrap();
        let _held = hold_lock(dir.path());
        let port_path = dir.path().join("native-app.port");
        let starting = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            std::fs::write(&port_path, listener.local_addr().unwrap().port().to_string()).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let mut message = Vec::new();
            stream.read_to_end(&mut message).unwrap();
            message
        });
        assert!(matches!(acquire_within(dir.path(), Duration::from_secs(5)), Instance::Secondary));
        assert_eq!(starting.join().unwrap(), FOCUS_MESSAGE);
    }

    #[test]
    fn the_lock_freed_while_waiting_makes_this_the_primary() {
        let dir = tempfile::tempdir().unwrap();
        let held = hold_lock(dir.path());
        let quitting = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(held);
        });
        assert!(matches!(acquire_within(dir.path(), Duration::from_secs(5)), Instance::Primary { .. }));
        quitting.join().unwrap();
    }
}
