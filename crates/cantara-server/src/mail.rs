//! Sending invitations.
//!
//! Over SMTP where the server is told how (`CANTARA_SMTP_*`), into a
//! directory as `.eml` files where it is told that instead
//! (`CANTARA_MAIL_DIR`), and not at all otherwise — then whoever invites is
//! shown the link to pass on, which is what happens anyway when sending fails.

use lettre::message::header::ContentType;
use lettre::message::{Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::MailConfig;
use crate::pages::escape;

pub enum Mailer {
    Off,
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: Mailbox,
    },
    Directory {
        path: std::path::PathBuf,
        from: Mailbox,
    },
}

impl Mailer {
    pub fn new(config: &MailConfig) -> Result<Mailer, String> {
        let mailbox = |from: &str| from.parse::<Mailbox>().map_err(|error| format!("CANTARA_MAIL_FROM: {error}"));
        Ok(match config {
            MailConfig::Off => Mailer::Off,
            MailConfig::Directory { path, from } => {
                std::fs::create_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))?;
                Mailer::Directory {
                    path: path.clone(),
                    from: mailbox(from)?,
                }
            }
            MailConfig::Smtp {
                host,
                port,
                security,
                username,
                password,
                from,
            } => {
                let builder = match security.as_str() {
                    "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(host),
                    "none" => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)),
                    _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host),
                }
                .map_err(|error| format!("CANTARA_SMTP_HOST: {error}"))?
                .port(*port);
                let builder = match (username, password) {
                    (Some(username), Some(password)) => {
                        builder.credentials(Credentials::new(username.clone(), password.clone()))
                    }
                    _ => builder,
                };
                Mailer::Smtp {
                    transport: builder.build(),
                    from: mailbox(from)?,
                }
            }
        })
    }

    pub fn is_on(&self) -> bool {
        !matches!(self, Mailer::Off)
    }

    /// Sends an invitation. `Ok(false)` where there is nothing to send with.
    pub async fn invite(&self, to: &str, organisation: &str, inviter: &str, link: &str) -> Result<bool, String> {
        let from = match self {
            Mailer::Off => return Ok(false),
            Mailer::Smtp { from, .. } | Mailer::Directory { from, .. } => from.clone(),
        };
        let to: Mailbox = to.parse().map_err(|error| format!("{to}: {error}"))?;
        let subject = format!("Einladung zu {organisation} / Invitation to {organisation}");
        let text = format!(
            "{inviter} lädt dich zu {organisation} in Cantara ein.\n\
             Mit diesem Link legst du ein Konto an oder meldest dich an:\n\n{link}\n\n\
             Der Link gilt 14 Tage.\n\n---\n\n\
             {inviter} invites you to {organisation} on Cantara.\n\
             This link creates an account or logs you in:\n\n{link}\n\n\
             The link is valid for 14 days.\n"
        );
        let html = format!(
            "<p>{inviter} lädt dich zu <strong>{organisation}</strong> in Cantara ein.</p>\
             <p><a href=\"{link}\">Einladung annehmen</a></p>\
             <p style=\"color:#666\">Der Link gilt 14 Tage.</p><hr>\
             <p>{inviter} invites you to <strong>{organisation}</strong> on Cantara.</p>\
             <p><a href=\"{link}\">Accept the invitation</a></p>\
             <p style=\"color:#666\">The link is valid for 14 days.</p>",
            inviter = escape(inviter),
            organisation = escape(organisation),
            link = escape(link),
        );
        let message = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::builder().header(ContentType::TEXT_PLAIN).body(text))
                    .singlepart(SinglePart::builder().header(ContentType::TEXT_HTML).body(html)),
            )
            .map_err(|error| error.to_string())?;

        match self {
            Mailer::Off => Ok(false),
            Mailer::Smtp { transport, .. } => transport
                .send(message)
                .await
                .map(|_| true)
                .map_err(|error| error.to_string()),
            Mailer::Directory { path, .. } => {
                let file = path.join(format!("{}.eml", uuid::Uuid::now_v7()));
                tokio::fs::write(&file, message.formatted())
                    .await
                    .map(|()| true)
                    .map_err(|error| error.to_string())
            }
        }
    }
}
