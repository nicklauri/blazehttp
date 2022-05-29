use std::error::Error as StdError;

use crate::error::BlazeError;

pub fn compose<F, G, A, B, C>(f: F, g: G) -> impl FnOnce(A) -> C
where
    F: FnOnce(A) -> B,
    G: FnOnce(B) -> C,
{
    move |a| g(f(a))
}

/// Tranform Result<T, E> to Result<T, BlazeError>
/// Note: `f` takes an arg type `I` and return `Result<T, E>` and is transformed into Result<T, BlazeError>
pub fn transform_error<F, I, T, E>(f: F) -> impl FnOnce(I) -> Result<T, BlazeError>
where
    F: FnOnce(I) -> Result<T, E>,
    BlazeError: From<E>,
{
    compose(f, BlazeError::transform_result)
}
