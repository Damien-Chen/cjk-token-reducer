use reqwest::StatusCode;
use thiserror::Error;

/// Error categories for actionable diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Authentication/authorization issues - check API key
    Auth,
    /// Rate limiting - slow down requests
    RateLimit,
    /// Quota exceeded - upgrade plan or wait
    Quota,
    /// Network connectivity - check internet connection
    Network,
    /// Server-side error - retry later
    Server,
    /// Client-side error - fix request
    Client,
    /// Configuration error - fix config file
    Config,
    /// Cache error - check disk space/permissions
    Cache,
    /// Unknown error
    Unknown,
}

impl ErrorCategory {
    /// Get actionable advice for this error category
    pub fn advice(&self) -> &'static str {
        match self {
            Self::Auth => "Check your API credentials or authentication setup",
            Self::RateLimit => "Too many requests. Wait and retry with backoff",
            Self::Quota => "API quota exceeded. Wait for reset or upgrade plan",
            Self::Network => "Check internet connection and firewall settings",
            Self::Server => "Google Translate service issue. Retry in a few minutes",
            Self::Client => "Invalid request. Check input text encoding",
            Self::Config => "Fix configuration file syntax or values",
            Self::Cache => "Check disk space and file permissions for cache directory",
            Self::Unknown => "Unexpected error. Check logs for details",
        }
    }
}

/// Unified crate-level error type
///
/// All errors in the crate should use this enum with `thiserror` for proper error propagation.
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Rate limited (HTTP 429){retry_msg}. {}", ErrorCategory::RateLimit.advice(), retry_msg = .retry_after_secs.map(|s| format!(", retry after {}s", s)).unwrap_or_default())]
    RateLimited {
        /// Server-suggested retry delay from Retry-After header
        retry_after_secs: Option<u64>,
    },

    #[error("HTTP {status} (retryable). {}", ErrorCategory::Server.advice())]
    RetryableHttp { status: StatusCode },

    #[error("Authentication failed (HTTP {status}). {}", ErrorCategory::Auth.advice())]
    AuthError { status: StatusCode },

    #[error("Quota exceeded (HTTP {status}). {}", ErrorCategory::Quota.advice())]
    QuotaExceeded { status: StatusCode },

    #[error("Translation failed: {message}")]
    Translation { message: String },

    #[error("Config error: {message}")]
    Config { message: String },

    #[error("Cache error: {message}")]
    Cache { message: String },

    #[error(
        "Circuit breaker open. Translation service temporarily unavailable. Retry in {0} seconds"
    )]
    CircuitOpen(u64),

    #[error("Connection timeout. {}", ErrorCategory::Network.advice())]
    Timeout,

    #[error("Connection failed. {}", ErrorCategory::Network.advice())]
    ConnectionFailed,
}

impl Error {
    /// Classify error into category for handling decisions
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Io(_) => ErrorCategory::Cache,
            Self::Json(_) => ErrorCategory::Client,
            Self::Http(e) => {
                if e.is_timeout() || e.is_connect() {
                    ErrorCategory::Network
                } else if let Some(status) = e.status() {
                    Self::category_from_status(status)
                } else {
                    ErrorCategory::Unknown
                }
            }
            Self::RateLimited { .. } => ErrorCategory::RateLimit,
            Self::RetryableHttp { status } => Self::category_from_status(*status),
            Self::AuthError { .. } => ErrorCategory::Auth,
            Self::QuotaExceeded { .. } => ErrorCategory::Quota,
            Self::Translation { .. } => ErrorCategory::Client,
            Self::Config { .. } => ErrorCategory::Config,
            Self::Cache { .. } => ErrorCategory::Cache,
            Self::CircuitOpen(_) => ErrorCategory::Server,
            Self::Timeout => ErrorCategory::Network,
            Self::ConnectionFailed => ErrorCategory::Network,
        }
    }

    /// Determine if this error should trigger a retry
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.category(),
            ErrorCategory::RateLimit | ErrorCategory::Server | ErrorCategory::Network
        )
    }

    /// Classify HTTP status code into error category
    fn category_from_status(status: StatusCode) -> ErrorCategory {
        match status.as_u16() {
            401 | 403 => ErrorCategory::Auth,
            429 => ErrorCategory::RateLimit,
            402 | 451 => ErrorCategory::Quota,
            400..=499 => ErrorCategory::Client,
            500..=599 => ErrorCategory::Server,
            _ => ErrorCategory::Unknown,
        }
    }

    /// Create appropriate error from HTTP status code
    pub fn from_status(status: StatusCode) -> Self {
        Self::from_status_with_retry_after(status, None)
    }

    /// Create error from HTTP status with optional Retry-After value
    pub fn from_status_with_retry_after(status: StatusCode, retry_after_secs: Option<u64>) -> Self {
        match status.as_u16() {
            401 | 403 => Self::AuthError { status },
            429 => Self::RateLimited { retry_after_secs },
            402 | 451 => Self::QuotaExceeded { status },
            500..=599 => Self::RetryableHttp { status },
            _ => Self::Translation {
                message: format!("HTTP {}", status.as_u16()),
            },
        }
    }

    /// Extract retry_after_secs from RateLimited error
    pub fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_secs } => *retry_after_secs,
            _ => None,
        }
    }
}

