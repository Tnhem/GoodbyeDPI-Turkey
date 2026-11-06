//! Thin RAII wrappers over the Service Control Manager API.

use std::ptr::null;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{GetLastError, ERROR_SERVICE_ALREADY_RUNNING};
use windows_sys::Win32::System::Services::*;

use crate::common::wide;

struct Handle(SC_HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseServiceHandle(self.0) };
    }
}

pub struct Service {
    handle: Handle,
    _manager: Handle,
}

fn open_manager(access: u32) -> Result<Handle, u32> {
    let h = unsafe { OpenSCManagerW(null(), null(), access) };
    if h.is_null() {
        Err(unsafe { GetLastError() })
    } else {
        Ok(Handle(h))
    }
}

pub fn open(name: &str, access: u32) -> Result<Service, u32> {
    let manager = open_manager(SC_MANAGER_CONNECT)?;
    let name = wide(name);
    let h = unsafe { OpenServiceW(manager.0, name.as_ptr(), access) };
    if h.is_null() {
        Err(unsafe { GetLastError() })
    } else {
        Ok(Service { handle: Handle(h), _manager: manager })
    }
}

pub fn exists(name: &str) -> bool {
    open(name, SERVICE_QUERY_STATUS).is_ok()
}

/// Creates the service, or repoints an existing one at `bin_path`.
pub fn create_or_update(name: &str, display: &str, bin_path: &str, description: &str) -> Result<Service, u32> {
    let manager = open_manager(SC_MANAGER_ALL_ACCESS)?;
    let (name_w, display_w, bin_w) = (wide(name), wide(display), wide(bin_path));

    let mut h = unsafe { OpenServiceW(manager.0, name_w.as_ptr(), SERVICE_ALL_ACCESS) };
    if h.is_null() {
        h = unsafe {
            CreateServiceW(
                manager.0,
                name_w.as_ptr(),
                display_w.as_ptr(),
                SERVICE_ALL_ACCESS,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_AUTO_START,
                SERVICE_ERROR_NORMAL,
                bin_w.as_ptr(),
                null(),
                std::ptr::null_mut(),
                null(),
                null(), // LocalSystem
                null(),
            )
        };
        if h.is_null() {
            return Err(unsafe { GetLastError() });
        }
    } else {
        let ok = unsafe {
            ChangeServiceConfigW(
                h,
                SERVICE_NO_CHANGE,
                SERVICE_AUTO_START,
                SERVICE_NO_CHANGE,
                bin_w.as_ptr(),
                null(),
                std::ptr::null_mut(),
                null(),
                null(),
                null(),
                display_w.as_ptr(),
            )
        };
        if ok == 0 {
            let err = unsafe { GetLastError() };
            unsafe { CloseServiceHandle(h) };
            return Err(err);
        }
    }

    let mut desc_w = wide(description);
    let desc = SERVICE_DESCRIPTIONW { lpDescription: desc_w.as_mut_ptr() };
    unsafe { ChangeServiceConfig2W(h, SERVICE_CONFIG_DESCRIPTION, &desc as *const _ as *const _) };

    Ok(Service { handle: Handle(h), _manager: manager })
}

impl Service {
    pub fn state(&self) -> Option<u32> {
        let mut st: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { QueryServiceStatus(self.handle.0, &mut st) } != 0 {
            Some(st.dwCurrentState)
        } else {
            None
        }
    }

    pub fn is_running(&self) -> bool {
        self.state() == Some(SERVICE_RUNNING)
    }

    pub fn control(&self, code: u32) -> Result<(), u32> {
        let mut st: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { ControlService(self.handle.0, code, &mut st) } != 0 {
            Ok(())
        } else {
            Err(unsafe { GetLastError() })
        }
    }

    pub fn start(&self) -> Result<(), u32> {
        if unsafe { StartServiceW(self.handle.0, 0, null()) } != 0 {
            return Ok(());
        }
        match unsafe { GetLastError() } {
            ERROR_SERVICE_ALREADY_RUNNING => Ok(()),
            err => Err(err),
        }
    }

    pub fn wait_for(&self, state: u32, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            match self.state() {
                Some(s) if s == state => return true,
                None => return false,
                _ if Instant::now() >= deadline => return false,
                _ => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    }

    pub fn stop_and_wait(&self) {
        if self.state() != Some(SERVICE_STOPPED) {
            let _ = self.control(SERVICE_CONTROL_STOP);
            self.wait_for(SERVICE_STOPPED, Duration::from_secs(15));
        }
    }

    pub fn delete(&self) -> Result<(), u32> {
        if unsafe { DeleteService(self.handle.0) } != 0 {
            Ok(())
        } else {
            Err(unsafe { GetLastError() })
        }
    }
}
