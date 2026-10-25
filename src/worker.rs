use crate::{ble::Connection, localization::data_dir, model::*};
use std::{
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct Sender(pub Arc<(Mutex<Queue>, Condvar)>);
impl Sender {
    pub fn send(&self, c: Command, immediate: bool) -> bool {
        let (lock, cv) = &*self.0;
        let mut q = lock.lock().unwrap();
        if q.stopped || matches!(c, Command::Reconnect) && q.connecting {
            return false;
        }
        if matches!(c, Command::Reconnect) {
            q.connecting = true;
        }
        q.push(c, Instant::now(), immediate);
        cv.notify_one();
        true
    }
    pub fn stop(&self) {
        let (lock, cv) = &*self.0;
        let mut queue = lock.lock().unwrap();
        queue.commands.clear();
        queue.push(Command::Quit, Instant::now(), true);
        queue.stopped = true;
        cv.notify_one();
    }
}
pub struct Worker {
    pub sender: Sender,
    pub events: mpsc::Receiver<Event>,
    thread: Option<std::thread::JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}
pub fn log(text: &str) {
    let dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("rust-client.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
        let _ = std::fs::rename(&path, dir.join("rust-client.previous.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{:?} {text}", std::time::SystemTime::now());
    }
}
fn publish(tx: &mpsc::Sender<Event>, state: &State, ctx: &eframe::egui::Context) {
    let dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(bytes) = serde_json::to_vec_pretty(state) {
        let _ = std::fs::write(dir.join("rust-status.json"), bytes);
    }

    let _ = tx.send(Event::State(Box::new(state.clone())));
    ctx.request_repaint();
}
impl Worker {
    pub fn start(ctx: eframe::egui::Context) -> Self {
        let pair = Arc::new((Mutex::new(Queue::new()), Condvar::new()));
        let sender = Sender(pair.clone());
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_worker = cancel.clone();
        let thread = std::thread::spawn(move || {
            crate::ble::set_cancellation(cancel_worker.clone());
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_MULTITHREADED,
                );
            }
            let mut connection: Option<Connection> = None;
            let mut address = None;
            let mut next_poll = Instant::now();
            let mut next_source_poll = Instant::now();
            let mut retry = Instant::now();
            let mut failures = 0u32;
            let mut suspended = false;
            loop {
                let command = {
                    let (lock, cv) = &*pair;
                    let mut q = lock.lock().unwrap();
                    if q.stopped {
                        break;
                    }
                    let c = q.pop_ready(Instant::now());
                    if c.is_none() {
                        let waited = cv.wait_timeout(q, Duration::from_millis(12)).unwrap();
                        q = waited.0;
                        if q.stopped {
                            break;
                        }
                    }
                    c
                };
                if matches!(command, Some(Command::Quit)) {
                    break;
                }
                if let Some(Command::Reconnect) = command {
                    connection = None;
                    address = None;
                    failures = 0;
                    suspended = false;
                    retry = Instant::now();
                    pair.0.lock().unwrap().clear_device_edits();
                }
                if connection.is_none() && !suspended && Instant::now() >= retry {
                    {
                        let mut queue = pair.0.lock().unwrap();
                        queue.connecting = true;
                        queue
                            .commands
                            .retain(|(command, _)| !matches!(command, Command::Reconnect));
                    }
                    let _ = tx.send(Event::Connecting);
                    ctx.request_repaint();
                    let result = Connection::connect(address);
                    pair.0.lock().unwrap().connecting = false;
                    match result {
                        Ok((conn, addr)) => {
                            address = Some(addr);
                            log(&format!("Bluetooth connected: {}", conn.state.name));
                            publish(&tx, &conn.state, &ctx);
                            connection = Some(conn);
                            next_poll = Instant::now() + Duration::from_secs(5);
                            next_source_poll = Instant::now() + Duration::from_secs(1);
                            failures = 0;
                        }
                        Err(e) => {
                            if cancel_worker.load(Ordering::Relaxed) {
                                break;
                            }
                            log(&format!("Connect: {e:#}"));
                            let _ = tx.send(Event::Disconnected(e.to_string()));
                            ctx.request_repaint();
                            retry = Instant::now()
                                + Duration::from_secs((5u64 << failures.min(4)).min(60));
                            failures = failures.saturating_add(1);
                        }
                    }
                }
                if let Some(command) = command {
                    if matches!(command, Command::Reconnect) {
                        continue;
                    }
                    let connectivity_revision = match &command {
                        Command::Connectivity { revision, .. } => Some(*revision),
                        _ => None,
                    };
                    if let Some(conn) = &mut connection {
                        let confirmed_before = conn.state.clone();
                        match conn.execute(&command) {
                            Ok(value) => {
                                publish(&tx, &conn.state, &ctx);
                                match &command {
                                    Command::ConnectDevice(address) => {
                                        let _ = tx.send(Event::DeviceAck(
                                            address.clone(),
                                            DeviceAction::Connect,
                                        ));
                                        ctx.request_repaint();
                                    }
                                    Command::DisconnectDevice(address) => {
                                        let _ = tx.send(Event::DeviceAck(
                                            address.clone(),
                                            DeviceAction::Disconnect,
                                        ));
                                        ctx.request_repaint();
                                    }
                                    _ => {}
                                }
                                if let Command::ResetEq(edits) = &command {
                                    let _ = tx.send(Event::ResetAck(edits.clone()));
                                    ctx.request_repaint();
                                }
                                if let (Command::Slider(e), Some(value)) = (&command, value) {
                                    let _ = tx.send(Event::Ack(*e, value));
                                    ctx.request_repaint();
                                }
                                if matches!(command, Command::Power | Command::Pair) {
                                    connection = None;
                                    suspended = true;
                                    let _ = tx.send(Event::Disconnected(String::new()));
                                    ctx.request_repaint();
                                }
                            }
                            Err(e) => {
                                if cancel_worker.load(Ordering::Relaxed) {
                                    break;
                                }
                                conn.state = confirmed_before;
                                let rejected = e
                                    .downcast_ref::<crate::protocol::DeviceRejected>()
                                    .is_some();
                                let device = match &command {
                                    Command::ConnectDevice(address) => {
                                        Some((address, DeviceAction::Connect))
                                    }
                                    Command::DisconnectDevice(address) => {
                                        Some((address, DeviceAction::Disconnect))
                                    }
                                    _ => None,
                                };
                                let recovered =
                                    (rejected || device.is_some()) && conn.poll().is_ok();
                                publish(&tx, &conn.state, &ctx);
                                log(&format!("Command: {e:#}"));
                                let edit = if let Command::Slider(e) = &command {
                                    Some(*e)
                                } else {
                                    None
                                };
                                if recovered {
                                    if let Command::ResetEq(edits) = &command {
                                        let _ = tx.send(Event::ResetAck(edits.clone()));
                                    }
                                    if let Some((address, action)) = device {
                                        let _ =
                                            tx.send(Event::DeviceFailed(address.clone(), action));
                                    } else {
                                        let _ = tx.send(Event::Rejected(edit));
                                    }
                                } else {
                                    if let Some((address, action)) = device {
                                        let _ =
                                            tx.send(Event::DeviceFailed(address.clone(), action));
                                    } else {
                                        let _ = tx.send(Event::Failed(edit, e.to_string()));
                                    }
                                    connection = None;
                                    pair.0.lock().unwrap().clear_device_edits();
                                    let _ = tx.send(Event::Disconnected(e.to_string()));
                                    retry = Instant::now() + Duration::from_secs(5);
                                }
                                ctx.request_repaint();
                            }
                        }
                    } else {
                        match command {
                            Command::Slider(e) => {
                                let _ = tx
                                    .send(Event::Failed(Some(e), "Headphones disconnected".into()));
                            }
                            Command::ConnectDevice(address) => {
                                let _ =
                                    tx.send(Event::DeviceFailed(address, DeviceAction::Connect));
                            }
                            Command::DisconnectDevice(address) => {
                                let _ =
                                    tx.send(Event::DeviceFailed(address, DeviceAction::Disconnect));
                            }
                            _ => {}
                        }
                        ctx.request_repaint();
                    }
                    if let Some(revision) = connectivity_revision {
                        let _ = tx.send(Event::ConnectivityDone(revision));
                        ctx.request_repaint();
                    }
                    next_poll = Instant::now() + Duration::from_secs(5);
                    next_source_poll = Instant::now() + Duration::from_secs(1);
                } else if connection.is_some()
                    && (Instant::now() >= next_poll || Instant::now() >= next_source_poll)
                    && pair.0.lock().unwrap().commands.is_empty()
                {
                    let full_poll = Instant::now() >= next_poll;
                    let result = if full_poll {
                        connection.as_mut().unwrap().poll()
                    } else {
                        connection.as_mut().unwrap().source()
                    };
                    match result {
                        Ok(()) => publish(&tx, &connection.as_ref().unwrap().state, &ctx),
                        Err(e) => {
                            if cancel_worker.load(Ordering::Relaxed) {
                                break;
                            }
                            log(&format!("Poll: {e:#}"));
                            connection = None;
                            pair.0.lock().unwrap().clear_device_edits();
                            let _ = tx.send(Event::Disconnected(e.to_string()));
                            ctx.request_repaint();
                            retry = Instant::now() + Duration::from_secs(5);
                        }
                    }
                    if full_poll {
                        next_poll = Instant::now() + Duration::from_secs(5);
                    }
                    next_source_poll = Instant::now() + Duration::from_secs(1);
                }
            }
            drop(connection);
            unsafe {
                windows::Win32::System::Com::CoUninitialize();
            }
        });
        Self {
            sender,
            events,
            thread: Some(thread),
            cancel,
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.sender.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
