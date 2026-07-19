//! Error types, and the conversion from `Discord_ClientResult` into [`Result`].
//!
//! Nearly every async SDK operation reports back through a `ClientResult`. The
//! wrapper converts that into a normal Rust [`Result`] at the callback boundary,
//! so callers match on `Ok`/`Err` instead of inspecting a success flag.

use crate::string;
use discord_social_sdk_sys as sys;
use std::fmt;

/// The category of a failure reported by the SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The transport failed before a response was received.
    Network,
    /// The server responded with a non-success HTTP status.
    Http,
    /// A call was made before the client reached `Status::Ready`.
    ClientNotReady,
    /// The requested feature is disabled for this application.
    Disabled,
    /// The client was destroyed while the request was in flight.
    ClientDestroyed,
    /// Arguments failed validation before being sent.
    Validation,
    /// The request was cancelled.
    Aborted,
    /// Authorization was rejected or the token was rejected.
    AuthorizationFailed,
    /// The Discord client returned an RPC-level error.
    Rpc,
    /// An error type this binding does not yet name.
    Other(i32),
}

impl ErrorKind {
    fn from_raw(raw: sys::Discord_ErrorType) -> Self {
        use sys::Discord_ErrorType as T;
        match raw {
            T::Discord_ErrorType_NetworkError => Self::Network,
            T::Discord_ErrorType_HTTPError => Self::Http,
            T::Discord_ErrorType_ClientNotReady => Self::ClientNotReady,
            T::Discord_ErrorType_Disabled => Self::Disabled,
            T::Discord_ErrorType_ClientDestroyed => Self::ClientDestroyed,
            T::Discord_ErrorType_ValidationError => Self::Validation,
            T::Discord_ErrorType_Aborted => Self::Aborted,
            T::Discord_ErrorType_AuthorizationFailed => Self::AuthorizationFailed,
            T::Discord_ErrorType_RPCError => Self::Rpc,
            other => Self::Other(other.0),
        }
    }
}

/// A failed SDK operation.
///
/// Carries everything the SDK reported so callers can distinguish a retryable
/// rate limit from a permanent validation failure.
// No `Eq`: `retry_after` is a float, and the SDK reports it as one.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    code: i32,
    http_status: u32,
    response_body: String,
    retryable: bool,
    /// Seconds to wait before retrying, in milliseconds of precision as reported.
    retry_after: Option<f32>,
}

impl Error {
    /// The category of this failure.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The SDK's human-readable description.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The SDK's numeric error code, or `0` when unset.
    pub fn code(&self) -> i32 {
        self.code
    }

    /// The HTTP status, or `0` when the failure was not an HTTP error.
    pub fn http_status(&self) -> u32 {
        self.http_status
    }

    /// The raw response body, when the failure came from an HTTP request.
    pub fn response_body(&self) -> &str {
        &self.response_body
    }

    /// Whether retrying this exact request may succeed.
    pub fn is_retryable(&self) -> bool {
        self.retryable
    }

    /// How long to wait before retrying, when the SDK specified a delay.
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        self.retry_after
            .filter(|s| *s > 0.0)
            .map(std::time::Duration::from_secs_f32)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.kind)?;
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        if self.http_status != 0 {
            write!(f, " (HTTP {})", self.http_status)?;
        }
        if self.code != 0 {
            write!(f, " (code {})", self.code)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// The result type used throughout this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Convert a `Discord_ClientResult` delivered to a callback into a [`Result`],
/// taking ownership of it.
///
/// The SDK hands the callback an **owned** result and expects the callee to
/// release it — the official C++ wrapper adopts it as `DiscordObjectState::Owned`
/// and drops it when the callback returns. Not dropping it leaks on every
/// completed request, so this function always drops before returning.
///
/// # Safety
///
/// `raw` must be a `ClientResult` the SDK just transferred to us, and must not
/// be used again afterwards. Calling this twice on the same pointer double-frees.
pub(crate) unsafe fn to_result(raw: *mut sys::Discord_ClientResult) -> Result<()> {
    if raw.is_null() {
        // A null result has no failure to report and nothing to release; treat it
        // as success rather than inventing an error the SDK never signalled.
        return Ok(());
    }
    unsafe {
        let outcome = if sys::Discord_ClientResult_Successful(raw) {
            Ok(())
        } else {
            Err(Error {
                kind: ErrorKind::from_raw(sys::Discord_ClientResult_Type(raw)),
                message: string::out(|out| sys::Discord_ClientResult_Error(raw, out)),
                code: sys::Discord_ClientResult_ErrorCode(raw),
                http_status: sys::Discord_ClientResult_Status(raw).0 as u32,
                response_body: string::out(|out| sys::Discord_ClientResult_ResponseBody(raw, out)),
                retryable: sys::Discord_ClientResult_Retryable(raw),
                retry_after: Some(sys::Discord_ClientResult_RetryAfter(raw)),
            })
        };
        // Release the result now that everything has been copied out of it.
        sys::Discord_ClientResult_Drop(raw);
        outcome
    }
}
