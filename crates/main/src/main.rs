#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    main::make_rocket("sqlite.db").launch().await?;

    Ok(())
}
