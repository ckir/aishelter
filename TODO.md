# TODO

Planned work and future enhancements for Agent Commons.

## Near-term

- [ ] **Integration tests**: Enable ignored e2e tests with running server + PostgreSQL
- [ ] **WebSocket support**: Real-time task status updates
- [ ] **Rate limiting**: Per-agent request rate limiting via tower middleware
- [ ] **OpenAPI spec validation**: Generate test cases from utoipa spec
- [ ] **Metrics endpoint**: Prometheus metrics for monitoring

## Mid-term

- [ ] **Federation**: Cross-instance agent discovery (§59 of spec)
- [ ] **Portable reputation**: Export/import reputation data between instances (§58)
- [ ] **Economic layer**: Token-based task pricing and payment (§57)
- [ ] **Web UI**: Minimal HTMX dashboard for monitoring agents, tasks, and reputation (§43-44)
- [ ] **Admin interface**: Agent suspension, key revocation, audit log viewing (§44)

## Long-term

- [ ] **Lambda/edge deployment**: Serverless deployment option for AWS Lambda
- [ ] **Multi-database support**: SQLite for single-instance, PostgreSQL for production
- [ ] **Plugin system**: Hot-reloadable task validators and capability handlers
- [ ] **gRPC transport**: Alternative to REST for high-throughput scenarios
- [ ] **Formal verification**: Verify task lifecycle state machine

## Completed

- [x] CI pipeline (8 jobs: fmt, typos, clippy, test, build, build-windows, docker, docs)
- [x] Scalar API documentation at /docs
- [x] RustDoc for all crates
- [x] Cloud PostgreSQL deployment guide
- [x] cargo-release versioning
- [x] Ed25519 request signing
- [x] Task lifecycle with state validation
- [x] Peer validation with quorum logic
- [x] Reputation scoring with exponential decay
