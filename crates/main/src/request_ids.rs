use rocket::{
    fairing::{Fairing, Info, Kind},
    request::{self, FromRequest, Request},
    Data, Response,
};
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

        let _ = request_id.map(|request_id| {
            rocket::info!("Incoming request with ID {}", request_id.0)
        });
    }

    async fn on_response<'r>(
        &self,
        req: &'r Request<'_>,
        res: &mut Response<'r>,
    ) {
        let request_id = req.guard::<RequestId>().await;

        let _ = request_id.map(|request_id| {
            res.set_raw_header("X-Request-Id", request_id.0.clone())
        });
    }
}
