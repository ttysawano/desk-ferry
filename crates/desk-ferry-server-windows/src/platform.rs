use desk_ferry_common::{DeskFerryError, Result};

use crate::display::MonitorInfo;

#[cfg(windows)]
mod windows {
    use std::{ffi::OsString, mem, os::windows::ffi::OsStringExt, ptr};

    use super::*;
    use crate::display::Rect;

    type Bool = i32;
    type Dword = u32;
    type Hdc = isize;
    type Hmonitor = isize;
    type Lparam = isize;

    const CCHDEVICENAME: usize = 32;
    const MONITORINFOF_PRIMARY: Dword = 0x00000001;

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

    type MonitorEnumProc =
        Option<unsafe extern "system" fn(Hmonitor, Hdc, *mut WinRect, Lparam) -> Bool>;

    #[link(name = "user32")]
    extern "system" {
        fn EnumDisplayMonitors(
            hdc: Hdc,
            lprc_clip: *const WinRect,
            lpfn_enum: MonitorEnumProc,
            dw_data: Lparam,
        ) -> Bool;
        fn GetMonitorInfoW(h_monitor: Hmonitor, lpmi: *mut MonitorInfoExW) -> Bool;
    }

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
}

#[cfg(windows)]
pub use windows::enumerate_monitors;

#[cfg(not(windows))]
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
    Err(DeskFerryError::ConfigValidation(
        "Windows display enumeration is only available on Windows".to_string(),
    ))
}
