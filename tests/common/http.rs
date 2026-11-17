//! Minimal concurrent Unix HTTP fixture with observed request/concurrency bounds.

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::unix::net::UnixListener,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

pub struct Server {
    pub socket: PathBuf,
    requests: Arc<Mutex<Vec<String>>>,
    peak: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    pub fn new(root: &Path, routes: BTreeMap<String, (u16, String)>, delay: Duration) -> Self {
        let socket = root.join("api.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let peak = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, peak_seen, stopped) = (requests.clone(), peak.clone(), stop.clone());
        let routes = Arc::new(routes);
        let worker = thread::spawn(move || {
            let mut workers = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (routes, seen, peak, active) = (
                            routes.clone(),
                            seen.clone(),
                            peak_seen.clone(),
                            active.clone(),
                        );
                        workers.push(thread::spawn(move || {
                            stream.set_nonblocking(false).unwrap();
                            stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                            let mut request = Vec::new();
                            let mut chunk = [0; 1024];
                            while request.len() < 8192 && !request.windows(4).any(|s| s == b"\r\n\r\n") {
                                let Ok(count) = stream.read(&mut chunk) else { return; };
                                if count == 0 { return; }
                                request.extend_from_slice(&chunk[..count]);
                            }
                            let text = String::from_utf8_lossy(&request);
                            let path = text.lines().next().unwrap_or_default().split_whitespace().nth(1).unwrap_or_default().to_owned();
                            seen.lock().unwrap().push(path.clone());
                            let inspect = path.starts_with("/containers/") && path.ends_with("/json");
                            if inspect {
                                let current = active.fetch_add(1, Ordering::Relaxed) + 1;
                                peak.fetch_max(current, Ordering::Relaxed);
                                thread::sleep(delay);
                            }
                            let (status, body) = routes.get(&path).cloned().unwrap_or((404, "{}".into()));
                            let header = format!("HTTP/1.1 {status} response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(body.as_bytes());
                            if inspect { active.fetch_sub(1, Ordering::Relaxed); }
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            socket,
            requests,
            peak,
            stop,
            thread: Some(worker),
        }
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::Relaxed)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.thread.take() {
            worker.join().unwrap();
        }
    }
}
