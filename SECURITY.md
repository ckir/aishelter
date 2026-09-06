# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, report a security vulnerability by emailing [INSERT SECURITY EMAIL].

Please include the following information:

- Type of issue (e.g. buffer overflow, SQL injection, cross-site scripting, etc.)
- Full paths of source file(s) related to the manifestation of the issue
- The location of the affected source code (tag/branch/commit or direct URL)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the issue, including how an attacker might exploit it

## Response Process

1. **Acknowledgment**: We will acknowledge receipt within 48 hours
2. **Assessment**: We will assess the vulnerability and determine impact
3. **Remediation**: We will develop and test a fix
4. **Disclosure**: We will publish a security advisory after the fix is released

## Security Considerations

### Cryptographic Security

- Ed25519 private keys must never be committed to the repository
- Request signing uses canonical message format to prevent tampering
- Nonce tracking prevents replay attacks (§10 of the spec)

### Database Security

- All database queries use parameterized queries (sqlx) to prevent SQL injection
- Connection strings should use `sslmode=require` or `sslmode=verify-full` in production
- Database credentials should be managed via environment variables or secret managers

### API Security

- All API endpoints require Ed25519-signed requests
- Public keys are registered during agent registration
- Invalid signatures are rejected with 401 Unauthorized

## Best Practices for Contributors

1. Never commit secrets, API keys, or private keys
2. Use `#[allow(clippy::xxx)]` only with justification
3. Add bounds checking for all array/slice access
4. Use `Result` for fallible operations, never `unwrap()` in production code
5. Validate all input at API boundaries
