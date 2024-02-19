use alloc::{boxed::Box, string::String, sync::Arc};

/// Creates a new ad hoc error with no causal chain.
///
/// This accepts the same arguments as the `format!` macro. The error it
/// creates is just a wrapper around the string created by `format!`.
macro_rules! err {
    ($($tt:tt)*) => {{
        crate::error::Error::adhoc(alloc::format!($($tt)*))
    }}
}

pub(crate) use err;

/// An error that can occur in this crate.
///
/// The most common type of error is a result of overflow. But other errors
/// exist as well:
///
/// * Time zone database lookup failure.
/// * Configuration problem. (For example, trying to round a span with calendar
/// units without providing a relative datetime.)
/// * An I/O error as a result of trying to open a time zone database from a
/// directory via
/// [`TimeZoneDatabase::from_dir`](crate::tz::TimeZoneDatabase::from_dir).
/// * Parse errors.
///
/// # Introspection is limited
///
/// Other than implementing the [`std::error::Error`] trait when the
/// `std` feature is enabled, the [`core::fmt::Debug`] trait and the
/// [`core::fmt::Display`] trait, this error type currently provides no
/// introspection capabilities.
///
/// # Design
///
/// This crate follows the "One True God Error Type Pattern," where only one
/// error type exists for a variety of different operations. This design was
/// chosen after attempting to provide finer grained error types. But finer
/// grained error types proved difficult in the face of composition.
///
/// More about this design choice can be found in a GitHub issue
/// [about error types].
///
/// [about error types]: https://github.com/BurntSushi/jiff/issues/8
#[derive(Clone, Debug)]
pub struct Error {
    /// The internal representation of an error.
    ///
    /// This is in an `Arc` to make an `Error` cloneable. It could otherwise
    /// be automatically cloneable, but it embeds a `std::io::Error` when the
    /// `std` feature is enabled, which isn't cloneable.
    ///
    /// This also makes clones cheap. And it also make the size of error equal
    /// to one word (although a `Box` would achieve that last goal).
    inner: Arc<ErrorInner>,
}

#[derive(Debug)]
struct ErrorInner {
    kind: ErrorKind,
    cause: Option<Error>,
}

/// The underlying kind of a [`Error`].
#[derive(Debug)]
enum ErrorKind {
    /// An ad hoc error that is constructed from anything that implements
    /// the `core::fmt::Display` trait.
    ///
    /// In theory we try to avoid these, but they tend to be awfully
    /// convenient. In practice, we use them a lot, and only use a structured
    /// representation when a lot of different error cases fit neatly into a
    /// structure (like range errors).
    Adhoc(AdhocError),
    /// An error that occurs when a number is not within its allowed range.
    ///
    /// This can occur directly as a result of a number provided by the caller
    /// of a public API, or as a result of an operation on a number that
    /// results in it being out of range.
    Range(RangeError),
    /// An error that occurs when a lookup of a time zone, by name, fails
    /// because that time zone does not exist.
    TimeZoneLookup(TimeZoneLookupError),
    /// An error associated with a file path.
    ///
    /// This is generally expected to always have a cause attached to it
    /// explaining what went wrong. The error variant is just a path to make
    /// it composable with other error types.
    ///
    /// The cause is typically `Adhoc` or `IO`.
    ///
    /// When `std` is not enabled, this variant can never be constructed.
    FilePath(FilePathError),
    /// An error that occurs when interacting with the file system.
    ///
    /// This is effectively a wrapper around `std::io::Error` coupled with a
    /// `std::path::PathBuf`.
    ///
    /// When `std` is not enabled, this variant can never be constructed.
    IO(IOError),
}

