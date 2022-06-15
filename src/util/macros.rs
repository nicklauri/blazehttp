#[macro_export]
macro_rules! err {
    ($cond:expr, $e:expr) => {
        if $cond {
            err!($e)
        }
    };

    ($e:expr) => {
        return Err($e)
    };
}

#[macro_export]
macro_rules! ok {
    () => {
        return Ok(());
    };

    ($e:expr) => {
        return Ok($e)
    };
}

#[macro_export]
macro_rules! try_break {
    ($e:expr) => {
        match $e {
            std::convert::ControlFlow::Continue(()) => {}
            std::convert::ControlFlow::Break(result) => {
                result?;
                break;
            }
        }
    };
}

pub use err;
pub use ok;
pub use try_break;
