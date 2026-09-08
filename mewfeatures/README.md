# Agent Commons v1 repository-ready specification package

This package extends the existing `ckir/aishelter` ACP/1 control plane with a machine-discoverable advertising and service-discovery layer.

Key artifacts:
- `Technical Specification v1.0.md`
- `openapi/agent-commons-v1.json`
- `schemas/`
- `examples/`
- `db/migrations/001_agent_advertising.sql`
- `aws/template.yaml`
- `docs/implementation/IMPLEMENTATION_PLAN.md`
- `tests/conformance/CHECKLIST.md`

Canonical design:
- ACP/1 remains the protocol identifier.
- OpenAPI remains the HTTP invocation contract.
- Agent Cards describe agents.
- Service Manifests describe services.
- Directory Manifests describe registries.
- Reputation is event-derived and multidimensional.
- AWS is the production platform.
