use super::*;

/// The error type which is returned when signing in fails.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum SignInError {
    /// Sign-up with an official client is required.
    ///
    /// Third-party applications, such as those developed with *grammers*,
    /// cannot be used to register new accounts. For more details, see this
    /// [comment regarding third-party app sign-ups](https://bugs.telegram.org/c/25410/1):
    /// > \[…] if a user doesn’t have a Telegram account yet,
    /// > they will need to create one first using an official mobile Telegram app.
    SignUpRequired,
    /// The account has 2FA enabled, and the password is required.
    PasswordRequired(PasswordToken),
    /// The code used to complete login was not valid.
    InvalidCode,
    /// The 2FA password used to complete login was not valid.
    InvalidPassword(PasswordToken),
    /// A generic invocation error occured.
    Other(InvocationError),
}

impl fmt::Display for SignInError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SignInError::*;
        match self {
            SignUpRequired => write!(f, "sign in error: sign up with official client required"),
            PasswordRequired(_password) => write!(f, "2fa password required"),
            InvalidCode => write!(f, "sign in error: invalid code"),
            InvalidPassword(_password) => write!(f, "invalid password"),
            Other(e) => write!(f, "sign in error: {e}"),
        }
    }
}

impl std::error::Error for SignInError {}

/// Login token needed to continue the login process after sending the code.
pub struct LoginToken {
    pub(crate) phone: String,
    pub(crate) phone_code_hash: String,
}

/// Password token needed to complete a 2FA login.
#[derive(Debug)]
pub struct PasswordToken {
    pub(crate) password: Box<tl::types::account::Password>,
}

impl PasswordToken {
    pub fn new(password: tl::types::account::Password) -> Self {
        PasswordToken {
            password: Box::new(password),
        }
    }

    pub fn hint(&self) -> Option<&str> {
        self.password.hint.as_deref()
    }
}
