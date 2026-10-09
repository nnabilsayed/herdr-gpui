//! A Windows GUI-subsystem executable starts with no console, so `--help`,
//! `--build-info` and errors would print nowhere. When a command-line argument
//! is given and a standard stream is not already redirected, this borrows the
//! launching terminal's console. A plain launch stays console-free.

#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn attach_parent() {
    use std::{fs::OpenOptions, os::windows::io::IntoRawHandle};
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::Console::{
            ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE,
            STD_OUTPUT_HANDLE, SetStdHandle,
        },
    };

    if std::env::args_os().len() < 2 {
        return;
    }
    let missing = |slot| {
        // SAFETY: GetStdHandle only reads this process's standard handle table.
        let handle = unsafe { GetStdHandle(slot) };
        handle.is_null() || handle == INVALID_HANDLE_VALUE
    };
    let slots: Vec<_> = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
        .into_iter()
        .filter(|slot| missing(*slot))
        .collect();
    // SAFETY: AttachConsole takes no pointers; failure (no parent console)
    // leaves the process as it was.
    if slots.is_empty() || unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        return;
    }
    for slot in slots {
        let Ok(console) = OpenOptions::new().write(true).open("CONOUT$") else {
            continue;
        };
        // SAFETY: the raw handle is a live console output handle. It is
        // deliberately leaked: the standard handle table owns it from here on.
        unsafe {
            SetStdHandle(slot, console.into_raw_handle());
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn attach_parent() {}
