use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub enum TrayEvent {
    Show,
    Quit,
}

const WM_TRAYICON: u32 = 0x0400 + 1;
const WM_SHOW: u32 = 0x0400 + 2;
const WM_QUIT: u32 = 0x0400 + 3;

pub fn create_tray(tx: mpsc::Sender<TrayEvent>, quit_flag: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

            let class_name: Vec<u16> = "CGuardianTray\0".encode_utf16().collect();

            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(std::ptr::null()),
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };

            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                0, class_name.as_ptr(), std::ptr::null(), 0,
                0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()), std::ptr::null(),
            );

            if hwnd.is_null() { return; }

            let boxed = Box::new((tx, quit_flag));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(boxed) as isize);

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, hwnd, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut (mpsc::Sender<TrayEvent>, Arc<AtomicBool>);
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
            }
        }
    });
}

unsafe extern "system" fn wnd_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    match msg {
        WM_TRAYICON => {
            let lparam = lparam as u32;
            if lparam == WM_LBUTTONUP {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut (mpsc::Sender<TrayEvent>, Arc<AtomicBool>);
                if !ptr.is_null() {
                    let _ = (*ptr).0.send(TrayEvent::Show);
                }
            } else if lparam == WM_RBUTTONUP {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut (mpsc::Sender<TrayEvent>, Arc<AtomicBool>);
                if !ptr.is_null() {
                    let hmenu = CreatePopupMenu();
                    let show_text: Vec<u16> = "\u{663e}\u{793a}\u{7a97}\u{53e3}\0".encode_utf16().collect(); // 显示窗口
                    let quit_text: Vec<u16> = "\u{9000}\u{51fa}\0".encode_utf16().collect(); // 退出
                    AppendMenuW(hmenu, MF_STRING, WM_SHOW as usize, show_text.as_ptr());
                    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
                    AppendMenuW(hmenu, MF_STRING, WM_QUIT as usize, quit_text.as_ptr());
                    let mut pt = std::mem::zeroed();
                    GetCursorPos(&mut pt);
                    SetForegroundWindow(hwnd);
                    let cmd = TrackPopupMenu(hmenu, TPM_RETURNCMD, pt.x, pt.y, 0, hwnd, std::ptr::null());
                    DestroyMenu(hmenu);
                    match cmd as u32 {
                        WM_SHOW => { let _ = (*ptr).0.send(TrayEvent::Show); }
                        WM_QUIT => { let _ = (*ptr).0.send(TrayEvent::Quit); }
                        _ => {}
                    }
                }
            }
            0
        }
        WM_SHOW => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut (mpsc::Sender<TrayEvent>, Arc<AtomicBool>);
            if !ptr.is_null() {
                let _ = (*ptr).0.send(TrayEvent::Show);
            }
            0
        }
        WM_QUIT => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut (mpsc::Sender<TrayEvent>, Arc<AtomicBool>);
            if !ptr.is_null() {
                let _ = (*ptr).0.send(TrayEvent::Quit);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
