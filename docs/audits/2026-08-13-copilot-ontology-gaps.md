# Copilot and ontology wiring audit

Date: 2026-08-13  
Scope: this Rust + Svelte tree (`dcnl1980/OpenFoundry`), compared with the four Go/React PRs on `u485349-coder/OpenFoundry`.

## Already correct here

| Claimed upstream bug | Status in this tree |
|---|---|
| Copilot panel never mounted | Mounted in `apps/web/src/routes/+layout.svelte` |
| Copilot 404 under `/api/v1/ai` | Routed in `ai-service` and proxied by the gateway |
| List object types returns `items` instead of `data` | `list_object_types` returns `{ data, total, page, per_page }` |
| Copilot never binds an LLM provider | `ask_copilot` loads enabled rows from `ai_providers` |
| `/object-views` 404 blanks Ontology Manager | This client never calls `/object-views` |
| `dataset_views` migration crash | No Go `dataset-versioning-service` in this tree |

## Gaps found and fixed

1. **Ontology type detail page could not load.**  
   `/ontology/[id]` loads properties, links, objects, and actions in `Promise.all`. The client called `GET /api/v1/ontology/types/{id}/properties`, but ontology-service never mounted that route. One 404 failed the whole page.  
   Fix: add list/create property handlers and degrade a leftover 404 to `[]`.

2. **Copilot context used `Promise.all`.**  
   A down dataset-service also hid knowledge bases, even though `/ai/copilot/ask` can run without either list.  
   Fix: load each list independently and wait for `auth.restore()` before fetching.

## Follow-ups

- Property create form on `/ontology/[id]`, posting to `POST /api/v1/ontology/types/{id}/properties`.
- Copilot (and chat) now call the configured provider HTTP API; local drafts remain a fallback when the live call fails.
- Ontology handlers return `{ "error": "..." }` and the web client reads string or JSON error bodies.
