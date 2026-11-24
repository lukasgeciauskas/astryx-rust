//! Hot-reload diagnostics (Windows): when the gallery's main thread
//! overflows its stack, write the innermost frames, symbolized, to
//! `%TEMP%/astryx-stack-overflow.txt` before the process dies. Used to chase
//! the crash after a hot patch (docs/KNOWN_ISSUES.md).

use std::ffi::c_void;

const EXCEPTION_STACK_OVERFLOW: i32 = 0xC000_00FD_u32 as i32;
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
const FRAMES: usize = 160;

#[repr(C)]
struct ExceptionRecord {
    code: i32,
}

#[repr(C)]
struct ExceptionPointers {
    record: *const ExceptionRecord,
    context: *const c_void,
}

unsafe extern "system" {
    fn AddVectoredExceptionHandler(
        first: u32,
        handler: unsafe extern "system" fn(*const ExceptionPointers) -> i32,
    ) -> *mut c_void;
    fn SetThreadStackGuarantee(size: *mut u32) -> i32;
}

unsafe extern "system" fn on_exception(info: *const ExceptionPointers) -> i32 {
    let code = unsafe { (*(*info).record).code };
    if code != EXCEPTION_STACK_OVERFLOW {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let mut ips = Vec::with_capacity(FRAMES);
    unsafe {
        backtrace::trace_unsynchronized(|frame| {
            ips.push(frame.ip());
            ips.len() < FRAMES
        });
    }
    let mut out = String::new();
    for (i, ip) in ips.iter().enumerate() {
        let mut line = format!("{i:4} {:?}", ip);
        unsafe {
            backtrace::resolve_unsynchronized(*ip, |symbol| {
                if let Some(name) = symbol.name() {
                    line.push_str(&format!(" {name}"));
                }
                if let (Some(file), Some(n)) = (symbol.filename(), symbol.lineno()) {
                    line.push_str(&format!(" ({}:{n})", file.display()));
                }
            });
        }
        out.push_str(&line);
        out.push('\n');
    }
    let _ = std::fs::write(std::env::temp_dir().join("astryx-stack-overflow.txt"), out);
    EXCEPTION_CONTINUE_SEARCH
}

/// Reserves stack for the handler on this thread and installs it first.
pub fn install() {
    unsafe {
        let mut guarantee: u32 = 1024 * 1024;
        SetThreadStackGuarantee(&mut guarantee);
        AddVectoredExceptionHandler(1, on_exception);
    }
}
