use std::collections::HashMap;

use maud::Markup;
use rocket::{form::Form, http::Status};
use tabbycat_api::types::{TeamStandings, TeamStandingsMetricsItemMetric};
use tracing::Instrument;

use crate::request_ids::{RequestId, TracingSpan};

#[get("/break-slides")]
pub async fn break_slides_page() -> Markup {
    ui::page_of_body(make_form(None, None), None)
}

fn make_form(
    error: Option<String>,
    form_data: Option<&BreakSlidesForm>,
) -> Markup {
    maud::html! {
        div class="row justify-content-center" {
            div class="col-md-6" {
                div class="card" {
                    div class="card-header" {
                        h4 class="mb-0" { "Generate Break Slides" }
                    }
                    div class="card-body" {
                        @if let Some(err) = error {
                            div class="alert alert-danger" {
                                (err)
                            }
                        }
                        form method="post" action="/break-slides?print-pdf" {
                            div class="mb-3" {
                                label for="url" class="form-label" { "URL" }
                                input type="url" class="form-control" id="url" name="url"
                                    value=(form_data.map(|f| f.url.as_str()).unwrap_or("")) required;
                            }
                            div class="mb-3" {
                                label for="api_key" class="form-label" { "API Key" }
                                input type="text" class="form-control" id="api_key" name="api_key"
                                    value=(form_data.map(|f| f.api_key.as_str()).unwrap_or("")) required;
                            }
                            div class="mb-3" {
                                label for="tournament_slug" class="form-label" { "Tournament Slug" }
                                input type="text" class="form-control" id="tournament_slug" name="tournament_slug"
                                    value=(form_data.map(|f| f.tournament_slug.as_str()).unwrap_or("")) required;
                            }

                            button type="submit" class="btn btn-primary" { "Generate Break Slides" }
                        }
                    }
                }
            }
        }
    }
}

#[derive(FromForm)]
pub struct BreakSlidesForm {
    url: String,
    api_key: String,
    tournament_slug: String,
}

#[derive(:: serde :: Deserialize, :: serde :: Serialize, Clone, Debug)]
struct CBreakingTeam {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_rank: Option<i32>,
    pub rank: i32,
    ///Used to explain why an otherwise-qualified team didn't break
    ///
    /// * `C` - Capped
    /// * `I` - Ineligible
    /// * `D` - Different break
    /// * `d` - Disqualified
    /// * `t` - Lost coin toss
    /// * `w` - Withdrawn
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    pub team: String,
}

