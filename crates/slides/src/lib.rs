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
                                div class="form-check" {
                                    label class="form-check-label" for="descending_order" {
                                        "Generate from highest to lowest priority (usually this
                                         means that ticking this box will generate slides
                                         in the order Open -> ESL -> EFL
                                         rather than the default EFL -> ESL -> Open, but
                                         this depends on how you have configured the
                                         priority of the different break categories on
                                         Tabbycat)."
                                    }
                                    @if form_data.as_ref().map(|f| f.descending_order).unwrap_or(false) {
                                        input class="form-check-input" type="checkbox" value="true" id="descending_order" name="descending_order" checked {}
                                    } @else {
                                        input class="form-check-input" type="checkbox" value="true" id="descending_order" name="descending_order" {}
                                    }
                                }
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
    descending_order: bool,
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
    let form_data = form.into_inner();
    let api_addr = form_data.url.clone();
    let api_key = form_data.api_key.clone();
    let tournament_slug = form_data.tournament_slug.clone();
    let template = form_data.template.clone();
    let user_clone = user.clone();

    let client = reqwest::Client::new();
    let auth_header = format!("Token {}", api_key);

    let url = format!("{api_addr}/api/v1/tournaments/{tournament_slug}");
    let tournament_result = client
        .get(&url)
        .header("Authorization", &auth_header)
        .send()
        .await;

    let tournament = match tournament_result {
        Err(e) => {
            let error_msg = format!("Failed to retrieve the URL: {} (error: {e:?}). Please check
                                     if the provided link to the tab site is
                                     correct, and that the tournament is correct. (Request ID: {})", url, req_id.to_string());
            return Err((
                Status::BadRequest,
                ui::page_of_body(
                    make_form(Some(error_msg), Some(&form_data)),
                    user,
                ),
            ));
        }
        Ok(response) => {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                let error_msg = format!(
                    "Tournament not found (404). URL: {url}. Please check if the tournament slug is correct. (Request ID: {})",
                    req_id.to_string()
                );
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(error_msg), Some(&form_data)),
                        user,
                    ),
                ));
            }
            if !response.status().is_success() {
                let error_msg = format!(
                    "HTTP error {}: Please check your API key and tournament slug. (Request ID: {})",
                    response.status(),
                    req_id.to_string()
                );
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(error_msg), Some(&form_data)),
                        user,
                    ),
                ));
            }
            match response.json::<tabbycat_api::types::Tournament>().await {
                Ok(t) => t,
                Err(e) => {
                    let error_msg = format!(
                        "Failed to decode tournament data: {}. (Request ID: {})",
                        e,
                        req_id.to_string()
                    );
                    return Err((
                        Status::BadRequest,
                        ui::page_of_body(
                            make_form(Some(error_msg), Some(&form_data)),
                            user,
                        ),
                    ));
                }
            }
        }
    };

    let standings_result = client
        .get(format!(
            "{api_addr}/api/v1/tournaments/{tournament_slug}/teams/standings"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await;

    let standings: HashMap<String, tabbycat_api::types::TeamStandings> =
        match standings_result {
            Ok(response) => {
                if !response.status().is_success() {
                    let error_msg = format!(
                        "HTTP error {}: Failed to fetch team standings. (Request ID: {})",
                        response.status(),
                        req_id.to_string()
                    );
                    return Err((
                        Status::BadRequest,
                        ui::page_of_body(
                            make_form(Some(error_msg), Some(&form_data)),
                            user,
                        ),
                    ));
                }
                match response.json::<Vec<TeamStandings>>().await {
                    Ok(standings_vec) => standings_vec
                        .into_iter()
                        .map(|standing| (standing.team.clone(), standing))
                        .collect(),
                    Err(e) => {
                        let error_msg = format!(
                            "Failed to decode team standings: {}. (Request ID: {})",
                            e,
                            req_id.to_string()
                        );
                        return Err((
                            Status::BadRequest,
                            ui::page_of_body(
                                make_form(Some(error_msg), Some(&form_data)),
                                user,
                            ),
                        ));
                    }
                }
            }
            Err(e) => {
                let error_msg = format!(
                    "Failed to fetch team standings: {}. (Request ID: {})",
                    e,
                    req_id.to_string()
                );
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(error_msg), Some(&form_data)),
                        user,
                    ),
                ));
            }
        };

    let adjudicators_result = client
        .get(format!(
            "{api_addr}/api/v1/tournaments/{tournament_slug}/adjudicators"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await;

    let mut adjudicators: Vec<Adjudicator> = match adjudicators_result {
        Ok(response) => {
            if !response.status().is_success() {
                let error_msg = format!(
                    "HTTP error {}: Failed to fetch adjudicators. (Request ID: {})",
                    response.status(),
                    req_id.to_string()
                );
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(error_msg), Some(&form_data)),
                        user,
                    ),
                ));
            }
            match response.json::<Vec<Adjudicator>>().await {
                Ok(judges) => judges
                    .into_iter()
                    .filter(|judge| judge.breaking.unwrap_or(false))
                    .collect(),
                Err(e) => {
                    let error_msg = format!(
                        "Failed to decode adjudicators: {}. (Request ID: {})",
                        e,
                        req_id.to_string()
                    );
                    return Err((
                        Status::BadRequest,
                        ui::page_of_body(
                            make_form(Some(error_msg), Some(&form_data)),
                            user,
                        ),
                    ));
                }
            }
        }
        Err(e) => {
            let error_msg = format!(
                "Failed to fetch adjudicators: {}. (Request ID: {})",
                e,
                req_id.to_string()
            );
            return Err((
                Status::BadRequest,
                ui::page_of_body(
                    make_form(Some(error_msg), Some(&form_data)),
                    user,
                ),
            ));
        }
    };

    adjudicators.sort_by_key(|judge| judge.name.clone());

    let teams_result = client
        .get(format!(
            "{api_addr}/api/v1/tournaments/{tournament_slug}/teams"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await;

    let teams: HashMap<String, tabbycat_api::types::Team> = match teams_result {
        Ok(response) => {
            if !response.status().is_success() {
                let error_msg = format!(
                    "HTTP error {}: Failed to fetch teams. (Request ID: {})",
                    response.status(),
                    req_id.to_string()
                );
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(error_msg), Some(&form_data)),
                        user,
                    ),
                ));
            }
            match response.json::<Vec<tabbycat_api::types::Team>>().await {
                Ok(teams_vec) => teams_vec
                    .into_iter()
                    .map(|team| (team.url.clone(), team))
                    .collect(),
                Err(e) => {
                    let error_msg = format!(
                        "Failed to decode teams: {}. (Request ID: {})",
                        e,
                        req_id.to_string()
                    );
                    return Err((
                        Status::BadRequest,
                        ui::page_of_body(
                            make_form(Some(error_msg), Some(&form_data)),
                            user,
                        ),
                    ));
                }
            }
        }
        Err(e) => {
            let error_msg = format!(
                "Failed to fetch teams: {}. (Request ID: {})",
                e,
                req_id.to_string()
            );
            return Err((
                Status::BadRequest,
                ui::page_of_body(
                    make_form(Some(error_msg), Some(&form_data)),
                    user,
                ),
            ));
        }
    };

    let break_categories_result = client
        .get(format!(
            "{api_addr}/api/v1/tournaments/{tournament_slug}/break-categories"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await;

    let mut break_categories = match break_categories_result {
        Ok(response) => {
            if !response.status().is_success() {
                let error_msg = format!(
                    "HTTP error {}: Failed to fetch break categories. (Request ID: {})",
                    response.status(),
                    req_id.to_string()
                );
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(error_msg), Some(&form_data)),
                        user,
                    ),
                ));
            }
            match response
                .json::<Vec<tabbycat_api::types::BreakCategory>>()
                .await
            {
                Ok(categories) => categories,
                Err(e) => {
                    let error_msg = format!(
                        "Failed to decode break categories: {}. (Request ID: {})",
                        e,
                        req_id.to_string()
                    );
                    return Err((
                        Status::BadRequest,
                        ui::page_of_body(
                            make_form(Some(error_msg), Some(&form_data)),
                            user,
                        ),
                    ));
                }
            }
        }
        Err(e) => {
            let error_msg = format!(
                "Failed to fetch break categories: {}. (Request ID: {})",
                e,
                req_id.to_string()
            );
            return Err((
                Status::BadRequest,
                ui::page_of_body(
                    make_form(Some(error_msg), Some(&form_data)),
                    user,
                ),
            ));
        }
    };

    if !form_data.descending_order {
        break_categories.sort_by_key(|cat| cat.priority);
    } else {
        break_categories.sort_by_key(|cat| -cat.priority);
    }

    let mut individual_break_categories = IndexMap::new();
    for cat in &break_categories {
        let breaking_teams_result = client
            .get(&cat.links.breaking_teams)
            .header("Authorization", &auth_header)
            .send()
            .await;

        match breaking_teams_result {
            Ok(response) => {
                if !response.status().is_success() {
                    let error_msg = format!(
                        "HTTP error {}: Failed to fetch breaking teams for category {}. (Request ID: {})",
                        response.status(),
                        cat.name.as_str(),
                        req_id.to_string()
                    );
                    return Err((
                        Status::BadRequest,
                        ui::page_of_body(
                            make_form(Some(error_msg), Some(&form_data)),
                            user,
                        ),
                    ));
                }

                let response_text = match response.text().await {
                    Ok(text) => text,
                    Err(e) => {
                        let error_msg = format!(
                            "Failed to get response text for breaking teams: {}. (Request ID: {})",
                            e,
                            req_id.to_string()
                        );
                        return Err((
                            Status::BadRequest,
                            ui::page_of_body(
                                make_form(Some(error_msg), Some(&form_data)),
                                user,
                            ),
                        ));
                    }
                };

                let breaking_teams = match serde_json::from_str::<
                    Vec<CBreakingTeam>,
                >(&response_text)
                {
                    Ok(teams) => teams,
                    Err(e) => {
                        tracing::error!(
                            "Failed to deserialize breaking teams JSON: {}, data: {}",
                            e,
                            response_text
                        );
                        let error_msg = format!(
                            "Failed to deserialize breaking teams data. (Request ID: {})",
                            req_id.to_string()
                        );
                        return Err((
                            Status::BadRequest,
                            ui::page_of_body(
                                make_form(Some(error_msg), Some(&form_data)),
                                user,
                            ),
                        ));
                    }
                };

                let breaking_team_contexts: Vec<BreakingTeamCtx> =
                    breaking_teams
                        .into_iter()
                        .filter_map(|breaking_team| {
                            let team = teams
                                .values()
                                .find(|cmp| cmp.url == breaking_team.team)?;
                            let standings_data =
                                standings.get(&breaking_team.team)?;

                            Some(BreakingTeamCtx {
                                break_rank: breaking_team.break_rank,
                                rank: breaking_team.rank,
                                remark: breaking_team
                                    .remark
                                    .map(|r| name_of_remark(&r)),
                                team: team.clone(),
                                metrics: standings_data
                                    .metrics
                                    .iter()
                                    .filter_map(|metric| {
                                        let metric_name =
                                            metric.metric.as_ref()?;
                                        let value = metric.value?;
                                        Some(BreakingTeamMetricsCtx {
                                            metric: name_of_metric(metric_name),
                                            value,
                                        })
                                    })
                                    .collect(),
                            })
                        })
                        .collect();

                individual_break_categories.insert(
                    cat.name.as_str().to_string(),
                    breaking_team_contexts,
                );
            }
            Err(e) => {
                let error_msg = format!(
                    "Failed to fetch breaking teams for category {}: {}. (Request ID: {})",
                    cat.name.as_str(),
                    e,
                    req_id.to_string()
                );
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(error_msg), Some(&form_data)),
                        user,
                    ),
                ));
            }
        }
    }

    let ctx = BreakSlidesCtx {
        tournament_name: tournament.name.as_str().to_string(),
        categories: individual_break_categories,
        adjudicators,
    };

    // Use the template
    let template_content = match &template {
        None => DEFAULT_TEMPLATE.to_string(),
        Some(t) if t.trim().is_empty() => DEFAULT_TEMPLATE.to_string(),
        Some(t) => t.to_string(),
    };

    let json = match serde_json::to_string(&ctx) {
        Ok(j) => j,
        Err(e) => {
            let error_msg = format!(
                "Failed to serialize context data: {}. (Request ID: {})",
                e,
                req_id.to_string()
            );
            return Err((
                Status::BadRequest,
                ui::page_of_body(
                    make_form(Some(error_msg), Some(&form_data)),
                    user,
                ),
            ));
        }
    };

    let span1 = span.0.clone();
    rocket::tokio::task::spawn_blocking(move || {
        let _guard = span1.enter();
        let world = TypstWrapperWorld::new(template_content)
            .add_file("break.json", json);

        // Render document
        let document = match typst::compile(&world).output {
            Ok(doc) => doc,
            Err(e) => {
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(format!("{:?}", e)), Some(&form_data)),
                        user_clone,
                    ),
                ));
            }
        };

        let pdf = match typst_pdf::pdf(&document, &PdfOptions::default()) {
            Ok(pdf) => pdf,
            Err(e) => {
                return Err((
                    Status::BadRequest,
                    ui::page_of_body(
                        make_form(Some(format!("{:?}", e)), Some(&form_data)),
                        user_clone,
                    ),
                ));
            }
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
