use rocket::{
    Data, Response,
    fairing::{Fairing, Info, Kind},
    http::Status,
    request::{self, FromRequest, Request},
};
use sentry::configure_scope;
use tracing::{Span, info};
use uuid::Uuid;

/// A type that represents a request's ID.
#[derive(Clone)]
pub struct RequestId(pub String);

impl ToString for RequestId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

/// Returns the current request's ID, assigning one only as necessary.
#[rocket::async_trait]
impl<'r> FromRequest<'r> for RequestId {
    type Error = ();

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, Self::Error> {
        // The closure passed to `local_cache` will be executed at most once per
        // request: the first time the `RequestId` guard is used. If it is
        // requested again, `local_cache` will return the same value.
        request::Outcome::Success(
            request
                .local_cache(|| {
                    RequestId(
                        request
                            .headers()
                            .get_one("X-Request-Id")
                            .map(ToString::to_string)
                            .unwrap_or_else(|| Uuid::new_v4().to_string()),
                    )
                })
                .clone(),
        )
    }
}

pub struct TracingSpan<T = tracing::Span>(pub T);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for TracingSpan {
    type Error = ();

    async fn from_request(
        request: &'r Request<'_>,
    ) -> rocket::request::Outcome<Self, ()> {
        match request.local_cache(|| TracingSpan::<Option<Span>>(None)) {
            TracingSpan(Some(span)) => {
                rocket::request::Outcome::Success(TracingSpan(span.to_owned()))
            }
            TracingSpan(None) => rocket::request::Outcome::Error((
                Status::InternalServerError,
                (),
            )),
        }
    }
}

pub struct RequestIdFairing;

#[rocket::async_trait]
impl Fairing for RequestIdFairing {
    fn info(&self) -> Info {
        Info {
            name: "Request ID fairing",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_request(&self, req: &mut Request<'_>, _data: &mut Data<'_>) {
        let request_id = req.guard::<RequestId>().await;
        let user_agent = req.headers().get_one("User-Agent").unwrap_or("");

        let _ = request_id.map(|request_id| {
            let span = tracing::info_span!(
                "request",
                otel.name=%format!("{} {}", req.method(), req.uri().path()),
                http.method = %req.method(),
                http.uri = %req.uri().path(),
                http.user_agent=%user_agent,
                http.status_code = tracing::field::Empty,
                http.request_id=%request_id.to_string()
            );
            span.in_scope(|| {
                tracing::info!("received request");
                configure_scope(|scope| {
                    scope.set_transaction(Some(&request_id.to_string()));
                });
            });
            req.local_cache(|| {
                TracingSpan::<Option<tracing::Span>>(Some(span))
            });
        });
    }

    async fn on_response<'r>(
        &self,
        req: &'r Request<'_>,
        res: &mut Response<'r>,
    ) {
        let request_id = req.guard::<RequestId>().await;

        if let Some(span) = req
            .local_cache(|| TracingSpan::<Option<Span>>(None))
            .0
            .to_owned()
        {
            let _entered_span = span.entered();
            _entered_span.record("http.status_code", res.status().code);

            let _ = request_id.as_ref().map(|request_id| {
                info!(
                    "Returning request {} with {}",
                    request_id.to_string(),
                    res.status()
                );
            });

            _entered_span.exit();
        }

        let _ = request_id.map(|request_id| {
            res.set_raw_header("X-Request-Id", request_id.to_string())
        });
    }
}
