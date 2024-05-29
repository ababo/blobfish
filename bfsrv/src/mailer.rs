use crate::config::Config;
use lettre::message::header::ContentType;
use lettre::{transport::smtp::authentication::Credentials, AsyncSmtpTransport, Tokio1Executor};
use lettre::{Address as EmailAddress, AsyncTransport, Message};
use reqwest::StatusCode;

/// Mailer error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("lettre_smtp")]
    LettreSmtp(
        #[from]
        #[source]
        lettre::transport::smtp::Error,
    ),
}

impl Error {
    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        use Error::*;
        match self {
            LettreSmtp(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match self {
            LettreSmtp(_) => "lettre_smtp",
        }
    }
}

/// Mailer result.
pub type Result<T> = std::result::Result<T, Error>;

/// Email sender.
pub struct Mailer {
    from: EmailAddress,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl Mailer {
    /// Create a new Mailer instance.
    pub fn new(config: &Config) -> Self {
        let credentials = Credentials::new(
            config.smtp_username.to_owned(),
            config.smtp_password.to_owned(),
        );

        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_relay)
            .unwrap()
            .credentials(credentials)
            .build();

        Self {
            from: config.smtp_from.clone(),
            transport,
        }
    }

    pub async fn send_token(&self, email: EmailAddress, access_token: &str) -> Result<()> {
        let body = format!(
            "Hi,

Here is the access token to confirm your email:

{access_token}

Please ignore this email if you did not initiate this action.
"
        );

        let message = Message::builder()
            .from(self.from.clone().into())
            .to(email.into())
            .subject("Email confirmation")
            .header(ContentType::TEXT_PLAIN)
            .body(body)
            .unwrap();

        self.transport.send(message).await?;

        Ok(())
    }
}
