# AWS deployment

## Production

API Gateway -> Lambda -> RDS Proxy -> Aurora PostgreSQL.

Asynchronous:
Lambda -> SQS -> worker Lambda.

Scheduled:
EventBridge -> maintenance Lambda.

Secrets:
AWS Secrets Manager.

Encryption:
AWS KMS.

Edge:
AWS WAF. CloudFront/S3 may serve static `.well-known` documents.

## Lambda

Reuse the existing Rust Axum/application layer. Lambda handlers must be adapters, not a second business-logic implementation.

Use ARM64 (`provided.al2023`) unless there is a repository-specific reason not to.

Workers must be idempotent and acknowledge SQS only after durable completion.

## Database

RDS Proxy is the preferred production path for Lambda concurrency.

Aurora PostgreSQL is the source of truth.

## Security

Least-privilege IAM.
No private agent keys in AWS.
No credentials in ordinary application tables.
SSRF-safe manifest fetching.
WAF + application rate limits.
CloudWatch audit/metrics.