impl Error {
    /// Creates a new "ad hoc" error value.
    ///
    /// An ad hoc error value is just an opaque string. In theory we should
    /// avoid creating such error values, but in practice, they are extremely
    /// convenient. And the alternative is quite brutal given the varied ways
    /// in which things in a datetime library can fail. (Especially parsing
    /// errors.)
    pub(crate) fn adhoc(
        err: impl core::fmt::Display + Send + Sync + 'static,
    ) -> Error {
        Error::from(ErrorKind::Adhoc(AdhocError(Box::new(err))))
    }

    pub(crate) fn unsigned(
        what: &'static str,
        given: impl Into<u128>,
        min: impl Into<i128>,
        max: impl Into<i128>,
    ) -> Error {
        Error::from(ErrorKind::Range(RangeError::unsigned(
            what, given, min, max,
        )))
    }

    pub(crate) fn signed(
        what: &'static str,
        given: impl Into<i128>,
        min: impl Into<i128>,
        max: impl Into<i128>,
    ) -> Error {
        Error::from(ErrorKind::Range(RangeError::signed(
            what, given, min, max,
        )))
    }

    pub(crate) fn specific(
        what: &'static str,
        given: impl Into<i128>,
    ) -> Error {
        Error::from(ErrorKind::Range(RangeError::specific(what, given)))
    }

    pub(crate) fn time_zone_lookup(name: impl Into<String>) -> Error {
        let inner = TimeZoneLookupErrorInner { name: name.into() };
        Error::from(ErrorKind::TimeZoneLookup(TimeZoneLookupError(Box::new(
            inner,
        ))))
    }

    /// A convenience constructor for building an I/O error associated with
    /// a file path.
    ///
    /// Generallly speaking, an I/O error should always be associated with some
    /// sort of context. In Jiff, it's always a file path (at time of writing).
    ///
    /// This is only available when the `std` feature is enabled.
    #[cfg(feature = "std")]
    pub(crate) fn fs(
        path: impl Into<std::path::PathBuf>,
        err: std::io::Error,
    ) -> Error {
        Error::io(err).path(path)
    }

    /// A convenience constructor for building an I/O error.
    ///
    /// This returns an error that is just a simple wrapper around the
    /// `std::io::Error` type. In general, callers should alwasys attach some
    /// kind of context to this error (like a file path).
    ///
    /// This is only available when the `std` feature is enabled.
    #[cfg(feature = "std")]
    pub(crate) fn io(err: std::io::Error) -> Error {
        Error::from(ErrorKind::IO(IOError { err }))
    }

    /// Contextualizes this error by associating the given file path with it.
    ///
    /// This is a convenience routine for calling `Error::context` with a
    /// `FilePathError`.
    ///
    /// This is only available when the `std` feature is enabled.
    #[cfg(feature = "std")]
    pub(crate) fn path(self, path: impl Into<std::path::PathBuf>) -> Error {
        let err = Error::from(ErrorKind::FilePath(FilePathError {
            path: path.into(),
        }));
        self.context(err)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let mut err = self;
        loop {
            write!(f, "{}", err.inner.kind)?;
            err = match err.inner.cause.as_ref() {
                None => break,
                Some(err) => err,
            };
            write!(f, ": ")?;
        }
        Ok(())
    }
}

impl core::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match *self {
            ErrorKind::Adhoc(ref msg) => msg.fmt(f),
            ErrorKind::Range(ref err) => err.fmt(f),
            ErrorKind::TimeZoneLookup(ref err) => err.fmt(f),
            ErrorKind::FilePath(ref err) => err.fmt(f),
            ErrorKind::IO(ref err) => err.fmt(f),
        }
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error { inner: Arc::new(ErrorInner { kind, cause: None }) }
    }
}

