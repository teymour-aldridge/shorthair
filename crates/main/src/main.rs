#[rocket::launch]
fn rocket() -> _ {
    main::make_rocket("sqlite.db")
}
