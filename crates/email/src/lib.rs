//! Sends emails.
use db::DbConn;
use std::sync::Arc;

#[cfg(debug_assertions)]
pub async fn send_mail(
    _to: Vec<(&str, &str)>,
    _subject: &str,
    _html_contents: &str,
    _text_contents: &str,
    _db: Arc<DbConn>,
) {
}

#[cfg(not(debug_assertions))]
pub async fn send_mail(
    to: Vec<(&str, &str)>,
    subject: &str,
    html_contents: &str,
    text_contents: &str,
    db: Arc<DbConn>,
) {
    send_mail_internal(to, html_contents, text_contents, db).await
}

#[allow(unused)]
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

    let msg_id = format!(
        "{}@{}",
        Uuid::now_v7().to_string(),
        std::env::var("SMTP_DOMAIN").unwrap()
    );

    let msg = msg
        .message_id(Some(msg_id.to_string()))
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

    // run in the background
    //
    // TODO: insert refernce to this into SQLite database
    rocket::tokio::spawn(async move {
        mailer.send(msg).await.unwrap();
        // todo: should log when this fails somewhere
        db.run(move |conn| {
            diesel::insert_into(emails::table)
                .values((
                    emails::message_id.eq(&msg_id),
                    emails::recipients.eq(recipients),
                ))
                .execute(conn)
                .unwrap();
        })
        .await
    });
}
