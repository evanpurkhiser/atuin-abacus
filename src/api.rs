use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::agents::AgentStore;
use crate::history::HistoryStore;
use crate::period::{PeriodError, PeriodQuery, TimeSeriesPeriod};
use crate::svg::{ContributionDay, GraphLabels, SvgOptions, generate_contribution_graph};

const SHELL_GRAPH_LABELS: GraphLabels = GraphLabels {
    aria_label: "Shell history contribution graph",
    singular: "command",
    plural: "commands",
};
const AGENT_GRAPH_LABELS: GraphLabels = GraphLabels {
    aria_label: "Agent prompt contribution graph",
    singular: "prompt",
    plural: "prompts",
};

#[derive(Clone)]
pub struct AppState {
    pub history: HistoryStore,
    pub agents: AgentStore,
    pub cache_ttl_seconds: u64,
    pub default_timezone: Tz,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(stats))
        .route("/shell-history", get(shell_history))
        .route("/agent-history", get(agent_history))
        .route("/time-of-day", get(time_of_day))
        .route("/shell-graph.svg", get(shell_graph))
        .route("/agent-graph.svg", get(agent_graph))
        .fallback(not_found)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn stats(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Response, ApiError> {
    let period = query.parse(timezone(&headers, state.default_timezone)?)?;
    Ok(cached_json(&state, state.history.stats(&period).await))
}

async fn shell_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Response, ApiError> {
    let period = query.parse_time_series(timezone(&headers, state.default_timezone)?)?;
    Ok(cached_json(&state, state.history.buckets(&period).await))
}

async fn agent_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Response, ApiError> {
    let period = query.parse_time_series(timezone(&headers, state.default_timezone)?)?;
    Ok(cached_json(&state, state.agents.buckets(&period).await))
}

async fn time_of_day(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Response, ApiError> {
    let period = query.parse(timezone(&headers, state.default_timezone)?)?;
    Ok(cached_json(
        &state,
        state.history.time_of_day(&period).await,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQuery {
    #[serde(flatten)]
    period: PeriodQuery,
    color: Option<String>,
    base_color: Option<String>,
    text_color: Option<String>,
    cell_background: Option<String>,
    cell_size: Option<u32>,
    cell_gap: Option<u32>,
    show_month_labels: Option<bool>,
    show_day_labels: Option<bool>,
}

async fn shell_graph(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GraphQuery>,
) -> Result<Response, ApiError> {
    let (period, options) = parse_graph_request(
        query,
        timezone(&headers, state.default_timezone)?,
        SHELL_GRAPH_LABELS,
    )?;
    let data = state
        .history
        .buckets(&period)
        .await
        .into_iter()
        .map(|bucket| ContributionDay {
            date: bucket.date,
            count: bucket.count,
        })
        .collect::<Vec<_>>();

    Ok(graph_response(&state, &data, &options))
}

async fn agent_graph(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GraphQuery>,
) -> Result<Response, ApiError> {
    let (period, options) = parse_graph_request(
        query,
        timezone(&headers, state.default_timezone)?,
        AGENT_GRAPH_LABELS,
    )?;
    let data = state
        .agents
        .buckets(&period)
        .await
        .into_iter()
        .filter(|bucket| bucket.prompts > 0)
        .map(|bucket| ContributionDay {
            date: bucket.date,
            count: bucket.prompts,
        })
        .collect::<Vec<_>>();

    Ok(graph_response(&state, &data, &options))
}

fn parse_graph_request(
    query: GraphQuery,
    timezone: Tz,
    labels: GraphLabels,
) -> Result<(TimeSeriesPeriod, SvgOptions), ApiError> {
    let cell_size = query.cell_size.unwrap_or(12);
    if !(1..=64).contains(&cell_size) {
        return Err(ApiError::BadRequest(
            "cellSize must be between 1 and 64".to_owned(),
        ));
    }
    let cell_gap = query.cell_gap.unwrap_or(3);
    if cell_gap > 64 {
        return Err(ApiError::BadRequest(
            "cellGap must be between 0 and 64".to_owned(),
        ));
    }
    let period = query.period.parse(timezone)?;

    Ok((
        TimeSeriesPeriod {
            period,
            rollup_seconds: 86_400,
        },
        SvgOptions {
            cell_size,
            cell_gap,
            show_month_labels: query.show_month_labels.unwrap_or(true),
            show_day_labels: query.show_day_labels.unwrap_or(true),
            base_color: query
                .color
                .or(query.base_color)
                .unwrap_or_else(|| "#fb7185".to_owned()),
            text_color: query.text_color.unwrap_or_else(|| "#57606a".to_owned()),
            cell_background: query
                .cell_background
                .unwrap_or_else(|| "#ebedf0".to_owned()),
            labels,
        },
    ))
}

fn graph_response(state: &AppState, data: &[ContributionDay], options: &SvgOptions) -> Response {
    let svg = generate_contribution_graph(data, options);
    let mut response = svg.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    cache_header(state, &mut response);
    response
}

async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"})))
}

fn timezone(headers: &HeaderMap, default: Tz) -> Result<Tz, ApiError> {
    let Some(prefer) = headers.get("prefer").and_then(|value| value.to_str().ok()) else {
        return Ok(default);
    };
    let requested = prefer.split([',', ';']).find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        name.eq_ignore_ascii_case("timezone")
            .then(|| value.trim_matches('"'))
    });
    let Some(requested) = requested else {
        return Ok(default);
    };

    requested
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("Invalid timezone: {requested}")))
}

