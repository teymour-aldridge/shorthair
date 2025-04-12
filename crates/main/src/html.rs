use db::user::User;
use maud::{html, Markup, DOCTYPE};

/// Renders an HTML page with the provided body markup.
pub fn page_of_body(body: Markup, user: Option<User>) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                title { "Shorthair" }
                script src="https://unpkg.com/htmx.org@2.0.2" integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ" crossorigin="anonymous" {}
                link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet" integrity="sha384-QWTKZyjpPEjISv5WaRU9OFeRpok6YctnYmDr5pNlyT2bRjXh0JMhjY6hW+ALEwIH" crossorigin="anonymous" {}
                meta name="viewport" content="width=device-width, initial-scale=1" {}
            }
            body {
                nav class="navbar navbar-expand" style="background-color: #CB0BAA" data-bs-theme="dark" {
                    div class="container-fluid" {
                        ul class="nav nav-justify-start" data-bs-theme="dark" {
                            li class="nav-item" {
                                a class="nav-link text-white" href="/" { "Home" }
                            }
                        }
                        ul class="nav nav-justify-end" data-bs-theme="dark" {
                            @if user.is_some() {
                                li class="nav-item nav-justify-start" {
                                    a class="nav-link text-white" aria-current="page" href="/user" { "Profile" }
                                }
                            } @else {
                                li {
                                    a href="/login" { "Login" }
                                }
                            }
                        }
                    }
                }
                div class="container" {
                    div class="mt-4" {
                        (body)
                    }
                }
            }
        }
    }
}

pub fn error_403<T: ToString>(error: Option<T>, user: Option<User>) -> Markup {
    page_of_body(
        html! {
            div class="text-center" {
                h1 class="display-1 text-danger" { "403" }
                h2 class="mb-4" { "Forbidden" }
                p class="lead" { "You don't have permission to access this resource." }
                @if let Some(err) = error {
                    div class="alert alert-danger" role="alert" {
                        (err.to_string())
                    }
                }
                a class="btn btn-danger" href="/" { "Return Home" }
            }
        },
        user,
    )
}

pub fn error_404<T: ToString>(error: Option<T>, user: Option<User>) -> Markup {
    page_of_body(
        html! {
            div class="text-center" {
                h1 class="display-1 text-danger" { "404" }
                h2 class="mb-4" { "Not found" }
                p class="lead" { "You don't have permission to access this resource." }
                @if let Some(err) = error {
                    div class="alert alert-danger" role="alert" {
                        (err.to_string())
                    }
                }
                a class="btn btn-danger" href="/" { "Return Home" }
            }
        },
        user,
    )
}
