pub use zihuan_core::error::{Error, Result};

#[macro_export]
macro_rules! string_error {
    ($($arg:tt)*) => {
        $crate::error::Error::StringError(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::string_error!($($arg)*))
    };
}
