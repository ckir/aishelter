use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;
use worker::{Env, HttpRequest, HttpResponse, Result, event};

fn router() -> Router {
    Router::new().route("/health", axum::routing::get(|| async { "ok" })).route(
        "/version",
        axum::routing::get(|| async { "{\"version\":\"0.3.0\",\"protocol\":\"acp/1\"}" }),
    )
}

#[event(fetch)]
async fn fetch(req: HttpRequest, _env: Env, _ctx: worker::Context) -> Result<HttpResponse> {
    let axum_resp = router().oneshot(req).await.map_err(|e| worker::Error::from(e.to_string()))?;

    let (parts, body) = axum_resp.into_parts();
    let bytes = body.collect().await.map_err(|e| worker::Error::from(e.to_string()))?.to_bytes();

    let worker_body = worker::Body::from_stream(futures_util::stream::once(async move {
        Ok::<_, std::io::Error>(bytes.to_vec())
    }))?;
    Ok(HttpResponse::from_parts(parts, worker_body))
}
