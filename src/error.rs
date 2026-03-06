//! Typed error variants for structured error matching.
//!
//! Most of the codebase uses `anyhow::Result` for ad-hoc errors.
//! `SkillDoError` provides typed variants only where callers need
//! to `downcast_ref` and branch on the error kind (e.g., timeout detection).

use std::time::Duration;

/// Typed errors that callers need to match on.
/// Wrapped automatically by `anyhow` — callers use `downcast_ref::<SkillDoError>()`.
#[derive(Debug, thiserror::Error)]
pub enum SkillDoError {
    #[error("Command timed out after {0:?}")]
    Timeout(Duration),
}

impl SkillDoError {
    /// Check whether an `anyhow::Error` wraps a `SkillDoError::Timeout`.
    pub fn is_timeout(err: &anyhow::Error) -> bool {
        err.downcast_ref::<SkillDoError>()
            .is_some_and(|e| matches!(e, SkillDoError::Timeout(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_error_displays_duration() {
        let err = SkillDoError::Timeout(Duration::from_secs(120));
        assert_eq!(err.to_string(), "Command timed out after 120s");
    }

    #[test]
    fn timeout_error_downcasts_through_anyhow() {
        let anyhow_err: anyhow::Error = SkillDoError::Timeout(Duration::from_secs(60)).into();
        let downcast = anyhow_err.downcast_ref::<SkillDoError>();
        assert!(downcast.is_some());
        assert!(
            matches!(downcast.unwrap(), SkillDoError::Timeout(d) if *d == Duration::from_secs(60))
        );
    }

    #[test]
    fn plain_anyhow_error_does_not_downcast_to_timeout() {
        let anyhow_err = anyhow::anyhow!("some other error");
        assert!(anyhow_err.downcast_ref::<SkillDoError>().is_none());
    }

    #[test]
    fn is_timeout_helper_returns_true_for_timeout() {
        let err: anyhow::Error = SkillDoError::Timeout(Duration::from_secs(30)).into();
        assert!(SkillDoError::is_timeout(&err));
    }

    #[test]
    fn is_timeout_helper_returns_false_for_other_errors() {
        let err = anyhow::anyhow!("some other error");
        assert!(!SkillDoError::is_timeout(&err));
    }
}
