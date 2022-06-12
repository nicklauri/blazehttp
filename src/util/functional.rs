use crate::error::BlazeError;

#[inline]
pub fn compose<F, G, A, B, C>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
{
    move |a| g(f(a))
}

/// Tranform Result<T, E> to Result<T, BlazeError>
/// Note: `f` takes an arg type `I` and return `Result<T, E>` and is transformed into Result<T, BlazeError>
#[inline]
pub fn transform_error<F, I, T, E>(f: F) -> impl Fn(I) -> Result<T, BlazeError>
where
    F: Fn(I) -> Result<T, E>,
    BlazeError: From<E>,
{
    compose(f, BlazeError::transform_result)
}

/// Transform a closure from accepting two arguments into accepting one tuple argument.
#[inline]
pub fn accept_tuple<F, T, U, R>(f: F) -> impl Fn((T, U)) -> R
where
    F: Fn(T, U) -> R,
{
    move |(t, u)| f(t, u)
}

pub mod result {
    use crate::{error::BlazeError, util};

    pub fn and_then_tuple<F1, F2, T, TOk, TErr, U, UOk, UErr>(f1: F1, f2: F2) -> impl Fn((T, U)) -> Result<(TOk, UOk), BlazeError>
    where
        F1: Fn(T) -> Result<TOk, TErr>,
        F2: Fn(U) -> Result<UOk, UErr>,
        BlazeError: From<UErr> + From<TErr>,
    {
        util::accept_tuple(util::result::and_then_combine(util::transform_error(f1), util::transform_error(f2)))
    }

    pub fn and_then_combine<F, G, T, TOk, U, UOk, E>(f: F, g: G) -> impl Fn(T, U) -> Result<(TOk, UOk), E>
    where
        F: Fn(T) -> Result<TOk, E>,
        G: Fn(U) -> Result<UOk, E>,
    {
        // This is too complicated to transform into functions.
        move |t, u| f(t).and_then(|t| g(u).and_then(|u| Ok((t, u))))
    }

    #[allow(dead_code)]
    pub fn and_then<F, G, T, U, R, E>(f: F, g: G) -> impl FnOnce(T) -> Result<R, E>
    where
        F: FnOnce(T) -> Result<U, E>,
        G: FnOnce(U) -> Result<R, E>,
    {
        move |t| Result::and_then(f(t), g)
    }

    #[allow(dead_code)]
    pub fn map_err<F, T, E1, E2>(f: F) -> impl FnMut(Result<T, E1>) -> Result<T, E2>
    where
        F: Fn(E1) -> E2,
    {
        move |res| res.map_err(&f)
    }

    #[allow(dead_code)]
    pub fn map_ok<F, T, U, E>(f: F) -> impl FnMut(Result<T, E>) -> Result<U, E>
    where
        F: Fn(T) -> U,
    {
        move |res| res.map(&f)
    }
}
