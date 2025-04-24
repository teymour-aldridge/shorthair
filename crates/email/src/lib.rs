//! Sends emails.

// todo: this code is wrong (and a mess)
use db::DbConn;
use std::sync::Arc;

#[cfg(debug_assertions)]
#[tracing::instrument(level = "trace", skip(db, html_contents, text_contents))]
/// Sends an email - this is done on a background thread.
pub fn send_mail(
    to: Vec<(&str, &str)>,
    subject: &str,
    html_contents: &str,
    text_contents: &str,
    db: Arc<DbConn>,
) {
    use db::schema::emails;
    use diesel::prelude::*;
    use lettre::{
        message::{header::ContentType, Mailbox, MultiPart, SinglePart},
        Message,
    };
    use uuid::Uuid;

    let mut msg = Message::builder();
    for (name, email) in &to {
        msg =
            msg.to(Mailbox::new(Some(name.to_string()), email.parse().unwrap()))
    }
    let domain = std::env::var("SMTP_DOMAIN")
        .unwrap_or_else(|_| "example.com".to_string());

    let msg_id = format!("{}@{domain}", Uuid::now_v7().to_string());

    let msg = msg
        .message_id(Some(msg_id.to_string()))
        .subject(subject)
        .from(format!("bureaucrat@{domain}").parse().unwrap())
        .multipart(
            MultiPart::mixed()
                .singlepart(
                    SinglePart::builder()
                        .content_type(ContentType::TEXT_PLAIN)
                        .body(text_contents.to_string()),
                )
                .singlepart(
                    SinglePart::builder()
                        .content_type(ContentType::TEXT_HTML)
                        .body(html_contents.to_string()),
                ),
        )
        .unwrap();

    let recipients = to
        .iter()
        .map(|(k, v)| format!("{k} <{v}>"))
        .collect::<Vec<_>>()
        .join(",");

    // run in the background
    //
    // TODO: insert refernce to this into SQLite database
    rocket::tokio::spawn(async move {
        // todo: should log when this fails somewhere
        db.run(move |conn| {
            diesel::insert_into(emails::table)
                .values((
                    emails::message_id.eq(&msg_id),
                    emails::recipients.eq(recipients),
                    emails::contents
                        .eq(std::str::from_utf8(&msg.formatted()).unwrap()),
                    emails::created_at.eq(diesel::dsl::now),
                ))
                .execute(conn)
                .unwrap();
        })
        .await
    });

    tracing::trace!("Sent email");
}

#[cfg(not(debug_assertions))]
#[tracing::instrument(level = "trace", skip(db, html_contents, text_contents))]
pub fn send_mail(
    to: Vec<(&str, &str)>,
    subject: &str,
    html_contents: &str,
    text_contents: &str,
    db: Arc<DbConn>,
) {
    send_mail_internal(to, subject, html_contents, text_contents, db)
}

#[allow(unused)]
#[tracing::instrument(level = "trace", skip(db, html_contents, text_contents))]
fn send_mail_internal(
    to: Vec<(&str, &str)>,
    subject: &str,
    html_contents: &str,
    text_contents: &str,
    db: Arc<DbConn>,
) {
    use db::schema::emails;
    use diesel::prelude::*;
    use lettre::{
        message::{header::ContentType, MultiPart, SinglePart},
        transport::smtp::authentication::Credentials,
        AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    };
    use uuid::Uuid;

    let mut msg = Message::builder();
    for (name, email) in &to {
        msg = msg.to(format!("{name} <{email}>").parse().unwrap())
    }

    tracing::trace!("Added to field");

    let msg_id = format!(
        "{}@{}",
        Uuid::now_v7().to_string(),
        std::env::var("SMTP_DOMAIN").unwrap()
    );

    tracing::trace!("Message id is {msg_id}");

    let domain = std::env::var("SMTP_DOMAIN")
        .unwrap_or_else(|_| "example.com".to_string());

    let msg = msg
        .message_id(Some(msg_id.to_string()))
        .subject(subject)
        .from(format!("bureaucrat@{domain}").parse().unwrap())
        .multipart(
            MultiPart::mixed()
                .singlepart(
                    SinglePart::builder()
                        .content_type(ContentType::TEXT_PLAIN)
                        .body(text_contents.to_string()),
                )
                .singlepart(
                    SinglePart::builder()
                        .content_type(ContentType::TEXT_HTML)
                        .body(html_contents.to_string()),
                ),
        )
        .unwrap();

    let creds = Credentials::new(
        std::env::var("SMTP_USERNAME").unwrap(),
        std::env::var("SMTP_PASSWORD").unwrap(),
    );
    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::relay(
            &std::env::var("SMTP_HOST").unwrap(),
        )
        .unwrap()
        .credentials(creds)
        .build();

    let recipients = to
        .iter()
        .map(|(k, v)| format!("{k} <{v}>"))
        .collect::<Vec<_>>()
        .join(",");

    tracing::trace!("Created the message");

    // run in the background
    rocket::tokio::spawn(async move {
        tracing::trace!("Sending message on background thread");
        mailer.send(msg).await.unwrap();
        tracing::trace!("Send message on background thread");
        // todo: should log when this fails somewhere
        db.run(move |conn| {
            diesel::insert_into(emails::table)
                .values((
                    emails::message_id.eq(&msg_id),
                    emails::recipients.eq(recipients),
                    emails::created_at.eq(diesel::dsl::now),
                ))
                .execute(conn)
                .unwrap();
        })
        .await
    });
}
