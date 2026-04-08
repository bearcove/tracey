//! Test file for integration testing.

use super::*;

/// r[verify auth.login]
#[test]
fn test_login_success() {
    let result = login("user", "pass");
    assert!(result.is_ok());
}

/// r[verify auth.login]
#[test]
fn test_login_empty_credentials() {
    let result = login("", "");
    assert!(result.is_err());
}

/// r[verify data.required-fields]
#[test]
fn test_validate_required_fields() {
    let data = [("name", Some("John")), ("email", None)];
    let result = validate_required(&data);
    assert!(result.is_err());
}

/// r[verify error.codes]
/// r[verify error.messages]
#[test]
fn test_error_display() {
    let err = Error::MissingField("email".to_string());
    assert!(err.to_string().contains("email"));
}
