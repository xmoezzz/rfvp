#![macro_use]

#[macro_export]
macro_rules! diag_assert {
    ($desired_abort_level:expr, $cond_expr:expr) => {{
        let cond = $cond_expr;
        if !cond {
            $crate::diag::abort::abort(
                $desired_abort_level,
                $crate::diag::rc::ResultAssertionFailed::make(),
            );
        }
    }};
}

// TODO: switch to a log system without having to reopen loggers for every log? global logger object(s) like N's log observers?

#[macro_export]
macro_rules! diag_log {
    ($logger:ty { $severity:expr, $verbosity:expr } => $msg:literal) => {{
        let metadata = $crate::diag::log::LogMetadata::new($severity, $verbosity, alloc::string::String::from($msg), file!(), $crate::cur_fn_name!(), line!());
        $crate::diag::log::log_with::<$logger>(&metadata);
    }};

    ($logger:ty { $severity:expr, $verbosity:expr } => $msg:expr) => {{
        let metadata = $crate::diag::log::LogMetadata::new($severity, $verbosity, $msg, file!(), $crate::cur_fn_name!(), line!());
        $crate::diag::log::log_with::<$logger>(&metadata);
    }};

    ($logger:ty { $severity:expr, $verbosity:expr } => $fmt:literal, $( $params:expr ),*) => {{
        let msg = format!($fmt, $( $params, )*);
        let metadata = $crate::diag::log::LogMetadata::new($severity, $verbosity, msg, file!(), $crate::cur_fn_name!(), line!());
        $crate::diag::log::log_with::<$logger>(&metadata);
    }};
}

#[macro_export]
macro_rules! diag_log_assert {
    ($logger:ty, $desired_abort_level:expr => $cond_expr:expr) => {{
        let cond = $cond_expr;
        if cond {
            $crate::diag_log!($logger { $crate::diag::log::LogSeverity::Info, true } => "Assertion succeeded: '{}'", stringify!($cond_expr));
        }
        else {
            $crate::diag_log!($logger { $crate::diag::log::LogSeverity::Fatal, true } => "Assertion failed: '{}' - proceeding to abort through {:?}...", stringify!($cond_expr), $desired_abort_level);

            $crate::diag::abort::abort($desired_abort_level, $crate::diag::rc::ResultAssertionFailed::make());
        }
    }};
}

#[macro_export]
macro_rules! diag_result_code_log_assert {
    ($logger:ty, $desired_abort_level:expr => $rc_expr:expr) => {{
        let rc = $rc_expr;
        if rc.is_success() {
            $crate::diag_log!($logger { $crate::diag::log::LogSeverity::Info, true } => "Result assertion succeeded: '{}'", stringify!($rc_expr));
        }
        else {
            $crate::diag_log!($logger { $crate::diag::log::LogSeverity::Fatal, true } => "Result assertion failed: '{0}' was {1} ({1:?}) - proceeding to abort through {2:?}...", stringify!($rc_expr), rc, $desired_abort_level);

            $crate::diag::abort::abort($desired_abort_level, rc);
        }
    }};
}

#[macro_export]
macro_rules! diag_result_log_assert {
    ($logger:ty, $desired_abort_level:expr => $rc_expr:expr) => {{
        let ret_rc = $rc_expr;

        match ret_rc {
            Ok(t) => {
                $crate::diag_log!($logger { $crate::diag::log::LogSeverity::Info, true } => "Result assertion succeeded: '{}'\n", stringify!($rc_expr));

                t
            },
            Err(rc) => {
                $crate::diag_log!($logger { $crate::diag::log::LogSeverity::Fatal, true } => "Result assertion failed: '{0}' was {1} ({1:?}) - proceeding to abort through {2:?}...", stringify!($rc_expr), rc, $desired_abort_level);

                $crate::diag::abort::abort($desired_abort_level, rc);
            }
        }
    }};
}
