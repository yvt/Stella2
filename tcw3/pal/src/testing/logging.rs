use log::{logger, set_boxed_logger, Level, Log, Metadata, Record, SetLoggerError};
use std::cell::RefCell;

/// The logger that fowards log messages to `with_testing_wm`'s calling thread
/// so that they can be captured by Rust's test runner.
///
/// Rust's test runner captures test output by redirecting the target of
/// standard output macros such as `println!` via an internal interface. This
/// redirection only works if they are called from the thread where `#[test]`
/// functions are called. Otherwise, they would just output to the proceess's
/// real standard output/error.
///
/// When `with_testing_wm` is being used, the inner code always runs on a
/// different thread that the testing backend set up as its own “main thread.”
/// This means that when you write a log message from the inner code block of
/// `with_testing_wm`, it never gets captured by the test runner and clutters
/// your terminal.
///
/// While we can't do anything about `std::println!`, we can improve the
/// situation at least for `log` crate. This type `Logger` wraps a given
/// `impl Log` and makes sure all log messages produced in the inner code block
/// are outputted from the thread where `with_testing_wm` is called.
///
/// # Usage
///
/// Wrap an existing logger (like `env_logger::Logger`) with `Logger` and call
/// the `try_init` method like the following example:
///
///     use tcw3_pal::testing::{Logger, run_test};
///     use log::warn;
///
///     let inner = env_logger::builder().is_test(true).build();
///     let max_level = inner.filter();
///     if Logger::new(Box::new(inner)).try_init().is_ok() {
///         log::set_max_level(max_level);
///     }
///
///     run_test(|_| {
///         warn!("this message shouldn't be displayed to the screen");
///     });
///
/// # Restrictions
///
///  - To send log messages (of type `log::Record`) between threads, they are
///    converted to a different form. This is lossy and incurs a moderate
///    runtime overhead.
///
pub struct Logger {
    inner: Box<dyn Log>,
}

impl Logger {
    pub fn new(inner: Box<dyn Log>) -> Self {
        Self { inner }
    }

    /// Set the global logger to `self`.
    pub fn try_init(self) -> Result<(), SetLoggerError> {
        set_boxed_logger(Box::new(self))
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        let ok = try_delegate_with(|| LoggerEvent {
            inner: LoggerEventInner::Record(RecordSend::new(record)),
        });

        if !ok {
            self.inner.log(record);
        }
    }

    fn flush(&self) {
        let ok = try_delegate_with(|| LoggerEvent {
            inner: LoggerEventInner::Flush,
        });

        if !ok {
            self.inner.flush();
        }
    }
}

// ============================================================================

/// The function called to send a logging event to another thread.
///
/// Returns `false` if it can't process events anymore, for example, because the
/// receiver hung up.
type LogDelegate = Box<dyn Fn(LoggerEvent) -> bool>;

thread_local! {
    static DELEGATE: RefCell<Option<LogDelegate>> = RefCell::new(None);
}

/// Set the function to be called for logging events produced by the current
/// thread.
pub(super) fn set_log_delegate(func: LogDelegate) {
    DELEGATE.with(|dlg| {
        *dlg.borrow_mut() = Some(func);
    });
}

fn try_delegate_with(evt_src: impl FnOnce() -> LoggerEvent) -> bool {
    DELEGATE.with(|dlg| {
        let mut dlg = dlg.borrow_mut();
        let dlg_cell = &mut *dlg;

        if let Some(dlg) = dlg_cell {
            if dlg(evt_src()) {
                true
            } else {
                // This `LogDelegate` can't process events anymore
                *dlg_cell = None;
                false
            }
        } else {
            false
        }
    })
}

// ============================================================================

/// An event packet to be exchanged between threads for `Logger` to work.
#[derive(Debug)]
pub(super) struct LoggerEvent {
    inner: LoggerEventInner,
}

#[derive(Debug)]
enum LoggerEventInner {
    Record(RecordSend),
    Flush,
}

impl LoggerEvent {
    /// Use the global logger to process this event.
    pub(super) fn process(self) {
        self.process_with_logger(logger());
    }

    fn process_with_logger(self, logger: &dyn Log) {
        match self.inner {
            LoggerEventInner::Record(rec) => {
                rec.with_record(|rec| logger.log(rec));
            }
            LoggerEventInner::Flush => {
                logger.flush();
            }
        }
    }
}

// ============================================================================

/// A `Send`-able, `'static` type representing a `log::Record`.
#[derive(Debug)]
struct RecordSend {
    target: String,
    level: Level,
    args: String,
    module_path: Option<String>,
    file: Option<String>,
    line: Option<u32>,
}

impl RecordSend {
    fn new(rec: &Record<'_>) -> Self {
        Self {
            target: rec.target().to_owned(),
            level: rec.level(),
            args: format!("{}", rec.args()),
            module_path: rec.module_path().map(str::to_string),
            file: rec.file().map(str::to_string),
            line: rec.line(),
        }
    }

    fn with_record<R>(&self, f: impl FnOnce(&Record<'_>) -> R) -> R {
        f(&Record::builder()
            .target(&self.target)
            .level(self.level)
            .args(format_args!("{}", self.args))
            .module_path(self.module_path.as_deref())
            .file(self.file.as_deref())
            .line(self.line)
            .build())
    }
}