struct AdhocError(Box<dyn core::fmt::Display + Send + Sync + 'static>);

#[cfg(feature = "std")]
impl std::error::Error for AdhocError {}

impl core::fmt::Display for AdhocError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl core::fmt::Debug for AdhocError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

/// An error that occurs when an input value is out of bounds.
///
/// The error message produced by this type will include a name describing
/// which input was out of bounds, the value given and its minimum and maximum
/// allowed values.
#[derive(Debug)]
struct RangeError(Box<RangeErrorKind>);

#[derive(Debug)]
enum RangeErrorKind {
    Positive { what: &'static str, given: u128, min: i128, max: i128 },
    Negative { what: &'static str, given: i128, min: i128, max: i128 },
    Specific { what: &'static str, given: i128 },
}

impl RangeError {
    fn unsigned(
        what: &'static str,
        given: impl Into<u128>,
        min: impl Into<i128>,
        max: impl Into<i128>,
    ) -> RangeError {
        RangeError(Box::new(RangeErrorKind::Positive {
            what,
            given: given.into(),
            min: min.into(),
            max: max.into(),
        }))
    }

    fn signed(
        what: &'static str,
        given: impl Into<i128>,
        min: impl Into<i128>,
        max: impl Into<i128>,
    ) -> RangeError {
        RangeError(Box::new(RangeErrorKind::Negative {
            what,
            given: given.into(),
            min: min.into(),
            max: max.into(),
        }))
    }

    fn specific(what: &'static str, given: impl Into<i128>) -> RangeError {
        RangeError(Box::new(RangeErrorKind::Specific {
            what,
            given: given.into(),
        }))
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RangeError {}

impl core::fmt::Display for RangeError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match *self.0 {
            RangeErrorKind::Positive { what, given, min, max } => {
                write!(
                    f,
                    "parameter '{what}' with value {given} \
                     is not in the required range of {min}..={max}",
                )
            }
            RangeErrorKind::Negative { what, given, min, max } => {
                write!(
                    f,
                    "parameter '{what}' with value {given} \
                     is not in the required range of {min}..={max}",
                )
            }
            RangeErrorKind::Specific { what, given } => {
                write!(f, "parameter '{what}' with value {given} is illegal",)
            }
        }
    }
}

#[derive(Debug)]
struct TimeZoneLookupError(Box<TimeZoneLookupErrorInner>);

#[derive(Debug)]
struct TimeZoneLookupErrorInner {
    name: String,
}

#[cfg(feature = "std")]
impl std::error::Error for TimeZoneLookupError {}

impl core::fmt::Display for TimeZoneLookupError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "failed to find timezone '{}' in time zone database",
            self.0.name
        )
    }
}

/// A `std::io::Error`.
///
/// This type is itself always available, even when the `std` feature is not
/// enabled. When `std` is not enabled, a value of this type can never be
/// constructed.
///
/// Otherwise, this type is a simple wrapper around `std::io::Error`. Its
/// purpose is to encapsulate the conditional compilation based on the `std`
/// feature.
struct IOError {
    #[cfg(feature = "std")]
    err: std::io::Error,
}

#[cfg(feature = "std")]
impl std::error::Error for IOError {}

impl core::fmt::Display for IOError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        #[cfg(feature = "std")]
        {
            write!(f, "{}", self.err)
        }
        #[cfg(not(feature = "std"))]
        {
            write!(f, "<BUG: SHOULD NOT EXIST>")
        }
    }
}

impl core::fmt::Debug for IOError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        #[cfg(feature = "std")]
        {
            f.debug_struct("IOError").field("err", &self.err).finish()
        }
        #[cfg(not(feature = "std"))]
        {
            write!(f, "<BUG: SHOULD NOT EXIST>")
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for IOError {
    fn from(err: std::io::Error) -> IOError {
        IOError { err }
    }
}

struct FilePathError {
    #[cfg(feature = "std")]
    path: std::path::PathBuf,
}

#[cfg(feature = "std")]
impl std::error::Error for FilePathError {}

impl core::fmt::Display for FilePathError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        #[cfg(feature = "std")]
        {
            write!(f, "{}", self.path.display())
        }
        #[cfg(not(feature = "std"))]
        {
            write!(f, "<BUG: SHOULD NOT EXIST>")
        }
    }
}

