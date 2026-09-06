//! AWS Lambda HTTP adapter using `lambda_http`.
//!
//! Wraps the axum `Router` in a `tower::Service` that the Lambda runtime
//! invokes per invocation. Cold-start creates any needed resources; warm
//! invocations reuse them.
//!
//! The adapter converts between `lambda_http`'s `Body` type (used in API
//! Gateway events) and axum's `Body` so the router can process requests
//! normally.
//!
//! **Note:** This adapter is intended for deployment on AWS Lambda (Linux).
//! It uses `tokio::task::spawn_blocking` to run the Lambda runtime, which
//! handles the fact that the runtime's internal future is not `Send`.

use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use axum::body::Body as AxumBody;
use axum::Router;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use lambda_http::{self, service_fn, Body as LambdaBody, Request, Response};
use tower::Service as TowerService;

use crate::{HttpAdapter, Result};

/// HTTP adapter for AWS Lambda via `lambda_http`.
///
/// Each Lambda invocation receives one API Gateway event, converts it to
/// an axum `Request`, dispatches to the `Router`, and converts the
/// response back. Use this adapter when deploying behind API Gateway or
/// an Application Load Balancer.
pub struct LambdaHttpAdapter;

impl LambdaHttpAdapter {
    /// Create a new Lambda adapter.
    ///
    /// Returns a zero-sized adapter — all state (the axum `Router`) is
    /// passed to [`Self::serve`] at startup.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LambdaHttpAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Tower service wrapping the axum `Router` for per-request dispatch.
///
/// Holds a clone of the router and implements `Service<Request>` so that
/// the Lambda runtime can call it for each incoming API Gateway event.
pub struct RouterService {
    router: Router,
}

impl TowerService<Request> for RouterService {
    type Response = Response<LambdaBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = std::result::Result<Response<LambdaBody>, Infallible>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut router = self.router.clone();

        Box::pin(async move {
            let (parts, lambda_body) = req.into_parts();

            let body_bytes: Bytes = match lambda_body {
                LambdaBody::Empty => Bytes::new(),
                LambdaBody::Text(s) => Bytes::from(s.into_bytes()),
                LambdaBody::Binary(b) => Bytes::from(b),
                _ => Bytes::new(),
            };

            let axum_req = http::Request::from_parts(parts, AxumBody::new(Full::new(body_bytes)));

            // axum::Router's Service::Error is Infallible, so this always succeeds
            let response = TowerService::call(&mut router, axum_req).await.unwrap();

            let (resp_parts, resp_body) = response.into_parts();

            let body_bytes = match resp_body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(e) => {
                    tracing::error!("failed to collect response body: {:?}", e);
                    return Ok(make_error_response(500, "internal server error"));
                }
            };

            let lambda_body = LambdaBody::Binary(body_bytes.to_vec());

            // Response::builder with a valid status and body never fails
            let mut resp = http::Response::builder()
                .status(resp_parts.status)
                .body(lambda_body)
                .unwrap();
            *resp.headers_mut() = resp_parts.headers;
            Ok(resp)
        })
    }
}

/// Build an HTTP error response with the given status and message.
fn make_error_response(status: u16, message: &'static str) -> Response<LambdaBody> {
    http::Response::builder()
        .status(status)
        .body(LambdaBody::from(message))
        .unwrap()
}

/// Tower service that delegates to a [`RouterService`].
///
/// Implements `Service<Request>` so it can be passed to `lambda_http::run`.
pub struct LambdaHandler {
    service: RouterService,
}

impl LambdaHandler {
    /// Create a new handler from an axum router.
    pub fn new(router: Router) -> Self {
        Self {
            service: RouterService { router },
        }
    }
}

impl TowerService<Request> for LambdaHandler {
    type Response = Response<LambdaBody>;
    type Error = Infallible;
    type Future = <RouterService as TowerService<Request>>::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        self.service.call(req)
    }
}

impl Clone for LambdaHandler {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
        }
    }
}

impl Clone for RouterService {
    fn clone(&self) -> Self {
        Self {
            router: self.router.clone(),
        }
    }
}

impl HttpAdapter for LambdaHttpAdapter {
    /// Start serving requests through the Lambda runtime.
    ///
    /// Blocks until the platform signals shutdown or an unrecoverable
    /// error occurs. Returns `Ok(())` on clean shutdown.
    ///
    /// This implementation uses `tokio::task::spawn_blocking` to run the
    /// Lambda runtime on a separate thread, which resolves a `Send`-bound
    /// incompatibility between the runtime's internal future and the
    /// `HttpAdapter` trait.
    async fn serve(self, router: Router) -> Result<()> {
        let handler = LambdaHandler::new(router);

        // Run the Lambda runtime on a blocking thread. The lambda_http::run
        // future is not Send due to internal pin! usage, so we isolate it on
        // a dedicated thread via spawn_blocking.
        let result = tokio::task::spawn_blocking(move || {
            // Build a fresh tokio runtime inside the spawned thread so that
            // lambda_http::run has a runtime context to drive its timers.
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| anyhow::anyhow!("failed to create tokio runtime: {}", e))?;

            rt.block_on(async {
                lambda_http::run(service_fn(move |req: Request| {
                    let mut handler = handler.clone();
                    async move { handler.call(req).await }
                }))
                .await
                .map_err(|e| anyhow::anyhow!("lambda runtime error: {}", e))
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking panicked: {}", e))??;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use std::task::Context;

    #[test]
    fn lambda_handler_poll_ready() {
        let router = Router::new();
        let mut handler = LambdaHandler::new(router);
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        let result = handler.poll_ready(&mut cx);
        assert!(matches!(result, Poll::Ready(Ok(()))));
    }

    #[tokio::test]
    async fn router_service_call_returns_response() {
        let router = Router::new().route("/health", get(|| async { "healthy" }));
        let mut service = RouterService { router };

        let mut req = lambda_http::Request::default();
        *req.uri_mut() = "https://example.com/health".parse().unwrap();
        let resp = service.call(req).await.unwrap();

        assert_eq!(resp.status(), 200);
        // Body is always Binary (the adapter emits Bytes regardless of content-type)
        if let LambdaBody::Binary(body) = resp.body() {
            assert_eq!(body, b"healthy");
        } else {
            panic!("expected Binary body, got {:?}", resp.body());
        }
    }
}
