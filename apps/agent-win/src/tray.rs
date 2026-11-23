use eq_backend::{Backend, Level};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

const TRAY: u32 = WM_APP + 1;
const OPEN: usize = 1;
const QUIT: usize = 2;
const TICK_MS: u32 = 5000;
/// Windows or driver updates can also unhook devices that stay plugged in.
const RECHECK: Duration = Duration::from_secs(600);

struct Agent {
    backend: Box<dyn Backend>,
    devices: Vec<eq_backend::Device>,
    checked: Option<Instant>,
    told: String,
    taskbar_created: u32,
}

thread_local! {
    static AGENT: RefCell<Option<Agent>> = const { RefCell::new(None) };
}

fn fill(dst: &mut [u16], text: &str) {
    let room = dst.len() - 1;
    let units: Vec<u16> = text.encode_utf16().take(room).collect();
    dst[..units.len()].copy_from_slice(&units);
    dst[units.len()] = 0;
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uCallbackMessage: TRAY,
        ..Default::default()
    }
}

fn add_icon(hwnd: HWND) {
    let mut d = icon_data(hwnd);
    d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    fill(&mut d.szTip, "EQaura");
    // SAFETY: loads our own icon resource, or the system default without one.
    unsafe {
        let module = GetModuleHandleW(None).ok();
        d.hIcon = module
            .and_then(|m| LoadIconW(Some(m.into()), PCWSTR(1 as _)).ok())
            .or_else(|| LoadIconW(None, IDI_APPLICATION).ok())
            .unwrap_or_default();
        let _ = Shell_NotifyIconW(NIM_ADD, &d);
    }
}

fn notify(hwnd: HWND, title: &str, text: &str) {
    let mut d = icon_data(hwnd);
    d.uFlags = NIF_INFO;
    d.dwInfoFlags = NIIF_WARNING;
    fill(&mut d.szInfoTitle, title);
    fill(&mut d.szInfo, text);
    // SAFETY: `d` is fully initialised.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
    }
}

fn tick(hwnd: HWND) {
    let message = AGENT.with(|a| {
        let mut a = a.try_borrow_mut().ok()?;
        let a = a.as_mut()?;
        let devices = a.backend.devices();
        let due = a.checked.is_none_or(|t| t.elapsed() > RECHECK);
        if devices == a.devices && !due {
            return None;
        }
        a.devices = devices;
        a.checked = Some(Instant::now());
        let default = a.devices.iter().find(|d| d.is_default).map(|d| d.name.clone());
        // Problems with the device in use, or with the whole setup.
        let problem = a.backend.checks().into_iter().find(|c| {
            c.level == Level::Bad || (c.level == Level::Warning && Some(&c.title) == default.as_ref())
        });
        let text = problem.map(|c| format!("{}. {}", c.title, c.detail)).unwrap_or_default();
        if text.is_empty() || text == a.told {
            a.told = text;
            return None;
        }
        a.told = text.clone();
        Some(text)
    });
    if let Some(text) = message {
        notify(hwnd, "The equalizer needs attention", &text);
    }
}

fn open_app() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe.with_file_name("eqaura.exe")).spawn();
    }
}

fn menu(hwnd: HWND) {
    // SAFETY: the menu is created, shown and destroyed here.
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let _ = AppendMenuW(menu, MF_STRING, OPEN, w!("Open EQaura"));
        let _ = AppendMenuW(menu, MF_STRING, QUIT, w!("Quit this helper"));
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, p.x, p.y, None, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => tick(hwnd),
        TRAY => match lp.0 as u32 & 0xffff {
            WM_LBUTTONUP | NIN_BALLOONUSERCLICK => open_app(),
            WM_RBUTTONUP => menu(hwnd),
            _ => {}
        },
        WM_COMMAND => match wp.0 & 0xffff {
            OPEN => open_app(),
            QUIT => {
                // SAFETY: our own window.
                let _ = unsafe { DestroyWindow(hwnd) };
            }
            _ => {}
        },
        WM_DESTROY => {
            // SAFETY: removes the icon added earlier and ends the loop.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
                PostQuitMessage(0);
            }
        }
        _ => {
            // Explorer restarted: put the icon back.
            let created =
                AGENT.with(|a| a.try_borrow().ok().and_then(|a| a.as_ref().map(|a| a.taskbar_created)));
            if created == Some(msg) && msg != 0 {
                add_icon(hwnd);
            }
            // SAFETY: default handling with the arguments we were given.
            return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
        }
    }
    LRESULT(0)
}

/// Sent when the user clicks the notification (WM_USER + 5).
const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;

pub fn run() -> Result<()> {
    // SAFETY: standard window setup on the main thread.
    unsafe {
        let _mutex = CreateMutexW(None, false, w!("Local\\EQauraAgent"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
        let instance = GetModuleHandleW(None)?;
        let class = w!("EQauraAgent");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("EQaura helper"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let agent = Agent {
            backend: eq_backend::system(eq_store::default_dir()),
            devices: Vec::new(),
            checked: None,
            told: String::new(),
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
        };
        AGENT.with(|a| *a.borrow_mut() = Some(agent));
        add_icon(hwnd);
        SetTimer(Some(hwnd), 1, TICK_MS, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        AGENT.with(|a| a.borrow_mut().take());
    }
    Ok(())
}