#[post("/break-slides", data = "<form>")]
pub async fn do_gen_break_slides(
    form: Form<BreakSlidesForm>,
    user: Option<db::user::User>,
    span: TracingSpan,
    req_id: RequestId,
) -> (Status, Markup) {
    let span1 = span.0.clone();
    // todo: run the data fetching using async executor, rather than on a thread
    rocket::tokio::task::spawn_blocking(move || {
        let _guard = span1.enter();
        let api_addr = form.url.clone();

        let url = format!("{api_addr}/api/v1/tournaments/{}", form.tournament_slug);
        match attohttpc::get(&url)
            .header("Authorization", format!("Token {}", form.api_key))
            .send()
        {
            Err(e) => {
                let error_msg = format!("Failed to retrieve the URL: {} (error: {e:?}). Please check
                                         if the provided link to the tab site is
                                         correct, and that the tournament is correct. (Request ID: {})", url, req_id.to_string());
                return (Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user));
            }
            Ok(response) => {
                if response.status() == attohttpc::StatusCode::NOT_FOUND {
                    let error_msg = format!("Tournament not found (404). URL: {url}. Please check if the tournament slug is correct. (Request ID: {})", req_id.to_string());
                    return (Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user));
                }
                if !response.status().is_success() {
                    let error_msg = format!("HTTP error {}: Please check your API key and tournament slug. (Request ID: {})", response.status(), req_id.to_string());
                    return (Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user));
                }
            }
        }

        let standings: HashMap<String, tabbycat_api::types::TeamStandings> =
            match attohttpc::get(format!(
                "{api_addr}/api/v1/tournaments/{}/teams/standings",
                form.tournament_slug
            ))
            .header("Authorization", format!("Token {}", form.api_key))
            .send()
            .and_then(|response| response.json::<Vec<TeamStandings>>())
            {
                Ok(standings_vec) => standings_vec
                    .into_iter()
                    .map(|standing| (standing.team.clone(), standing))
                    .collect(),
                Err(e) => {
                    let error_msg = format!("Failed to fetch or decode team standings: {}. (Request ID: {})", e, req_id.to_string());
                    return (Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user));
                }
            };

        let teams: HashMap<String, tabbycat_api::types::Team> =
            match attohttpc::get(format!(
                "{api_addr}/api/v1/tournaments/{}/teams",
                form.tournament_slug
            ))
            .header("Authorization", format!("Token {}", form.api_key))
            .send()
            .and_then(|response| response.json::<Vec<tabbycat_api::types::Team>>())
            {
                Ok(teams_vec) => teams_vec
                    .into_iter()
                    .map(|team| (team.url.clone(), team))
                    .collect(),
                Err(e) => {
                    let error_msg = format!("Failed to fetch or decode teams: {}. (Request ID: {})", e, req_id.to_string());
                    return (Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user));
                }
            };

        let mut break_categories: Vec<tabbycat_api::types::BreakCategory> =
            attohttpc::get(format!(
                "{api_addr}/api/v1/tournaments/{}/break-categories",
                form.tournament_slug
            ))
            .header("Authorization", format!("Token {}", form.api_key))
            .send()
            .unwrap()
            .json()
            .unwrap();

        break_categories.sort_by_key(|cat| cat.priority);

        let individual_break_categories: Vec<Markup> = {
            break_categories
                .iter()
                .map(|cat| {
                    let breaking_teams: Vec<CBreakingTeam> =
                        {
                            let response = attohttpc::get(&cat.links.breaking_teams)
                                .header(
                                    "Authorization",
                                    format!("Token {}", form.api_key),
                                )
                                .send()
                                .unwrap();

                            let response_text = response.text().unwrap();
                            match serde_json::from_str::<Vec<CBreakingTeam>>(&response_text) {
                                Ok(teams) => teams,
                                Err(e) => {
                                    tracing::error!("Failed to deserialize breaking teams JSON: {}, data: {}", e, response_text);
                                    panic!("JSON deserialization failed");
                                }
                            }
                        };

                    maud::html! {
                        section {
                            h1 {(format!("{} break", cat.name.as_str()))}
                        }
                        @for team in breaking_teams {
                            section {
                                h3 {(format!("Breaking {}{}", team.rank, add_suffix(team.rank)))}
                                h1 { (teams.get(&team.team).unwrap().long_name) }
                                h3 {
                                    @for (i, metric) in standings.get(&team.team).unwrap().metrics.iter().enumerate() {
                                        ({
                                            let value = match metric.value {
                                                Some(f) => f.to_string(),
                                                _ => "-".to_string()
                                            };
                                            let metric_name = match metric.metric {
                                                Some(m) => name_of_metric(&m),
                                                None => "-".to_string()
                                            };
                                            format!("{}{} {}", if i > 0 {","} else {""}, value, metric_name)
                                        })
                                    }
                                }
                                @if let Some(remark) = team.remark {
                                    i {
                                        (match remark.as_str() {
                                            "C" => "Capped",
                                            "I" => "Ineligible",
                                            "D" => "Different break",
                                            "d" => "Disqualified",
                                            "t" => "Lost coin toss",
                                            "w" => "Withdrawn",
                                            _ => panic!("Unknown remark code: {}", remark),
                                        })
                                    }
                                }
                            }
                        }
                    }
                })
                .collect()
        };

        let slides = maud::html! {
            html {
                head {
                    link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/reveal.js@5.0.4/dist/reveal.css";
                    link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/reveal.js@5.0.4/dist/theme/white.css";
                    style {
                        ".reveal h1, .reveal h2, .reveal h3 { text-transform: none; }"
                        ".reveal .slides section { text-align: center; }"
                    }
                }
                body {
                    div class="reveal" {
                        div class="slides" {
                            section {
                                h1 {
                                    "Break slides!"
                                }
                                h3 {
                                    "Before using the slides, print them as a
                                     PDF (by pressing CMD + P/CTRL + P and then
                                     selecting 'save as PDF')."
                                }
                            }
                            @for category_slides in &individual_break_categories {
                                (category_slides)
                            }
                        }
                    }
                    script src="https://cdn.jsdelivr.net/npm/reveal.js@5.0.4/dist/reveal.js" {}
                    script {
                        "Reveal.initialize({ hash: true, transition: 'slide' });"
                    }
                }
            }
        };

        (Status::Ok, slides)
    })
    .instrument(span.0)
    .await
    .unwrap()
}

fn add_suffix(rank: i32) -> String {
    match rank {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
    .to_string()
}

fn name_of_metric(metric: &TeamStandingsMetricsItemMetric) -> String {
    match metric {
        TeamStandingsMetricsItemMetric::Points => "Points".to_string(),
        TeamStandingsMetricsItemMetric::Wins => "Wins".to_string(),
        TeamStandingsMetricsItemMetric::SpeaksSum => {
            "Speaker Points Sum".to_string()
        }
        TeamStandingsMetricsItemMetric::SpeaksAvg => {
            "Speaker Points Average".to_string()
        }
        TeamStandingsMetricsItemMetric::SpeaksIndAvg => {
            "Individual Speaker Points Average".to_string()
        }
        TeamStandingsMetricsItemMetric::SpeaksStddev => {
            "Speaker Points Standard Deviation".to_string()
        }
        TeamStandingsMetricsItemMetric::DrawStrength => {
            "Draw Strength".to_string()
        }
        TeamStandingsMetricsItemMetric::DrawStrengthSpeaks => {
            "Draw Strength Speaker Points".to_string()
        }
        TeamStandingsMetricsItemMetric::MarginSum => "Margin Sum".to_string(),
        TeamStandingsMetricsItemMetric::MarginAvg => {
            "Margin Average".to_string()
        }
        TeamStandingsMetricsItemMetric::Npullups => {
            "Number of Pullups".to_string()
        }
        TeamStandingsMetricsItemMetric::PullupDebates => {
            "Pullup Debates".to_string()
        }
        TeamStandingsMetricsItemMetric::NumAdjs => {
            "Number of Adjudications".to_string()
        }
        TeamStandingsMetricsItemMetric::Firsts => "Firsts".to_string(),
        TeamStandingsMetricsItemMetric::Seconds => "Seconds".to_string(),
        TeamStandingsMetricsItemMetric::Thirds => "Thirds".to_string(),
        TeamStandingsMetricsItemMetric::NumIron => {
            "Number of Iron Persons".to_string()
        }
        TeamStandingsMetricsItemMetric::Wbw => "Who Beat Whom".to_string(),
    }
}
