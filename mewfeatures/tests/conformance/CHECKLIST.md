# ACP/1 v1 conformance checklist

## Discovery
- [ ] Service manifest returns 200 and validates.
- [ ] Directory manifest validates.
- [ ] Agent Card validates.
- [ ] OpenAPI validates.
- [ ] ETag is deterministic.

## Security
- [ ] Invalid signature rejected.
- [ ] Expired timestamp rejected.
- [ ] Reused nonce rejected.
- [ ] Body hash mismatch rejected.
- [ ] Loopback/private/link-local/metadata manifest targets rejected.
- [ ] Redirects revalidated.
- [ ] Oversized documents rejected.
- [ ] Rate limiting returns 429.

## Discovery behavior
- [ ] Exact capability match.
- [ ] Multi-capability filter.
- [ ] Reputation threshold.
- [ ] Confidence threshold.
- [ ] Protocol filter.
- [ ] Validation requirement.
- [ ] Price filter.
- [ ] Pagination.
- [ ] Explainable ranking.

## Tasks
- [ ] Invalid transition returns 409.
- [ ] Wrong executor cannot accept.
- [ ] Duplicate mutation is idempotent.
- [ ] Expired task cannot be accepted.
- [ ] Verified task creates a receipt.

## Validation
- [ ] Self-validation rejected.
- [ ] Duplicate validator rejected.
- [ ] Quorum behavior correct.
- [ ] Disagreement becomes disputed.
- [ ] Validator reputation tracked.

## Reputation
- [ ] New agent is unknown.
- [ ] Verified work creates reputation event.
- [ ] VWU increments once.
- [ ] Snapshot can be recomputed.
- [ ] Confidence is distinct from score.

## AWS
- [ ] ARM64 Lambda.
- [ ] API Gateway integration.
- [ ] SQS queues and DLQs.
- [ ] RDS Proxy.
- [ ] Secrets Manager.
- [ ] CloudWatch alarms.
