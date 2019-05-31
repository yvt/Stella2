use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(target_os = "macos")] {
        // We rely on CrashReporter at the moment. It displays a crash dialog
        // with a plenty of information, which is "automatically sent to Apple"
        // (based on the user's privacy settings). Unfortunately, the crash
        // reports are available to the developers only if the app is
        // distributed via Mac App Store. Still, the user can manually
        // copy-and-paste and submit the contents of the crash dialog.
        use std::{sync::atomic::{AtomicPtr, Ordering}, os::raw::c_char, panic};

        pub fn init() {
            panic::set_hook(Box::new(panic_handler));
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
                write!(msg, "\nat: '{}' at line {}", loc.file(), loc.line()).unwrap();
            }

            msg.push('\0');

            // Tell CrashReporter the panic's details.
            __crashreporter_info__.store(msg.as_ptr() as _, Ordering::Relaxed);

            // Crash the application
            std::process::abort();
        }
    } else {
        pub fn init() {}
    }
}
