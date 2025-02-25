use db::user::LOGIN_COOKIE;
use rocket::{http::CookieJar, response::Redirect};

#[get("/logout")]
pub async fn logout(jar: &CookieJar<'_>) -> Redirect {
    jar.remove(LOGIN_COOKIE);
    Redirect::to("/")
}
