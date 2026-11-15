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
use crate::period::{HistoryPeriod, PeriodError, PeriodQuery};
use crate::svg::{SvgOptions, generate_contribution_graph};

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
        .route("/history", get(history))
        .route("/agent-history", get(agent_history))
        .route("/time-of-day", get(time_of_day))
        .route("/graph.svg", get(graph))
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

async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Response, ApiError> {
    let period = query.parse_history(timezone(&headers, state.default_timezone)?)?;
    Ok(cached_json(&state, state.history.buckets(&period).await))
}

async fn agent_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Response, ApiError> {
    let period = query.parse_history(timezone(&headers, state.default_timezone)?)?;
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
    show_month_labels: Option<String>,
    show_day_labels: Option<String>,
}

async fn graph(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GraphQuery>,
) -> Result<Response, ApiError> {
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
    let period = query
        .period
        .parse(timezone(&headers, state.default_timezone)?)?;
    let data = state
        .history
        .buckets(&HistoryPeriod {
            period,
            rollup_seconds: 86_400,
        })
        .await;
    let svg = generate_contribution_graph(
        &data,
        &SvgOptions {
            cell_size,
            cell_gap,
            show_month_labels: parse_boolean(query.show_month_labels.as_deref(), true),
            show_day_labels: parse_boolean(query.show_day_labels.as_deref(), true),
            base_color: query
                .color
                .or(query.base_color)
                .unwrap_or_else(|| "#fb7185".to_owned()),
            text_color: query.text_color.unwrap_or_else(|| "#57606a".to_owned()),
            cell_background: query
                .cell_background
                .unwrap_or_else(|| "#ebedf0".to_owned()),
        },
    );
    let mut response = svg.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    cache_header(&state, &mut response);
    Ok(response)
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

fn parse_boolean(value: Option<&str>, default: bool) -> bool {
    value.map_or(default, |value| matches!(value, "true" | "1"))
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
    use axum::body::Body;
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
            .oneshot(Request::get("/history").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "max-age=300");

        let response = app()
            .clone()
            .oneshot(Request::post("/history").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        let response = app()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/history")
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
    async fn rejects_unsafe_graph_dimensions() {
        let response = app()
            .oneshot(
                Request::get("/graph.svg?cellSize=4294967295")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
