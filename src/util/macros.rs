#[macro_export]
macro_rules! err {
    ($cond:expr, $e:expr) => {
        if $cond {
            err!($e)
        }
    };

    ($e:expr) => {
        return Err($e);
    };
}

#[macro_export]
macro_rules! ok {
    () => {
        return Ok(());
    };

    ($e:expr) => {
        return Ok($e);
    };
}

pub use err;
pub use ok;
