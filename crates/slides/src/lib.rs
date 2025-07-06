use std::collections::HashMap;

mod typst_as_library;

use indexmap::IndexMap;
use maud::Markup;
use rocket::{
    form::Form,
    http::{ContentType, Status},
};
use serde::{Deserialize, Serialize};
use tabbycat_api::types::{
    Adjudicator, TeamStandings, TeamStandingsMetricsItemMetric,
};
use tracing::Instrument;

#[macro_use]
extern crate rocket;

use trace_request::{RequestId, TracingSpan};
use typst_pdf::PdfOptions;

use crate::typst_as_library::TypstWrapperWorld;

const DEFAULT_TEMPLATE: &'static str = include_str!("break_slides.typst");

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
                        div class="alert alert-info" {
                            "Note: please save the break slides once you have
                             generated them, as once you leave (or refresh) the
                             page with the generated slides, you will need to
                             generate them again from scratch (which can take a
                             few moments)."
                        }
                        form method="post" {
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
                            div class="mb-3" {
                                label for="template" class="form-label" {
                                    "Custom template (optional). If you don't provide
                                     a template, then the default one will be used.
                                     Templates must be written in the "
                                     a href="https://typst.app" {"Typst"}
                                     " language. "
                                     "The "
                                     a href="https://raw.githubusercontent.com/teymour-aldridge/shorthair/refs/heads/main/crates/slides/src/break_slides.typst" {
                                        "source code of the default template"
                                     }
                                     " is available."
                                }
                                textarea class="form-control" id="template" name="template" rows="5" placeholder="Leave empty to use the default template" {
                                    (form_data.map(|f| f.template.clone().unwrap_or_default()).unwrap_or_default())
                                }
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
    template: Option<String>,
}

#[derive(Serialize)]
pub struct BreakSlidesCtx {
    tournament_name: String,
    categories: IndexMap<String, Vec<BreakingTeamCtx>>,
    adjudicators: Vec<Adjudicator>,
}

#[derive(Serialize)]
pub struct BreakingTeamCtx {
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
    pub team: tabbycat_api::types::Team,
    pub metrics: Vec<BreakingTeamMetricsCtx>,
}

#[derive(Serialize)]
pub struct BreakingTeamMetricsCtx {
    metric: String,
    value: f64,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
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
) -> Result<(ContentType, Vec<u8>), (Status, Markup)> {
    let span1 = span.0.clone();
    // todo: run the data fetching using async executor, rather than on a thread
    rocket::tokio::task::spawn_blocking(move || {
        let _guard = span1.enter();
        let api_addr = form.url.clone();

        let url = format!("{api_addr}/api/v1/tournaments/{}", form.tournament_slug);
        let tournament = match attohttpc::get(&url)
            .header("Authorization", format!("Token {}", form.api_key))
            .send()
        {
            Err(e) => {
                let error_msg = format!("Failed to retrieve the URL: {} (error: {e:?}). Please check
                                         if the provided link to the tab site is
                                         correct, and that the tournament is correct. (Request ID: {})", url, req_id.to_string());
                return Err((Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user)));
            }
            Ok(response) => {
                if response.status() == attohttpc::StatusCode::NOT_FOUND {
                    let error_msg = format!("Tournament not found (404). URL: {url}. Please check if the tournament slug is correct. (Request ID: {})", req_id.to_string());
                    return Err((Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user)));
                }
                if !response.status().is_success() {
                    let error_msg = format!("HTTP error {}: Please check your API key and tournament slug. (Request ID: {})", response.status(), req_id.to_string());
                    return Err((Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user)));
                }
                response.json::<tabbycat_api::types::Tournament>().unwrap()
            }
        };

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
                    return Err((Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user)));
                }
            };

        let mut adjudicators: Vec<tabbycat_api::types::Adjudicator> =
            match attohttpc::get(format!(
                "{api_addr}/api/v1/tournaments/{}/adjudicators",
                form.tournament_slug
            ))
            .header("Authorization", format!("Token {}", form.api_key))
            .send()
            .and_then(|response| response.json::<Vec<Adjudicator>>())
            {
                Ok(judges) => judges.into_iter().filter(|judge| {
                    judge.breaking.unwrap_or(false)
                }).collect(),
                Err(e) => {
                    let error_msg = format!("Failed to fetch or decode team standings: {}. (Request ID: {})", e, req_id.to_string());
                    return Err((Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user)));
                }
            };

        adjudicators.sort_by_key(|judge| {
            judge.name.clone()
        });

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
                    return Err((Status::BadRequest, ui::page_of_body(make_form(Some(error_msg), Some(&form)), user)));
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

        let individual_break_categories: IndexMap<String, Vec<BreakingTeamCtx>> = {
            break_categories
                .iter()
                .map(|cat| {
                    let response = attohttpc::get(&cat.links.breaking_teams)
                        .header(
                            "Authorization",
                            format!("Token {}", form.api_key),
                        )
                        .send()
                        .unwrap();

                    let response_text = response.text().unwrap();
                    let b = match serde_json::from_str::<Vec<CBreakingTeam>>(&response_text) {
                        Ok(teams) => teams,
                        Err(e) => {
                            tracing::error!("Failed to deserialize breaking teams JSON: {}, data: {}", e, response_text);
                            panic!("JSON deserialization failed");
                        }
                    };

                    (
                        cat.name.as_str().to_string(),
                        b.into_iter().map(|breaking_team| {
                            BreakingTeamCtx {
                                break_rank: breaking_team.break_rank,
                                rank: breaking_team.rank,
                                remark: breaking_team.remark.map(|r| name_of_remark(&r)),
                                team: teams.values().find(|cmp| cmp.url == breaking_team.team).expect("failed to find team").clone(),
                                metrics: standings
                                    .get(&breaking_team.team)
                                    .unwrap()
                                    .metrics
                                    .iter()
                                    .map(|metric| {
                                        BreakingTeamMetricsCtx {
                                            metric: name_of_metric(metric.metric.as_ref().unwrap()),
                                            value: metric.value.unwrap(),
                                        }
                                    })
                                    .collect(),
                            }
                        }).collect()
                    )
                })
                .collect()
        };

        let ctx = BreakSlidesCtx { tournament_name: tournament.name.as_str().to_string(), categories: individual_break_categories, adjudicators };

        let template = match &form.template {
            None => {
                DEFAULT_TEMPLATE.to_string()
            }
            Some(t) if t.trim().len() == 0 => {
                DEFAULT_TEMPLATE.to_string()
            }
            Some(t) => t.to_string(),
        };

        let json = serde_json::to_string(&ctx).unwrap();
        let world = TypstWrapperWorld::new(template).add_file("break.json", json);

        // Render document
        let document = match typst::compile(&world)
            .output {
                Ok(doc) => doc,
                Err(e) => {
                    return Err((Status::BadRequest, ui::page_of_body(
                        make_form(Some(format!("{:?}", e)), Some(&form)), user)
                    ));
                },
            };

        let pdf = match typst_pdf::pdf(&document, &PdfOptions::default()) {
            Ok(pdf) => pdf,
            Err(e) => {
                return Err((Status::BadRequest, ui::page_of_body(
                    make_form(Some(format!("{:?}", e)), Some(&form)), user)
                ));
            },
        };

        Ok((ContentType::PDF, pdf))
    })
    .instrument(span.0)
    .await
    .unwrap()
}

fn name_of_remark(remark: &str) -> String {
    match remark {
        "C" => "Capped".to_string(),
        "I" => "Ineligible".to_string(),
        "D" => "Different break".to_string(),
        "d" => "Disqualified".to_string(),
        "t" => "Lost coin toss".to_string(),
        "w" => "Withdrawn".to_string(),
        _ => remark.to_string(),
    }
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
