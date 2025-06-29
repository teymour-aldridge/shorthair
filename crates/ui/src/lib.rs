use db::user::User;
use maud::{html, Markup, DOCTYPE};

pub fn page_of_body_and_flash_msg(
    body: Markup,
    flash: Option<String>,
    user: Option<User>,
) -> Markup {
    page_of_body(
        maud::html! {
            @if let Some(flash_msg) = flash {
                div class="container mt-3" {
                    div class="alert alert-danger" role="alert" {
                        (flash_msg)
                    }
                }
            }
            (body)
        },
        user,
    )
}

pub fn page_of_body_with_extra_head(
    body: Markup,
    user: Option<User>,
    extra_head: Option<Markup>,
) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                title { "Eldemite" }
                script src="https://unpkg.com/htmx.org@2.0.2" integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ" crossorigin="anonymous" {}
                link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet" integrity="sha384-QWTKZyjpPEjISv5WaRU9OFeRpok6YctnYmDr5pNlyT2bRjXh0JMhjY6hW+ALEwIH" crossorigin="anonymous" {}
                meta name="viewport" content="width=device-width, initial-scale=1" {}
                @if let Some(head) = extra_head {
                    (head)
                }
            }
            body {
                nav class="navbar navbar-expand" style="background-color: #E32879" data-bs-theme="dark" {
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
                                li class="nav-item nav-justify-start" {
                                    a class="nav-link text-white" aria-current="page" href="/logout" { "Logout" }
                                }
                            } @else {
                                li {
                                    a class="nav-link text-white" href="/login" { "Admin" }
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

/// Renders an HTML page with the provided body markup.
pub fn page_of_body(body: Markup, user: Option<User>) -> Markup {
    page_of_body_with_extra_head(body, user, None)
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

pub fn page_title<T: ToString>(title: T) -> Markup {
    maud::html! {
        div class="col-md m-3 h2 d-flex align-items-center" {
            h1 { (title.to_string()) }
        }
    }
}
