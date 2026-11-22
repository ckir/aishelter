//! Integration test harness.
//!
//! Provides [`TestApp`] which spins up a PostgreSQL container via testcontainers,
//! runs `sqlx` migrations, builds the axum application via `ac_server::server::create_app`,
//! and exposes convenience methods for issuing requests against the in-process server.

use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use sqlx::PgPool;
use sqlx::migrate::Migrator;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;

use ac_crypto::keypair::AgentKeypair;
use ac_crypto::signature::{
    SignedRequest, X_AGENT_ID, X_AGENT_NONCE, X_AGENT_SIGNATURE, X_AGENT_TIMESTAMP,
};
use ac_metrics::registry::Metrics;
use ac_server::middleware::rate_limit::RateLimiter;

/// In-process test application wrapping an axum [`Router`].
///
/// A `TestApp` owns a PostgreSQL test container and a connection pool.
/// It is created via [`TestApp::setup`] and provides HTTP convenience
/// methods that issue requests through `tower::ServiceExt::oneshot`.
pub struct TestApp {
    pool: PgPool,
    app: axum::Router,
}

impl TestApp {
    /// Start a PostgreSQL container, create a connection pool, run migrations,
    /// and build the application router.
    ///
    /// # Panics
    ///
    /// Panics if the test container cannot be started, the pool cannot be
    /// created, migrations fail, or the app router cannot be built.
    pub async fn setup() -> Self {
        let pool = if let Ok(dsn) = std::env::var("DATABASE_URL") {
            // Use the CI-provided postgres service when available.
            // The migrations use CREATE TABLE IF NOT EXISTS, so they are idempotent
            // and safe to run multiple times against the same database.
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(2)
                .connect(&dsn)
                .await
                .expect("failed to connect to DATABASE_URL")
        } else {
            // Fall back to testcontainers for local dev
            let container =
                Postgres::default().start().await.expect("failed to start postgres container");
            let host = container.get_host().await.expect("failed to get postgres host");
            let port =
                container.get_host_port_ipv4(5432).await.expect("failed to get postgres port");
            let dsn = format!("postgres://postgres:postgres@{host}:{port}/postgres");
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(2)
                .connect(&dsn)
                .await
                .expect("failed to connect to test postgres")
        };

        // Resolve migrations path relative to the workspace root.
        // For the [[test]] in the root Cargo.toml, CARGO_MANIFEST_DIR is the workspace root.
        let migrations_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let migrator = Migrator::new(migrations_path).await.expect("failed to create migrator");
        migrator.run(&pool).await.expect("failed to run migrations");

        let metrics = Metrics::new();
        let rate_limiter = RateLimiter::new(1000, 100, 60);
        let shared_pool = ac_db::pool::SharedPool::new(pool.clone());
        let app = ac_server::server::create_app(shared_pool, metrics, rate_limiter);

        Self { pool, app }
    }

    /// Return a reference to the connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Issue a raw request through the axum router via oneshot.
    ///
    /// # Panics
    ///
    /// Panics if the service call fails.
    pub async fn request(&self, req: Request<Body>) -> Response<Body> {
        self.app.clone().oneshot(req).await.expect("service call failed")
    }

    /// Issue a GET request to the given path.
    ///
    /// # Panics
    ///
    /// Panics if the URI cannot be parsed or the service call fails.
    pub async fn get(&self, path: &str) -> Response<Body> {
        let uri = path.parse::<axum::http::Uri>().expect("invalid URI");
        let req = Request::builder().uri(uri).body(Body::empty()).expect("failed to build request");
        self.request(req).await
    }

    /// Issue a POST request with a JSON body.
    ///
    /// # Panics
    ///
    /// Panics if the body cannot be serialised, the URI cannot be parsed, or
    /// the service call fails.
    pub async fn post_json(&self, path: &str, body: serde_json::Value) -> Response<Body> {
        let json = serde_json::to_string(&body).expect("failed to serialise JSON");
        let uri = path.parse::<axum::http::Uri>().expect("invalid URI");
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .expect("failed to build request");
        self.request(req).await
    }

    /// Issue a PUT request with a JSON body.
    ///
    /// # Panics
    ///
    /// Panics if the body cannot be serialised, the URI cannot be parsed, or
    /// the service call fails.
    pub async fn put_json(&self, path: &str, body: serde_json::Value) -> Response<Body> {
        let json = serde_json::to_string(&body).expect("failed to serialise JSON");
        let uri = path.parse::<axum::http::Uri>().expect("invalid URI");
        let req = Request::builder()
            .method("PUT")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .expect("failed to build request");
        self.request(req).await
    }

