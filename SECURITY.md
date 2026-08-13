# Security

## Reporting

Report vulnerabilities privately to the repository maintainers. Do not open a public issue for an exploitable defect. We aim to acknowledge a report within 3 business days and to provide a status update within 10 business days.

## Supported versions

The `main` branch is the supported line until a numbered release exists.

## Authentication

- Users authenticate through `auth-service`. Access tokens (`token_use=access` or `api_key`) are the only credentials the gateway accepts on private routes.
- Refresh tokens cannot call APIs.
- The gateway strips inbound `x-openfoundry-*` headers and writes tenant scope from the JWT only.

## Tenant isolation

- Tenant key: JWT `org_id`, or the user id when the user has no organization.
- Control plane: gateway trusted headers and quotas.
- Data plane: `tenant_id` on tenant-owned tables, `SET LOCAL openfoundry.tenant_id`, and PostgreSQL RLS (`FORCE ROW LEVEL SECURITY`).
- The runtime database role must not be `SUPERUSER` or `BYPASSRLS`.
- Admins stay inside their tenant. There is no RLS bypass for `role=admin`.

## Secrets

- `JWT_SECRET` and database credentials are environment-only. Do not commit them.
- Service TLS material (`TLS_CERT_PATH`, `TLS_KEY_PATH`, `TLS_CA_PATH`) enables mTLS between the gateway and services.

## Encryption and transport

- Production hops should use mTLS (`service-runtime`). Plaintext HTTP is development-only and logs a warning.
- Tokens are HMAC-SHA256 JWTs.

## Dependencies

- CI runs Clippy (`-D warnings`), `cargo deny`, and a scheduled RustSec audit.
- Dependabot is configured for dependency updates.

## Threat model (current)

| Threat | Control |
|---|---|
| Client-spoofed tenant headers | Gateway strips and rewrites after JWT decode |
| Cross-tenant row access | `tenant_id` + RLS + tenant transaction GUC |
| Superuser RLS bypass | Runtime role must not be superuser |
| Direct service port access | mTLS + service `auth_layer` |
| Refresh token as API credential | Gateway and services reject `token_use=refresh` |
