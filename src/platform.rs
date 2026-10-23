use crate::model::Event;
use anyhow::{ensure, Result};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use windows::{
    core::{w, HSTRING, PCWSTR},
    Win32::{
        Foundation::*,
        System::{LibraryLoader::GetModuleHandleW, Threading::*},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
};

const TRAY_CLASS: PCWSTR = w!("BoseHeadphoneControl.Tray");
const TRAY_CALLBACK: u32 = WM_APP + 1;
const OPEN_APP: u32 = WM_APP + 2;
const QUIT_APP: u32 = WM_APP + 3;
const STOP_TRAY: u32 = WM_APP + 4;

pub fn set_app_identity() -> Result<()> {
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(w!("BoseHeadphoneControl.Desktop"))?;
    }
    Ok(())
}

pub struct Instance(HANDLE);
impl Instance {
    pub fn acquire() -> Result<Option<Self>> {
        for attempt in 0..2 {
            unsafe {
                // Share the legacy mutex to prevent parallel Bluetooth sessions.
                let handle = CreateMutexW(None, false, w!(r"Local\QuietRebellion.Tray"))?;
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let _ = CloseHandle(handle);
                    match signal(true) {
                        Ok(()) => return Ok(None),
                        Err(_) if attempt == 0 => continue,
                        Err(error) => return Err(error),
                    }
                }
                return Ok(Some(Self(handle)));
            }
        }
        unreachable!()
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub fn signal(show: bool) -> Result<()> {
    signal_message(if show { OPEN_APP } else { QUIT_APP })
}
fn signal_message(message: u32) -> Result<()> {
    // The tray may still be initializing during a second launch.
    for _ in 0..40 {
        unsafe {
            if let Ok(hwnd) = FindWindowW(TRAY_CLASS, None) {
                PostMessageW(hwnd, message, WPARAM(0), LPARAM(0))?;
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // Older installations receive named events instead of window messages.
    unsafe {
        let name = if message == OPEN_APP {
            w!(r"Local\QuietRebellion.Show")
        } else {
            w!(r"Local\QuietRebellion.Quit")
        };
        let event = OpenEventW(EVENT_MODIFY_STATE, false, name)?;
        let result = SetEvent(event);
        let _ = CloseHandle(event);
        result?;
    }
    Ok(())
}

struct TrayState {
    main: HWND,
    tx: mpsc::Sender<Event>,
    quitting: Arc<AtomicBool>,
    taskbar_created: u32,
    icon: HICON,
    context: Option<eframe::egui::Context>,
}
impl TrayState {
    unsafe fn show(&self, command: SHOW_WINDOW_CMD) {
        if GetWindowThreadProcessId(self.main, None) == GetCurrentThreadId() {
            let _ = ShowWindow(self.main, command);
        } else {
            let _ = ShowWindowAsync(self.main, command);
        }
    }
    unsafe fn notify_data(&self, hwnd: HWND) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: TRAY_CALLBACK,
            hIcon: self.icon,
            ..Default::default()
        };
        for (i, c) in "Bose Headphone Control".encode_utf16().enumerate() {
            data.szTip[i] = c;
        }
        data
    }
    unsafe fn register_icon(&self, hwnd: HWND) -> Result<()> {
        let mut data = self.notify_data(hwnd);
        ensure!(
            Shell_NotifyIconW(NIM_ADD, &data).as_bool()
                || Shell_NotifyIconW(NIM_MODIFY, &data).as_bool(),
            "Tray icon creation failed"
        );
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        ensure!(
            Shell_NotifyIconW(NIM_SETVERSION, &data).as_bool(),
            "Tray event version failed"
        );
        Ok(())
    }
    unsafe fn open(&self) {
        if self.quitting.load(Ordering::Acquire) {
            return;
        }
        let _ = self.tx.send(Event::Show);
        self.show(if IsIconic(self.main).as_bool() {
            SW_RESTORE
        } else {
            SW_SHOW
        });
        let _ = SetForegroundWindow(self.main);
        let _ = PostMessageW(self.main, WM_NULL, WPARAM(0), LPARAM(0));
        if let Some(context) = &self.context {
            context.request_repaint();
        }
    }
    unsafe fn quit(&self) {
        self.quitting.store(true, Ordering::Release);
        let _ = self.tx.send(Event::Quit);
        // Wake hidden windows so eframe can process the close request.
        if !IsWindowVisible(self.main).as_bool() || IsIconic(self.main).as_bool() {
            self.show(SW_SHOWNOACTIVATE);
        }
        let _ = PostMessageW(self.main, WM_CLOSE, WPARAM(0), LPARAM(0));
        if let Some(context) = &self.context {
            context.request_repaint();
        }
    }
    unsafe fn menu(&self, hwnd: HWND) {
        if let Ok(menu) = CreatePopupMenu() {
            let l = crate::localization::Localization::new();
            let _ = AppendMenuW(menu, MF_STRING, 1, &HSTRING::from(l.text("tray.open")));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            let _ = AppendMenuW(menu, MF_STRING, 2, &HSTRING::from(l.text("action.exit")));
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            let _ = SetForegroundWindow(hwnd);
            let id = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                p.x,
                p.y,
                0,
                hwnd,
                None,
            )
            .0;
            let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            self.command(id as usize);
        }
    }
    unsafe fn command(&self, id: usize) {
        match id {
            1 => self.open(),
            2 => self.quit(),
            _ => {}
        }
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(lp.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const TrayState;
    if !state.is_null() {
        let state = &*state;
        match msg {
            OPEN_APP => {
                state.open();
                return LRESULT(0);
            }
            QUIT_APP => {
                state.quit();
                return LRESULT(0);
            }
            STOP_TRAY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            WM_COMMAND => {
                state.command(wp.0 & 0xffff);
                return LRESULT(0);
            }
            TRAY_CALLBACK => {
                match (lp.0 as u32) & 0xffff {
                    NIN_SELECT | 1025 | WM_LBUTTONUP | WM_LBUTTONDBLCLK => state.open(),
                    WM_CONTEXTMENU | WM_RBUTTONUP => state.menu(hwnd),
                    _ => {}
                }
                return LRESULT(0);
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            _ if msg == state.taskbar_created && msg != 0 => {
                if let Err(e) = state.register_icon(hwnd) {
                    crate::worker::log(&format!("Restore tray icon: {e:#}"));
                }
                return LRESULT(0);
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

pub struct Tray {
    pub events: mpsc::Receiver<Event>,
    quitting: Arc<AtomicBool>,
    hwnd: HWND,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Tray {
    pub fn start(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let handle = cc
            .window_handle()
            .map_err(|e| anyhow::anyhow!("Native window unavailable: {e}"))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            anyhow::bail!("Windows native window required");
        };
        Self::for_window(handle.hwnd.get() as usize, Some(cc.egui_ctx.clone()))
    }
    fn for_window(main: usize, context: Option<eframe::egui::Context>) -> Result<Self> {
        let (tx, events) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let quitting = Arc::new(AtomicBool::new(false));
        let native_quitting = quitting.clone();
        let thread = std::thread::Builder::new()
            .name("native-tray".into())
            .spawn(move || unsafe {
                match NativeTray::create(HWND(main as *mut _), tx, native_quitting, context) {
                    Ok(tray) => {
                        if ready_tx.send(Ok(tray.hwnd.0 as usize)).is_err() {
                            return;
                        }
                        let mut message = MSG::default();
                        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(format!("{error:#}")));
                    }
                }
            })?;
        let hwnd = match ready_rx.recv()? {
            Ok(hwnd) => HWND(hwnd as *mut _),
            Err(error) => {
                let _ = thread.join();
                anyhow::bail!(error);
            }
        };
        Ok(Self {
            events,
            quitting,
            hwnd,
            thread: Some(thread),
        })
    }
    pub fn quitting(&self) -> bool {
        self.quitting.load(Ordering::Acquire)
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(self.hwnd, WM_CANCELMODE, WPARAM(0), LPARAM(0));
            let _ = PostMessageW(self.hwnd, STOP_TRAY, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
struct NativeTray {
    state: Box<TrayState>,
    hwnd: HWND,
}
impl NativeTray {
    unsafe fn create(
        main: HWND,
        tx: mpsc::Sender<Event>,
        quitting: Arc<AtomicBool>,
        context: Option<eframe::egui::Context>,
    ) -> Result<Self> {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            lpszClassName: TRAY_CLASS,
            hInstance: instance.into(),
            ..Default::default()
        };
        ensure!(RegisterClassW(&wc) != 0, "Tray window registration failed");
        let icon = LoadImageW(
            instance,
            PCWSTR(101usize as *const u16),
            IMAGE_ICON,
            32,
            32,
            LR_DEFAULTCOLOR,
        )?;
        let mut state = Box::new(TrayState {
            main,
            tx,
            quitting,
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            icon: HICON(icon.0),
            context,
        });
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            TRAY_CLASS,
            w!("Bose Headphone Control"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            Some((&mut *state as *mut TrayState).cast()),
        )?;
        if let Err(error) = state.register_icon(hwnd) {
            let _ = DestroyWindow(hwnd);
            let _ = DestroyIcon(state.icon);
            let _ = UnregisterClassW(TRAY_CLASS, instance);
            return Err(error);
        }
        Ok(Self { state, hwnd })
    }
}
impl Drop for NativeTray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.state.notify_data(self.hwnd));
            let _ = DestroyWindow(self.hwnd);
            let _ = DestroyIcon(self.state.icon);
            if let Ok(instance) = GetModuleHandleW(None) {
                let _ = UnregisterClassW(TRAY_CLASS, instance);
            }
        }
    }
}
