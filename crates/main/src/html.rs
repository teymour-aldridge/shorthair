use db::user::User;
use maud::{html, Markup, DOCTYPE};

pub fn page_of_body_and_flash_msg(
    body: Markup,
    flash: Option<String>,
    user: Option<User>,
) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                title { "Eldemite" }
                script src="/static/htmx.js" crossorigin="anonymous" {}
                link href="/static/styles.css" rel="stylesheet" {}
                meta name="viewport" content="width=device-width, initial-scale=1" {}
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
                                    a class="nav-link text-white" href="/login" { "Login" }
                                }
                            }
                        }
                    }
                }

                @if let Some(flash_msg) = flash {
                    div class="container mt-3" {
                        div class="alert alert-danger" role="alert" {
                            (flash_msg)
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
    html! {
        (DOCTYPE)
        html {
            head {
                title { "Eldemite" }
                script src="/static/htmx.js" crossorigin="anonymous" {}
                link href="/static/styles.css" rel="stylesheet" {}
                meta name="viewport" content="width=device-width, initial-scale=1" {}
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
                                    a class="nav-link text-white" href="/login" { "Login" }
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

pub fn page_title<T: ToString>(title: T) -> Markup {
    maud::html! {
        div class="col-md m-3 h2 d-flex align-items-center" {
            h1 { (title.to_string()) }
        }
    }
}