fn cached_json<T: Serialize>(state: &AppState, value: T) -> Response {
    let mut response = Json(value).into_response();
    cache_header(state, &mut response);
    response
}

fn cache_header(state: &AppState, response: &mut Response) {
    let value = format!("max-age={}", state.cache_ttl_seconds);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_str(&value).expect("numeric cache duration is a valid header value"),
    );
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
}

impl From<PeriodError> for ApiError {
    fn from(error: PeriodError) -> Self {
        Self::BadRequest(error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, Json(json!({"error": message}))).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    fn app() -> Router {
        router(AppState {
            history: HistoryStore::with_timestamps(Vec::<i64>::new()),
            agents: AgentStore::empty(),
            cache_ttl_seconds: 300,
            default_timezone: chrono_tz::America::New_York,
        })
    }

    fn app_with_agent_events(
        prompts: impl IntoIterator<Item = i64>,
        responses: impl IntoIterator<Item = i64>,
    ) -> Router {
        router(AppState {
            history: HistoryStore::with_timestamps(Vec::<i64>::new()),
            agents: AgentStore::with_event_timestamps(prompts, responses),
            cache_ttl_seconds: 300,
            default_timezone: chrono_tz::UTC,
        })
    }

    #[test]
    fn parses_quoted_prefer_timezone() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "prefer",
            HeaderValue::from_static("respond-async, timezone=\"America/New_York\""),
        );
        assert_eq!(
            timezone(&headers, chrono_tz::UTC).unwrap(),
            chrono_tz::America::New_York
        );
    }

    #[test]
    fn rejects_unknown_timezone() {
        let mut headers = HeaderMap::new();
        headers.insert("prefer", HeaderValue::from_static("timezone=Middle/Earth"));
        assert!(matches!(
            timezone(&headers, chrono_tz::UTC),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn handles_methods_cache_and_cors() {
        let response = app()
            .clone()
            .oneshot(Request::get("/shell-history").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "max-age=300");

        let response = app()
            .clone()
            .oneshot(Request::post("/shell-history").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        let response = app()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/shell-history")
                    .header(header::ORIGIN, "https://example.com")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn rejects_invalid_graph_options() {
        let dimensions = app()
            .clone()
            .oneshot(
                Request::get("/shell-graph.svg?cellSize=4294967295")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let labels = app()
            .oneshot(
                Request::get("/agent-graph.svg?showMonthLabels=1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(dimensions.status(), StatusCode::BAD_REQUEST);
        assert_eq!(labels.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn agent_graph_uses_only_user_prompts() {
        let response = app_with_agent_events(
            [1_704_067_200_000],
            [1_704_067_260_000, 1_704_067_320_000, 1_704_153_600_000],
        )
        .oneshot(
            Request::get("/agent-graph.svg?start=2024-01-01&end=2024-01-02")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let svg = String::from_utf8(body.to_vec()).unwrap();

        assert!(svg.contains("Agent prompt contribution graph"));
        assert!(svg.contains("2024-01-01: 1"));
        assert!(svg.contains("1 prompt over 1 day"));
        assert!(!svg.contains("2024-01-02"));
    }
}
