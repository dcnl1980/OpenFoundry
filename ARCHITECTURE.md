# Architecture

This document is a map of the current Rust + Svelte tree in [dcnl1980/OpenFoundry](https://github.com/dcnl1980/OpenFoundry). It describes how requests, identity, and events move through the system today — not a target-state rewrite.

Clone this repository:

```bash
git clone https://github.com/dcnl1980/OpenFoundry.git
cd OpenFoundry
```

## Planes

Treat the four planes as a routing map, not as a reason to merge services.

| Plane | What it owns | Where it lives |
|---|---|---|
| **Experience** | Browser UI, session restore, Copilot panel | `apps/web` (SvelteKit). Calls `/api/v1/*` only. |
| **Control** | Auth, tenancy claims, gateway routing, audit collection | `services/gateway`, `services/auth-service`, `services/audit-service` |
| **Data** | Datasets, queries, pipelines, ontology, streaming, geospatial | `dataset-service`, `query-service`, `pipeline-service`, `ontology-service`, `streaming-service`, `geospatial-service`, `data-connector` |
| **Intelligence** | LLM providers, Copilot, ML, fusion, workflows, apps | `ai-service`, `ml-service`, `fusion-service`, `workflow-service`, `app-builder-service` |

Shared crates under `libs/` are the contract layer: JWT/tenant context (`auth-middleware`), NATS publish/subscribe (`event-bus`), and domain models (`core-models`).

```text
Browser (apps/web)
        |
        |  /api/v1/*  + Authorization: Bearer <jwt>
        v
   gateway :8080
        |  strips client x-openfoundry-*
        |  adds trusted tenant headers from JWT
        |  enqueues audit (path only, no query string)
        v
   service by URL prefix  (auth, datasets, ontology, ai, ...)
        |
        +--> Postgres (per service)
        +--> NATS JetStream  (OF_AUDIT / of.audit.>)
                    |
                    v
              audit-service collector
```

## Tenant boundary

A tenant is an organization scope taken from the JWT (`org_id`, else the subject). `auth-middleware::TenantContext` also carries tier and quota policy (query limit, pipeline workers, request body size, requests per minute).

Services must not trust the client for that scope. The only trusted copy is the one the gateway writes after it decodes the bearer token.

## Auth trust chain

1. User signs in through `auth-service`. The browser stores the JWT.
2. Every API call goes to the gateway with `Authorization: Bearer <jwt>`.
3. The gateway decodes the token with `JWT_SECRET` and builds `TenantContext`.
4. The gateway **removes every inbound `x-openfoundry-*` header**, then writes:

   | Header | Source |
   |---|---|
   | `x-openfoundry-tenant-scope` | JWT org/subject |
   | `x-openfoundry-tenant-tier` | JWT `tenant_tier` (or admin → enterprise) |
   | `x-openfoundry-quota-query-limit` | tier / `tenant_quotas` |
   | `x-openfoundry-quota-pipeline-workers` | tier / `tenant_quotas` |
   | `x-openfoundry-quota-requests-per-minute` | tier / `tenant_quotas` |

5. Hop-by-hop headers (`host`, `connection`, `keep-alive`, `transfer-encoding`, `upgrade`, `te`, `proxy-*`, plus names listed in `Connection`) are not forwarded.
6. Downstream services may read those headers, but they are only as trustworthy as this gateway hop. There is no service-to-service mTLS yet.

Unauthenticated routes (`/health`, login/register) still pass through the gateway; they simply have no tenant headers.

## Gateway audit and NATS

On startup, if `NATS_URL` is set, the gateway connects **once**, ensures the `OF_AUDIT` stream, and starts a worker.

Each request produces one audit payload (`action = request.forwarded`):

- `metadata.path` and `resource_id` are the URL **path only**. Query strings (API keys, search terms, IDs) are not recorded.
- The payload is sent on a bounded channel (1024). If the worker is behind, the event is dropped and a warning is logged. The HTTP response is not blocked.
- The worker publishes to `of.audit.gateway` and retries once on failure.
- If NATS is unset or the connect/ensure-stream step fails, audit publishing is disabled and the gateway still serves traffic.

`audit-service` is the consumer: it creates a durable pull consumer on `OF_AUDIT` and persists events.

## Failure modes

| Failure | What the user sees | What the platform does |
|---|---|---|
| Unknown `/api/v1/...` prefix | `404 unknown service route` | No upstream call |
| Upstream down | `502 upstream unavailable` | Gateway logs the reqwest error |
| Request body over tenant clamp | `413 body too large` | Declared `Content-Length` is rejected up front; a streamed body that exceeds the remaining budget aborts the upstream call |
| Tenant or IP over `requests_per_minute` | `429 rate limit exceeded` + `Retry-After` | In-process token bucket; `/health` is exempt. Not shared across gateway replicas |
| Invalid JWT | Request is proxied without tenant headers | Downstream must treat the call as unscoped |
| NATS down at boot | API still works | Audit handle is disabled |
| NATS down at runtime | API still works | Worker logs publish failures; queue may drop |
| Audit queue full | API still works | Event dropped (`queue full`) |
| LLM provider down | Copilot/chat fall back to a local draft | `ai-service` does not fail the whole panel |

## Rate limiting

The gateway keeps an in-memory token bucket per tenant scope (JWT) or client IP (`X-Forwarded-For` / `X-Real-IP`). Capacity and refill follow `TenantContext.quotas.requests_per_minute` (anonymous callers use the standard 300/min policy). This is per process: two gateway replicas do not share counters. Redis-backed limiting is not implemented.

## Proxied bodies

Request and response bodies are streamed. The gateway does not buffer the full payload. Size is enforced with the tenant body quota (10 MiB when unauthenticated): an oversize `Content-Length` returns 413 before the upstream call; a chunked body that crosses the remaining budget fails the stream and returns 413.

## What this map does not claim

- Rate limits are not cluster-wide.
- Gateway auth middleware is not applied as a global reject; JWT is decoded in the proxy for header injection and in the rate-limit layer for the bucket key.
- `ROADMAP.md` status icons mean a service or UI surface exists, not that the feature is production-ready.

## Related docs

- [README.md](README.md) — product overview and clone/setup
- [ROADMAP.md](ROADMAP.md) — capability tracker
- [CONTRIBUTING.md](CONTRIBUTING.md) — developer setup
- [docs/audits/2026-08-13-copilot-ontology-gaps.md](docs/audits/2026-08-13-copilot-ontology-gaps.md) — recent Copilot/ontology wiring notes
