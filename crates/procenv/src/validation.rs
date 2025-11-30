//! Validation error types for the `validator` crate integration.
//!
//! This module provides error types for validation failures, converting
//! from the `validator` crate's error types to procenv's error model.

use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};

use miette::Diagnostic;

/// A validation error for a specific field.
///
/// This struct represents a single validation failure, including the field
/// name, validation rule code, and human-readable message.
#[derive(Debug, Diagnostic)]
#[diagnostic(code(procenv::field_validation_error))]
pub struct ValidationFieldError {
    /// The field name that failed validation.
    pub field: String,

    /// The validation rule that failed (e.g., "email", "range", "url").
    pub code: String,

    /// Human-readable error message.
    #[help]
    pub message: String,

    /// Additional parameters from the validation rule (e.g., min/max values).
    pub params: Option<String>,
}

impl ValidationFieldError {
    /// Create a new validation field error.
    pub fn new(
        field: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
            message: message.into(),
            params: None,
        }
    }

    /// Add parameters to the error (e.g., "min: 1, max: 100").
    pub fn with_params(mut self, params: impl Into<String>) -> Self {
        self.params = Some(params.into());

        self
    }

    /// Extract the human-readable message from a validation error.
    ///
    /// Returns the custom message if set, otherwise generates a default
    /// message using the validation code.
    fn extract_message(error: &::validator::ValidationError) -> String {
        error
            .message
            .as_ref()
            .map(|m| m.to_string())
            .unwrap_or_else(|| format!("validation failed: {}", error.code))
    }

    /// Extract validation parameters as a formatted string.
    ///
    /// Filters out the "value" parameter (which contains the actual value)
    /// and formats remaining parameters as "key: value" pairs.
    fn extract_params(error: &::validator::ValidationError) -> Option<String> {
        if error.params.is_empty() {
            return None;
        }

        let param_strs: Vec<String> = error
            .params
            .iter()
            .filter(|(k, _)| *k != "value")
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();

        if param_strs.is_empty() {
            None
        } else {
            Some(param_strs.join(", "))
        }
    }

    /// Create a ValidationFieldError from a validator error.
    fn from_validator_error(field: &str, error: &::validator::ValidationError) -> Self {
        let message = Self::extract_message(error);
        let params = Self::extract_params(error);

        let mut err = Self::new(field.to_string(), error.code.to_string(), message);

        if let Some(p) = params {
            err = err.with_params(p);
        }

        err
    }

    /// Convert validator crate errors to our error type.
    pub fn validation_errors_to_procenv(
        errors: ::validator::ValidationErrors,
    ) -> Vec<ValidationFieldError> {
        // Collect flat field errors
        let flat_errors = errors
            .field_errors()
            .into_iter()
            .flat_map(|(field, field_errors)| {
                field_errors
                    .iter()
                    .map(move |error| Self::from_validator_error(&field, error))
            });

        // Collect nested struct errors with prefixed field paths
        let nested_errors = errors.errors().into_iter().filter_map(|(field, nested)| {
            if let ::validator::ValidationErrorsKind::Struct(nested) = nested {
                let nested_field_errors = Self::validation_errors_to_procenv(*nested.clone());
                Some(nested_field_errors.into_iter().map(move |mut err| {
                    err.field = format!("{}.{}", field, err.field);
                    err
                }))
            } else {
                None
            }
        });

        flat_errors.chain(nested_errors.flatten()).collect()
    }
}

impl Display for ValidationFieldError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "field `{}` failed validation: {}",
            self.field, self.message
        )
    }
}

impl StdError for ValidationFieldError {}

/// Standalone function to convert validator crate errors to procenv errors.
///
/// This is a convenience wrapper around [`ValidationFieldError::validation_errors_to_procenv`].
/// It's exported at the crate root for use in generated code.
pub fn validation_errors_to_procenv(
    errors: ::validator::ValidationErrors,
) -> Vec<ValidationFieldError> {
    ValidationFieldError::validation_errors_to_procenv(errors)
}