impl core::fmt::Debug for FilePathError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        #[cfg(feature = "std")]
        {
            f.debug_struct("FilePathError").field("path", &self.path).finish()
        }
        #[cfg(not(feature = "std"))]
        {
            write!(f, "<BUG: SHOULD NOT EXIST>")
        }
    }
}

/// A simple trait to encapsulate automatic conversion to `Error`.
///
/// This trait basically exists to make `Error::context` work without needing
/// to rely on public `From` impls. For example, without this trait, we might
/// otherwise write `impl From<String> for Error`. But this would make it part
/// of the public API. Which... maybe we should do, but at time of writing,
/// I'm starting very conservative so that we can evolve errors in semver
/// compatible ways.
pub(crate) trait IntoError {
    fn into_error(self) -> Error;
}

impl IntoError for Error {
    fn into_error(self) -> Error {
        self
    }
}

impl IntoError for &'static str {
    fn into_error(self) -> Error {
        Error::adhoc(self)
    }
}

impl IntoError for String {
    fn into_error(self) -> Error {
        Error::adhoc(self)
    }
}

/// A trait for contextualizing error values.
///
/// This makes it easy to contextualize either `Error` or `Result<T, Error>`.
/// Specifically, in the latter case, it absolves one of the need to call
/// `map_err` everywhere one wants to add context to an error.
///
/// This trick was borrowed from `anyhow`.
pub(crate) trait ErrorContext {
    /// Contextualize the given consequent error with this (`self`) error as
    /// the cause.
    ///
    /// This is equivalent to saying that "consequent is caused by self."
    ///
    /// Note that if an `Error` is given for `kind`, then this panics if it has
    /// a cause. (Because the cause would otherwise be dropped. An error causal
    /// chain is just a linked list, not a tree.)
    fn context(self, consequent: impl IntoError) -> Self;

    /// Like `context`, but hides error construction within a closure.
    ///
    /// This is useful if the creation of the consequent error is not otherwise
    /// guarded and when error construction is potentially "costly" (i.e., it
    /// allocates). The closure avoids paying the cost of contextual error
    /// creation in the happy path.
    ///
    /// Usually this only makes sense to use on a `Result<T, Error>`, otherwise
    /// the closure is just executed immediately anyway.
    fn with_context<E: IntoError>(
        self,
        consequent: impl FnOnce() -> E,
    ) -> Self;
}

impl ErrorContext for Error {
    fn context(self, consequent: impl IntoError) -> Error {
        let mut err = consequent.into_error();
        assert!(
            err.inner.cause.is_none(),
            "cause of consequence must be `None`"
        );
        // OK because we just created this error so the Arc has one reference.
        Arc::get_mut(&mut err.inner).unwrap().cause = Some(self);
        err
    }

    fn with_context<E: IntoError>(
        self,
        consequent: impl FnOnce() -> E,
    ) -> Error {
        let mut err = consequent().into_error();
        assert!(
            err.inner.cause.is_none(),
            "cause of consequence must be `None`"
        );
        // OK because we just created this error so the Arc has one reference.
        Arc::get_mut(&mut err.inner).unwrap().cause = Some(self);
        err
    }
}

impl<T> ErrorContext for Result<T, Error> {
    fn context(self, consequent: impl IntoError) -> Result<T, Error> {
        self.map_err(|err| err.context(consequent))
    }

    fn with_context<E: IntoError>(
        self,
        consequent: impl FnOnce() -> E,
    ) -> Result<T, Error> {
        self.map_err(|err| err.with_context(consequent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // We test that our 'Error' type is the size we expect. This isn't an API
    // guarantee, but if the size increases, we really want to make sure we
    // decide to do that intentionally. So this should be a speed bump. And in
    // general, we should not increase the size without a very good reason.
    #[test]
    fn error_size() {
        let expected_size = core::mem::size_of::<usize>();
        assert_eq!(expected_size, core::mem::size_of::<Error>());
    }
}
