use cfg_if::cfg_if;

#[allow(dead_code)]
fn should_install_panic_hook() -> bool {
    // Install a panic hook only when the environment variable is not set
    // to aid debugging
    if let Some(value) = std::env::var_os("RUST_BACKTRACE") {
        value.len() == 0
    } else {
        false
    }
}

cfg_if! {
    if #[cfg(target_os = "macos")] {
        // We rely on CrashReporter at the moment. It displays a crash dialog
        // with a plenty of information, which is "automatically sent to Apple"
        // (based on the user's privacy settings). Unfortunately, the crash
        // reports are available to the developers only if the app is
        // distributed via Mac App Store. Still, the user can manually
        // copy-and-paste and submit the contents of the crash dialog.
        use std::{sync::atomic::{AtomicPtr, Ordering}, os::raw::c_char, panic};
        use cocoa::{foundation::NSString, base::{id, nil}};
        use objc::{msg_send, sel, sel_impl};

        pub fn init() {
            if should_install_panic_hook() {
                panic::set_hook(Box::new(panic_handler));
            }
        }

        fn nslog(text: &str) {
            extern {
                fn NSLog(format: id, ...);
            }
            unsafe {
                let format = NSString::alloc(nil).init_str("%@");
                let text = NSString::alloc(nil).init_str(text);
                NSLog(format, text);
                let () = msg_send![format, release];
                let () = msg_send![text, release];
            }
        }

        // macOS's CrashReporter looks for symbols named like this, and outputs
        // their contents to a crash report
        #[no_mangle]
        #[allow(non_upper_case_globals)]
        static __crashreporter_info__: AtomicPtr<c_char> = AtomicPtr::new(std::ptr::null_mut());
        fn panic_handler(panic_info: &panic::PanicInfo<'_>) {
            let payload;

            if let Some(s) = panic_info.payload().downcast_ref::<&'static str>() {
                payload = *s;
            } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                payload = &*s;
            } else {
                payload = "Box<Any>";
            }

            let mut msg = format!("panic: {:?}", payload);

            if let Some(loc) = panic_info.location() {
                use std::fmt::Write;
                write!(msg, " location: '{}' at line {}", loc.file(), loc.line()).unwrap();
            }

            msg.push('\0');

            // Tell CrashReporter the panic's details.
            __crashreporter_info__.store(msg.as_ptr() as _, Ordering::Relaxed);

            // Output to Apple System Log as well.
            // TODO: Add a logging system to the application
            nslog(&msg);
            nslog("Escalating panic to abort. Run with the environment variable \
                   'RUST_BACKTRACE' to disable this behavior.");

            // Crash the application
            std::process::abort();
        }
    } else {
        pub fn init() {}
    }
}
