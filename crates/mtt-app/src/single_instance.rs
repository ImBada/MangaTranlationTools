//! One app per data folder (`requestSingleInstanceLock` in `electron/main.cjs`): the first
//! instance holds `native-app.lock` and listens on a local port; a second launch asks it
//! to come to the front and exits.

use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

const FOCUS_MESSAGE: &[u8] = b"focus";

pub enum Instance {
    /// This process owns the lock; keep the file open for the app's lifetime.
    Primary { _lock: File, listener: Option<TcpListener> },
    /// Another instance was asked to focus itself.
    Secondary,
}

pub fn acquire(data_root: &Path) -> Instance {
    let lock_path = data_root.join("native-app.lock");
    let port_path = data_root.join("native-app.port");
    let Ok(lock) = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path) else {
        return Instance::Primary { _lock: tempfile_fallback(), listener: None };
    };
    if lock.try_lock().is_err() {
        let focused = std::fs::read_to_string(&port_path)
            .ok()
            .and_then(|port| port.trim().parse::<u16>().ok())
            .and_then(|port| TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(1)).ok())
            .is_some_and(|mut stream| stream.write_all(FOCUS_MESSAGE).is_ok());
        if focused {
            return Instance::Secondary;
        }
        // The lock holder does not answer; run anyway rather than refuse to start.
        log::warn!("another instance holds the lock but did not respond");
        return Instance::Primary { _lock: lock, listener: None };
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok();
    if let Some(port) = listener.as_ref().and_then(|l| l.local_addr().ok()).map(|addr| addr.port()) {
        let _ = std::fs::write(&port_path, port.to_string());
    }
    Instance::Primary { _lock: lock, listener }
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
}