/// Crate-level Result type alias for convenience
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_categories() {
        assert_eq!(
            Error::RateLimited {
                retry_after_secs: None
            }
            .category(),
            ErrorCategory::RateLimit
        );
        assert_eq!(
            Error::RetryableHttp {
                status: StatusCode::SERVICE_UNAVAILABLE
            }
            .category(),
            ErrorCategory::Server
        );
        assert_eq!(
            Error::AuthError {
                status: StatusCode::UNAUTHORIZED
            }
            .category(),
            ErrorCategory::Auth
        );
    }

    #[test]
    fn test_retryable_errors() {
        assert!(Error::RateLimited {
            retry_after_secs: None
        }
        .is_retryable());
        assert!(Error::RetryableHttp {
            status: StatusCode::BAD_GATEWAY
        }
        .is_retryable());
        assert!(Error::Timeout.is_retryable());
        assert!(Error::ConnectionFailed.is_retryable());
        assert!(!Error::Config {
            message: "bad config".into()
        }
        .is_retryable());
        assert!(!Error::AuthError {
            status: StatusCode::UNAUTHORIZED
        }
        .is_retryable());
        assert!(!Error::QuotaExceeded {
            status: StatusCode::PAYMENT_REQUIRED
        }
        .is_retryable());
    }

    #[test]
    fn test_from_status() {
        assert!(matches!(
            Error::from_status(StatusCode::UNAUTHORIZED),
            Error::AuthError { .. }
        ));
        assert!(matches!(
            Error::from_status(StatusCode::TOO_MANY_REQUESTS),
            Error::RateLimited { .. }
        ));
        assert!(matches!(
            Error::from_status(StatusCode::BAD_GATEWAY),
            Error::RetryableHttp { .. }
        ));
    }

    #[test]
    fn test_error_messages_include_advice() {
        let err = Error::RateLimited {
            retry_after_secs: None,
        };
        let msg = err.to_string();
        assert!(msg.contains("Wait and retry"));

        let err = Error::RateLimited {
            retry_after_secs: Some(30),
        };
        let msg = err.to_string();
        assert!(msg.contains("retry after 30s"));

        let err = Error::CircuitOpen(60);
        let msg = err.to_string();
        assert!(msg.contains("60 seconds"));
    }

    #[test]
    fn test_retry_after_extraction() {
        let err = Error::RateLimited {
            retry_after_secs: Some(60),
        };
        assert_eq!(err.retry_after_secs(), Some(60));

        let err = Error::RateLimited {
            retry_after_secs: None,
        };
        assert_eq!(err.retry_after_secs(), None);

        let err = Error::Timeout;
        assert_eq!(err.retry_after_secs(), None);
    }

    #[test]
    fn test_error_category_advice_all_variants() {
        // Every category should return non-empty advice
        let categories = [
            ErrorCategory::Auth,
            ErrorCategory::RateLimit,
            ErrorCategory::Quota,
            ErrorCategory::Network,
            ErrorCategory::Server,
            ErrorCategory::Client,
            ErrorCategory::Config,
            ErrorCategory::Cache,
            ErrorCategory::Unknown,
        ];
        for cat in categories {
            assert!(!cat.advice().is_empty(), "{:?} should have advice", cat);
        }
    }

    #[test]
    fn test_error_display_all_variants() {
        // Every error variant should produce a non-empty Display string
        let errors: Vec<Error> = vec![
            Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "test")),
            Error::Json(serde_json::from_str::<()>("bad").unwrap_err()),
            Error::RateLimited {
                retry_after_secs: Some(10),
            },
            Error::RetryableHttp {
                status: StatusCode::BAD_GATEWAY,
            },
            Error::AuthError {
                status: StatusCode::UNAUTHORIZED,
            },
            Error::QuotaExceeded {
                status: StatusCode::PAYMENT_REQUIRED,
            },
            Error::Translation {
                message: "test".into(),
            },
            Error::Config {
                message: "test".into(),
            },
            Error::Cache {
                message: "test".into(),
            },
            Error::CircuitOpen(30),
            Error::Timeout,
            Error::ConnectionFailed,
        ];
        for err in errors {
            let msg = err.to_string();
            assert!(!msg.is_empty(), "{:?} should have Display output", err);
        }
    }

    #[test]
    fn test_category_all_error_variants() {
        // Test category() for variants not covered by other tests
        let io_err = Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "test"));
        assert_eq!(io_err.category(), ErrorCategory::Cache);

        let json_err = Error::Json(serde_json::from_str::<()>("bad").unwrap_err());
        assert_eq!(json_err.category(), ErrorCategory::Client);

        let translation_err = Error::Translation {
            message: "test".into(),
        };
        assert_eq!(translation_err.category(), ErrorCategory::Client);

        let circuit_err = Error::CircuitOpen(60);
        assert_eq!(circuit_err.category(), ErrorCategory::Server);

        let conn_err = Error::ConnectionFailed;
        assert_eq!(conn_err.category(), ErrorCategory::Network);
    }

    #[test]
    fn test_from_status_client_error() {
        // 400 BAD_REQUEST should become Translation variant
        let err = Error::from_status(StatusCode::BAD_REQUEST);
        assert!(matches!(err, Error::Translation { .. }));
        assert_eq!(err.category(), ErrorCategory::Client);
    }

    #[test]
    fn test_from_status_with_retry_after() {
        let err = Error::from_status_with_retry_after(StatusCode::TOO_MANY_REQUESTS, Some(42));
        assert_eq!(err.retry_after_secs(), Some(42));
    }
}
