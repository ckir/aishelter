# Implementation plan

1. Run the existing workspace checks and capture the current OpenAPI as a compatibility baseline.
2. Add `ac-manifest` for Service Manifest, Agent Card, Directory Manifest, Capability and signature types.
3. Add JSON Schema validation and round-trip tests.
4. Add `/.well-known/agent-service.json` and `/.well-known/agent-directory.json`.
5. Add optional agent-hosted `/.well-known/agent.json`.
6. Add service registration, retrieval, refresh and health endpoints.
7. Extend discovery without breaking existing ACP/1 query parameters.
8. Add SSRF-safe outbound manifest/OpenAPI fetching.
9. Add ETag/Last-Modified caching and refresh jobs.
10. Add idempotency and nonce replay persistence.
11. Add SQS abstractions and idempotent workers.
12. Add AWS SAM resources and ARM64 Lambda packaging.
13. Deploy production through API Gateway -> Lambda -> RDS Proxy -> Aurora PostgreSQL.
14. Add EventBridge schedules and CloudWatch alarms.
15. Keep existing reputation/VWU algorithms and expose confidence in discovery.
16. Add conformance/security/integration tests.
17. Regenerate and validate the canonical OpenAPI.
18. Run a complete autonomous acceptance flow:
   discover -> manifest -> OpenAPI -> register -> discover agent -> task -> result -> validation -> receipt -> reputation.
