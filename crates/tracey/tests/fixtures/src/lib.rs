//! Test implementation for integration testing.

/// Login function
///
/// r[impl auth.login]
/// This function also implements the fetch API requirement from the other spec.
/// r[impl api.fetch]
pub fn login(username: &str, password: &str) -> Result<Session, Error> {
    if username.is_empty() || password.is_empty() {
        return Err(Error::InvalidCredentials);
    }
    // Simplified implementation
    Ok(Session { id: 1 })
}

/// Session struct
///
/// r[impl auth.session]
pub struct Session {
    pub id: u64,
}

/// Logout function
///
/// r[impl auth.logout]
pub fn logout(_session: Session) {
    // Simplified implementation
}

/// Validate required fields
///
/// r[impl data.required-fields]
pub fn validate_required(data: &[(&str, Option<&str>)]) -> Result<(), Error> {
    // r[impl error.codes]
    // r[impl error.messages]
    for (name, value) in data {
        if value.is_none() {
            return Err(Error::MissingField(name.to_string()));
        }
    }
    Ok(())
}

/// Error type
///
/// r[impl error.codes]
#[derive(Debug)]
pub enum Error {
    InvalidCredentials,
    MissingField(String),
    InvalidFormat(String),
}

impl std::fmt::Display for Error {
    /// r[impl error.messages]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidCredentials => write!(f, "Invalid username or password"),
            Error::MissingField(field) => write!(f, "Missing required field: {}", field),
            Error::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
        }
    }
}