    /// Extract and deserialise the JSON body from a response.
    ///
    /// # Panics
    ///
    /// Panics if the body cannot be read or deserialised.
    pub async fn json_body<T: for<'de> serde::Deserialize<'de>>(resp: Response<Body>) -> T {
        let bytes =
            axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("failed to read body");
        serde_json::from_slice(&bytes).expect("failed to parse JSON body")
    }

    /// Extract the HTTP status code from a response.
    pub fn status(resp: &Response<Body>) -> StatusCode {
        resp.status()
    }

    /// Register a fresh agent through the public endpoint and return it.
    ///
    /// # Panics
    ///
    /// Panics if registration does not return 200.
    pub async fn register_agent(&self) -> TestAgent {
        let agent = TestAgent::new();
        let resp = self
            .post_json(
                "/v1/agents/register",
                serde_json::json!({
                    "agent_id": agent.id,
                    "public_key": agent.public_key_hex(),
                    "profile": { "name": "Test Agent" }
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::OK, "register_agent helper: expected 200 OK");
        agent
    }

    /// Issue a correctly signed request (fresh nonce, current timestamp).
    ///
    /// `path_and_query` is signed as given, so the query string is bound.
    pub async fn send_signed(
        &self,
        agent: &TestAgent,
        method: &str,
        path_and_query: &str,
        body: Option<&serde_json::Value>,
    ) -> Response<Body> {
        self.request(agent.request(method, path_and_query, body)).await
    }
}

/// A registered-or-registrable test agent holding its own Ed25519 keypair.
pub struct TestAgent {
    /// The agent id sent in `X-Agent-Id`.
    pub id: String,
    keypair: AgentKeypair,
}

impl Default for TestAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl TestAgent {
    /// Create an agent with a random id and a fresh keypair (not yet registered).
    pub fn new() -> Self {
        Self {
            id: format!("agent_{}", uuid::Uuid::new_v4().simple()),
            keypair: AgentKeypair::generate(),
        }
    }

    /// Hex-encoded public key, as stored in `agents.public_key`.
    pub fn public_key_hex(&self) -> String {
        self.keypair.public_key_hex()
    }

    /// A fresh random nonce.
    pub fn nonce() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    /// Sign `method path_and_query body` now, with a fresh nonce.
    pub fn sign(&self, method: &str, path_and_query: &str, body: &[u8]) -> SignedRequest {
        SignedRequest::sign(&self.keypair, &self.id, method, path_and_query, body, &Self::nonce())
    }

    /// Sign with an explicit nonce and timestamp string (for replay and skew tests).
    pub fn sign_at(
        &self,
        method: &str,
        path_and_query: &str,
        body: &[u8],
        nonce: &str,
        timestamp: &str,
    ) -> SignedRequest {
        SignedRequest::sign_at(
            &self.keypair,
            &self.id,
            method,
            path_and_query,
            body,
            nonce,
            timestamp,
        )
    }

    /// Build a request that is correctly signed for `body` (JSON, or empty if `None`).
    pub fn request(
        &self,
        method: &str,
        path_and_query: &str,
        body: Option<&serde_json::Value>,
    ) -> Request<Body> {
        let bytes = body.map(|b| serde_json::to_vec(b).expect("serialise")).unwrap_or_default();
        let signed = self.sign(method, path_and_query, &bytes);
        request_from_signed(&signed, bytes)
    }
}

/// Turn a [`SignedRequest`] plus a body into an HTTP request.
///
/// The request line comes from the signed fields, and the body is supplied
/// separately, so tests can tamper with either side independently of the
/// signature.
pub fn request_from_signed(signed: &SignedRequest, body: Vec<u8>) -> Request<Body> {
    let has_body = !body.is_empty();
    let mut builder = Request::builder()
        .method(signed.method.as_str())
        .uri(signed.path.as_str())
        .header(X_AGENT_ID, &signed.agent_id)
        .header(X_AGENT_TIMESTAMP, &signed.timestamp)
        .header(X_AGENT_NONCE, &signed.nonce)
        .header(X_AGENT_SIGNATURE, &signed.signature);
    if has_body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder.body(Body::from(body)).expect("failed to build request")
}
