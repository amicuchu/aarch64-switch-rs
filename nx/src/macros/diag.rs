#![macro_use]

#[macro_export]
macro_rules! diag_assert {
    ($mode:expr, $cond:expr) => {
        if !$cond {
            $crate::diag::assert::assert($mode, $crate::result::ResultCode::from::<$crate::diag::assert::ResultAssertionFailed>());
        }
    };
}

#[macro_export]
macro_rules! diag_log {
    ($logger:ty { $severity:expr, $verbosity:expr } => $msg:literal) => {
        {
            fn f() {}
            fn type_name_of<T>(_: T) -> &'static str {
                core::any::type_name::<T>()
            }
            let name = type_name_of(f);
            // `3` is the length of the `::f`.
            let fn_name = &name[..name.len() - 3];

            let mut logger = <$logger>::new();
            let metadata = $crate::diag::log::LogMetadata::new($severity, $verbosity, alloc::string::String::from($msg), file!(), fn_name, line!());
            logger.log(&metadata);
        }
    };
    ($logger:ty { $severity:expr, $verbosity:expr } => $fmt:literal, $( $params:expr ),*) => {
        {
            fn f() {}
            fn type_name_of<T>(_: T) -> &'static str {
                core::any::type_name::<T>()
            }
            let name = type_name_of(f);
            // `3` is the length of the `::f`.
            let fn_name = &name[..name.len() - 3];

            let msg = format!($fmt, $( $params, )*);
            let mut logger = <$logger>::new();
            let metadata = $crate::diag::log::LogMetadata::new($severity, $verbosity, msg, file!(), fn_name, line!());
            logger.log(&metadata);
        }
    };
}

#[macro_export]
macro_rules! diag_log_assert {
    ($logger:ty, $assert_mode:expr => $cond:expr) => {
        {
            fn f() {}
            fn type_name_of<T>(_: T) -> &'static str {
                core::any::type_name::<T>()
            }
            let name = type_name_of(f);
            // `3` is the length of the `::f`.
            let fn_name = &name[..name.len() - 3];

            if $cond {
                let msg = format!("Assertion suceeded -> {}", stringify!($cond));

                let mut logger = <$logger>::new();
                let metadata = $crate::diag::log::LogMetadata::new($crate::diag::log::LogSeverity::Info, false, msg, file!(), fn_name, line!());
                logger.log(&metadata);
            }
            else {
                let msg = format!("Assertion failed ({}) -> {}", stringify!($assert_mode), stringify!($cond));

                let mut logger = <$logger>::new();
                let metadata = $crate::diag::log::LogMetadata::new($crate::diag::log::LogSeverity::Fatal, false, msg, file!(), fn_name, line!());
                logger.log(&metadata);

                $crate::diag::assert::assert($assert_mode, $crate::result::ResultCode::new(0x9BA1));
            }
        }
    };
}