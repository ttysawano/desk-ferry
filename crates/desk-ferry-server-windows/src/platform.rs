use std::time::Duration;

use desk_ferry_common::{DeskFerryError, Result};

use crate::display::MonitorInfo;
use crate::input::{InputEngine, InputMode, InputProcessResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputHookConfig {
    pub server_host: String,
    pub initial_active_host: Option<String>,
    pub mode: InputMode,
    pub duration: Option<Duration>,
}

#[cfg(windows)]
mod windows {
    use std::{
        ffi::OsString,
        mem,
        os::windows::ffi::OsStringExt,
        ptr,
        sync::{mpsc, Mutex, OnceLock},
        time::Instant,
    };

    use super::*;
    use crate::display::Rect;
    use crate::input::{RawButtonState, RawInputEvent, RawKeyState};
    use desk_ferry_common::protocol::MouseButton;

    type Bool = i32;
    type Dword = u32;
    type Hdc = isize;
    type Hmonitor = isize;
    type Hhook = isize;
    type Hinstance = isize;
    type Lparam = isize;
    type Lresult = isize;
    type Uint = u32;
    type Wparam = usize;

    const CCHDEVICENAME: usize = 32;
    const MONITORINFOF_PRIMARY: Dword = 0x00000001;
    const HC_ACTION: i32 = 0;
    const WH_MOUSE_LL: i32 = 14;
    const WH_KEYBOARD_LL: i32 = 13;
    const PM_REMOVE: Uint = 0x0001;
    const WM_MOUSEMOVE: Wparam = 0x0200;
    const WM_LBUTTONDOWN: Wparam = 0x0201;
    const WM_LBUTTONUP: Wparam = 0x0202;
    const WM_RBUTTONDOWN: Wparam = 0x0204;
    const WM_RBUTTONUP: Wparam = 0x0205;
    const WM_MBUTTONDOWN: Wparam = 0x0207;
    const WM_MBUTTONUP: Wparam = 0x0208;
    const WM_MOUSEWHEEL: Wparam = 0x020A;
    const WM_XBUTTONDOWN: Wparam = 0x020B;
    const WM_XBUTTONUP: Wparam = 0x020C;
    const WM_KEYDOWN: Wparam = 0x0100;
    const WM_KEYUP: Wparam = 0x0101;
    const WM_SYSKEYDOWN: Wparam = 0x0104;
    const WM_SYSKEYUP: Wparam = 0x0105;
    const XBUTTON1: u16 = 0x0001;

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct WinRect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MonitorInfoExW {
        cb_size: Dword,
        rc_monitor: WinRect,
        rc_work: WinRect,
        dw_flags: Dword,
        sz_device: [u16; CCHDEVICENAME],
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: Uint,
        w_param: Wparam,
        l_param: Lparam,
        time: Dword,
        pt: Point,
        l_private: Dword,
    }

    #[repr(C)]
    struct MsLlHookStruct {
        pt: Point,
        mouse_data: Dword,
        flags: Dword,
        time: Dword,
        dw_extra_info: usize,
    }

    #[repr(C)]
    struct KbdLlHookStruct {
        vk_code: Dword,
        scan_code: Dword,
        flags: Dword,
        time: Dword,
        dw_extra_info: usize,
    }

    type MonitorEnumProc =
        Option<unsafe extern "system" fn(Hmonitor, Hdc, *mut WinRect, Lparam) -> Bool>;
    type HookProc = Option<unsafe extern "system" fn(i32, Wparam, Lparam) -> Lresult>;

    #[link(name = "user32")]
    extern "system" {
        fn EnumDisplayMonitors(
            hdc: Hdc,
            lprc_clip: *const WinRect,
            lpfn_enum: MonitorEnumProc,
            dw_data: Lparam,
        ) -> Bool;
        fn GetMonitorInfoW(h_monitor: Hmonitor, lpmi: *mut MonitorInfoExW) -> Bool;
        fn SetWindowsHookExW(
            id_hook: i32,
            lpfn: HookProc,
            hmod: Hinstance,
            dw_thread_id: Dword,
        ) -> Hhook;
        fn CallNextHookEx(hhk: Hhook, n_code: i32, w_param: Wparam, l_param: Lparam) -> Lresult;
        fn UnhookWindowsHookEx(hhk: Hhook) -> Bool;
        fn PeekMessageW(
            lp_msg: *mut Msg,
            hwnd: isize,
            w_msg_filter_min: Uint,
            w_msg_filter_max: Uint,
            w_remove_msg: Uint,
        ) -> Bool;
        fn TranslateMessage(lp_msg: *const Msg) -> Bool;
        fn DispatchMessageW(lp_msg: *const Msg) -> Lresult;
        fn Sleep(dw_milliseconds: Dword);
    }

    struct HookState {
        engine: InputEngine,
        sender: mpsc::Sender<InputProcessResult>,
        last_mouse_position: Option<(i32, i32)>,
    }

    static HOOK_STATE: OnceLock<Mutex<Option<HookState>>> = OnceLock::new();

    pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
        let mut monitors = Vec::<MonitorInfo>::new();
        let ok = unsafe {
            EnumDisplayMonitors(
                0,
                ptr::null(),
                Some(enum_monitor),
                (&mut monitors as *mut Vec<MonitorInfo>) as Lparam,
            )
        };

        if ok == 0 {
            return Err(DeskFerryError::ConfigValidation(
                "EnumDisplayMonitors failed".to_string(),
            ));
        }
        if monitors.is_empty() {
            return Err(DeskFerryError::ConfigValidation(
                "no Windows monitors found".to_string(),
            ));
        }

        Ok(monitors)
    }

    unsafe extern "system" fn enum_monitor(
        monitor: Hmonitor,
        _hdc: Hdc,
        _rect: *mut WinRect,
        data: Lparam,
    ) -> Bool {
        let monitors = &mut *(data as *mut Vec<MonitorInfo>);
        let mut info = MonitorInfoExW {
            cb_size: mem::size_of::<MonitorInfoExW>() as Dword,
            rc_monitor: WinRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rc_work: WinRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dw_flags: 0,
            sz_device: [0; CCHDEVICENAME],
        };

        if GetMonitorInfoW(monitor, &mut info as *mut MonitorInfoExW) == 0 {
            return 0;
        }

        monitors.push(MonitorInfo {
            name: wide_device_name(&info.sz_device),
            rect: Rect::new(
                info.rc_monitor.left,
                info.rc_monitor.top,
                info.rc_monitor.right,
                info.rc_monitor.bottom,
            ),
            primary: (info.dw_flags & MONITORINFOF_PRIMARY) != 0,
        });

        1
    }

    fn wide_device_name(value: &[u16]) -> String {
        let len = value.iter().position(|ch| *ch == 0).unwrap_or(value.len());
        OsString::from_wide(&value[..len])
            .to_string_lossy()
            .into_owned()
    }

    pub fn run_input_hooks(
        config: InputHookConfig,
        mut on_result: impl FnMut(InputProcessResult),
    ) -> Result<()> {
        let (sender, receiver) = mpsc::channel();
        let mut engine = InputEngine::new(config.server_host, config.mode);
        if let Some(active_host) = config.initial_active_host {
            let result = engine.set_active_host(active_host);
            on_result(result);
        }

        let state = HOOK_STATE.get_or_init(|| Mutex::new(None));
        {
            let mut guard = state.lock().map_err(|_| {
                DeskFerryError::ConfigValidation("input hook state lock failed".to_string())
            })?;
            *guard = Some(HookState {
                engine,
                sender,
                last_mouse_position: None,
            });
        }

        let mouse_hook = HookHandle::install(WH_MOUSE_LL, Some(low_level_mouse_proc))?;
        let keyboard_hook = HookHandle::install(WH_KEYBOARD_LL, Some(low_level_keyboard_proc))?;
        let started = Instant::now();
        let duration = config.duration;

        loop {
            pump_messages();
            while let Ok(result) = receiver.try_recv() {
                on_result(result);
            }
            if duration.is_some_and(|duration| started.elapsed() >= duration) {
                break;
            }
            unsafe {
                Sleep(10);
            }
        }

        drop(keyboard_hook);
        drop(mouse_hook);
        if let Ok(mut guard) = state.lock() {
            *guard = None;
        }

        Ok(())
    }

    struct HookHandle(Hhook);

    impl HookHandle {
        fn install(id: i32, proc: HookProc) -> Result<Self> {
            let hook = unsafe { SetWindowsHookExW(id, proc, 0, 0) };
            if hook == 0 {
                return Err(DeskFerryError::ConfigValidation(
                    "failed to install Windows low level input hook".to_string(),
                ));
            }
            Ok(Self(hook))
        }
    }

    impl Drop for HookHandle {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe {
                    UnhookWindowsHookEx(self.0);
                }
            }
        }
    }

    fn pump_messages() {
        let mut msg = Msg {
            hwnd: 0,
            message: 0,
            w_param: 0,
            l_param: 0,
            time: 0,
            pt: Point { x: 0, y: 0 },
            l_private: 0,
        };
        loop {
            let has_message =
                unsafe { PeekMessageW(&mut msg as *mut Msg, 0, 0, 0, PM_REMOVE) } != 0;
            if !has_message {
                break;
            }
            unsafe {
                TranslateMessage(&msg as *const Msg);
                DispatchMessageW(&msg as *const Msg);
            }
        }
    }

    unsafe extern "system" fn low_level_mouse_proc(
        n_code: i32,
        w_param: Wparam,
        l_param: Lparam,
    ) -> Lresult {
        if n_code == HC_ACTION {
            if let Some(event) = mouse_event_from_hook(w_param, l_param) {
                if process_hook_event(event) {
                    return 1;
                }
            }
        }
        CallNextHookEx(0, n_code, w_param, l_param)
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        n_code: i32,
        w_param: Wparam,
        l_param: Lparam,
    ) -> Lresult {
        if n_code == HC_ACTION {
            if let Some(event) = keyboard_event_from_hook(w_param, l_param) {
                if process_hook_event(event) {
                    return 1;
                }
            }
        }
        CallNextHookEx(0, n_code, w_param, l_param)
    }

    unsafe fn mouse_event_from_hook(w_param: Wparam, l_param: Lparam) -> Option<RawInputEvent> {
        let info = &*(l_param as *const MsLlHookStruct);
        match w_param {
            WM_MOUSEMOVE => {
                let state = HOOK_STATE.get()?;
                let mut guard = state.lock().ok()?;
                let hook_state = guard.as_mut()?;
                let current = (info.pt.x, info.pt.y);
                let (dx, dy) = hook_state
                    .last_mouse_position
                    .map(|previous| (current.0 - previous.0, current.1 - previous.1))
                    .unwrap_or((0, 0));
                hook_state.last_mouse_position = Some(current);
                Some(RawInputEvent::MouseMove { dx, dy })
            }
            WM_LBUTTONDOWN => Some(mouse_button(MouseButton::Left, RawButtonState::Pressed)),
            WM_LBUTTONUP => Some(mouse_button(MouseButton::Left, RawButtonState::Released)),
            WM_RBUTTONDOWN => Some(mouse_button(MouseButton::Right, RawButtonState::Pressed)),
            WM_RBUTTONUP => Some(mouse_button(MouseButton::Right, RawButtonState::Released)),
            WM_MBUTTONDOWN => Some(mouse_button(MouseButton::Middle, RawButtonState::Pressed)),
            WM_MBUTTONUP => Some(mouse_button(MouseButton::Middle, RawButtonState::Released)),
            WM_XBUTTONDOWN => Some(mouse_button(
                x_button(info.mouse_data),
                RawButtonState::Pressed,
            )),
            WM_XBUTTONUP => Some(mouse_button(
                x_button(info.mouse_data),
                RawButtonState::Released,
            )),
            WM_MOUSEWHEEL => Some(RawInputEvent::MouseWheel {
                delta: high_word_signed(info.mouse_data) as i32,
            }),
            _ => None,
        }
    }

    unsafe fn keyboard_event_from_hook(w_param: Wparam, l_param: Lparam) -> Option<RawInputEvent> {
        let info = &*(l_param as *const KbdLlHookStruct);
        match w_param {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(RawInputEvent::Key {
                key_code: info.vk_code,
                state: RawKeyState::Pressed,
            }),
            WM_KEYUP | WM_SYSKEYUP => Some(RawInputEvent::Key {
                key_code: info.vk_code,
                state: RawKeyState::Released,
            }),
            _ => None,
        }
    }

    fn process_hook_event(event: RawInputEvent) -> bool {
        let Some(state) = HOOK_STATE.get() else {
            return false;
        };
        let Ok(mut guard) = state.lock() else {
            return false;
        };
        let Some(hook_state) = guard.as_mut() else {
            return false;
        };
        let result = hook_state.engine.handle_event(event);
        let suppress = result.suppress_input;
        let _ = hook_state.sender.send(result);
        suppress
    }

    fn mouse_button(button: MouseButton, state: RawButtonState) -> RawInputEvent {
        RawInputEvent::MouseButton { button, state }
    }

    fn x_button(mouse_data: Dword) -> MouseButton {
        if high_word(mouse_data) == XBUTTON1 {
            MouseButton::Back
        } else {
            MouseButton::Forward
        }
    }

    fn high_word(value: Dword) -> u16 {
        ((value >> 16) & 0xffff) as u16
    }

    fn high_word_signed(value: Dword) -> i16 {
        high_word(value) as i16
    }
}

#[cfg(windows)]
pub use windows::enumerate_monitors;
#[cfg(windows)]
pub use windows::run_input_hooks;

#[cfg(not(windows))]
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
    Err(DeskFerryError::ConfigValidation(
        "Windows display enumeration is only available on Windows".to_string(),
    ))
}

#[cfg(not(windows))]
pub fn run_input_hooks(
    _config: InputHookConfig,
    _on_result: impl FnMut(InputProcessResult),
) -> Result<()> {
    Err(DeskFerryError::ConfigValidation(
        "Windows input hooks are only available on Windows".to_string(),
    ))
}
