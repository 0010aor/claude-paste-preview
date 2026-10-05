use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    PAINTSTRUCT,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, LoadCursorW, PostQuitMessage,
    RegisterClassW, ShowWindow, SystemParametersInfoW, TranslateMessage, IDC_HAND, MSG,
    SPI_GETWORKAREA, SW_SHOWNOACTIVATE, WM_DESTROY, WM_LBUTTONDOWN, WM_PAINT, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use super::{bottom_right_origin, Monitor, Rgb};
use crate::Result;

struct Bgrx {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

static PICTURE: OnceLock<Bgrx> = OnceLock::new();

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn primary_work_area() -> Monitor {
    let mut area = RECT {
        left: 0,
        top: 0,
        right: 1280,
        bottom: 720,
    };
    unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut area as *mut RECT).cast(), 0) };
    Monitor {
        x: area.left,
        y: area.top,
        width: area.right - area.left,
        height: area.bottom - area.top,
    }
}

fn to_bgrx(picture: &Rgb) -> Bgrx {
    let pixels = picture
        .pixels
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|[red, green, blue]| [*blue, *green, *red, 0])
        .collect();
    Bgrx {
        width: picture.width as i32,
        height: picture.height as i32,
        pixels,
    }
}

fn paint(window: HWND) {
    let Some(picture) = PICTURE.get() else { return };
    let mut header = unsafe { std::mem::zeroed::<BITMAPINFO>() };
    header.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: picture.width,
        biHeight: -picture.height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..unsafe { std::mem::zeroed() }
    };
    unsafe {
        let mut paint_info = std::mem::zeroed::<PAINTSTRUCT>();
        let context = BeginPaint(window, &mut paint_info);
        SetDIBitsToDevice(
            context,
            0,
            0,
            picture.width as u32,
            picture.height as u32,
            0,
            0,
            0,
            picture.height as u32,
            picture.pixels.as_ptr().cast(),
            &header,
            DIB_RGB_COLORS,
        );
        EndPaint(window, &paint_info);
    }
}

unsafe extern "system" fn window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            paint(window);
            0
        }
        WM_LBUTTONDOWN | WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn parent_process_id() -> Option<u32> {
    let own = unsafe { GetCurrentProcessId() };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() {
        return None;
    }
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    let mut parent = None;
    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while has_entry {
        if entry.th32ProcessID == own {
            parent = Some(entry.th32ParentProcessID);
            break;
        }
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    parent
}

pub(super) fn exit_when_parent_exits() {
    let Some(parent) = parent_process_id() else {
        return;
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parent) } as usize;
    if handle == 0 {
        return;
    }
    std::thread::spawn(move || {
        unsafe { WaitForSingleObject(handle as _, INFINITE) };
        std::process::exit(0);
    });
}

pub(super) fn show(compose: impl FnOnce(&Monitor) -> Result<Rgb>) -> Result<()> {
    let monitor = primary_work_area();
    let picture = compose(&monitor)?;
    let (x, y) = bottom_right_origin(&monitor, picture.width, picture.height);
    let (width, height) = (picture.width as i32, picture.height as i32);
    PICTURE
        .set(to_bgrx(&picture))
        .map_err(|_| "the preview is already showing")?;

    let class_name = wide("ClaudePastePreview");
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_procedure),
            hInstance: instance,
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return Err("could not register the preview window class".into());
        }
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            wide("Pasted images").as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if window.is_null() {
            return Err("could not create the preview window".into());
        }
        ShowWindow(window, SW_SHOWNOACTIVATE);
        let mut message = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}
