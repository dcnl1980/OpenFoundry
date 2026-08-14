#!/usr/bin/env python3
"""End-to-end production-path check through the OpenFoundry gateway."""

from __future__ import annotations

import json
import sys
import time
import urllib.error
import urllib.request
import uuid
from typing import Any

GATEWAY = "http://127.0.0.1:8080"
JWT_SECRET = "change-me-in-production-use-a-256-bit-key"
AUTH_DB = "postgres://openfoundry:openfoundry@127.0.0.1:5432/ofe2e_auth"

DIRECT_HEALTH = {
    "gateway": 8080,
    "auth-service": 50051,
    "data-connector": 50152,
    "dataset-service": 50053,
    "streaming-service": 50054,
    "query-service": 50055,
    "pipeline-service": 50056,
    "ontology-service": 50057,
    "fusion-service": 50058,
    "ml-service": 50059,
    "ai-service": 50060,
    "workflow-service": 50061,
    "notebook-service": 50062,
    "app-builder-service": 50063,
    "report-service": 50064,
    "code-repo-service": 50065,
    "marketplace-service": 50066,
    "nexus-service": 50067,
    "geospatial-service": 50068,
    "notification-service": 50069,
    "audit-service": 50070,
}

results: list[tuple[str, bool, str]] = []


def record(name: str, ok: bool, detail: str = "") -> None:
    results.append((name, ok, detail))
    mark = "PASS" if ok else "FAIL"
    print(f"[{mark}] {name}{': ' + detail if detail else ''}")


def request(
    method: str,
    path: str,
    *,
    token: str | None = None,
    body: Any | None = None,
    timeout: float = 20,
) -> tuple[int, Any]:
    data = None
    headers = {"Accept": "application/json"}
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(GATEWAY + path, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read()
            payload: Any = raw.decode() if raw else ""
            if payload:
                try:
                    payload = json.loads(payload)
                except json.JSONDecodeError:
                    pass
            return resp.status, payload
    except urllib.error.HTTPError as exc:
        raw = exc.read()
        payload = raw.decode() if raw else ""
        if payload:
            try:
                payload = json.loads(payload)
            except json.JSONDecodeError:
                pass
        return exc.code, payload


def extract_ids(payload: Any) -> list[str]:
    if isinstance(payload, list):
        return [str(item["id"]) for item in payload if isinstance(item, dict) and "id" in item]
    if isinstance(payload, dict):
        if "id" in payload and not any(key in payload for key in ("data", "items")):
            return [str(payload["id"])]
        for key in ("data", "items", "datasets", "results"):
            if key in payload:
                return extract_ids(payload[key])
    return []


def first_id(payload: Any) -> str | None:
    ids = extract_ids(payload)
    return ids[0] if ids else None


def expect_status(name: str, status: int, allowed: set[int], detail: str = "") -> bool:
    ok = status in allowed
    record(name, ok, detail or f"status={status}")
    return ok


def health_direct() -> None:
    for name, port in DIRECT_HEALTH.items():
        url = f"http://127.0.0.1:{port}/health"
        try:
            with urllib.request.urlopen(url, timeout=5) as resp:
                body = resp.read().decode()
                record(f"health {name}", resp.status == 200 and body.strip() == "ok", body.strip())
        except Exception as exc:  # noqa: BLE001
            record(f"health {name}", False, str(exc))


def login(email: str, password: str) -> tuple[str, str]:
    status, payload = request("POST", "/api/v1/auth/login", body={"email": email, "password": password})
    if status != 200 or not isinstance(payload, dict):
        raise RuntimeError(f"login failed {status} {payload}")
    if payload.get("status") == "authenticated" or "access_token" in payload:
        access = payload.get("access_token") or payload.get("Authenticated", {}).get("access_token")
        refresh = payload.get("refresh_token")
        if access and refresh:
            return access, refresh
    raise RuntimeError(f"unexpected login payload {payload}")


def promote_admin(email: str) -> None:
    import subprocess

    sql = f"""
    INSERT INTO user_roles (user_id, role_id, tenant_id)
    SELECT u.id, r.id, u.tenant_id
    FROM openfoundry_lookup_user_by_email('{email}') AS u
    CROSS JOIN roles r
    WHERE r.name = 'admin'
    ON CONFLICT (user_id, role_id) DO NOTHING;
    """
    subprocess.check_call(["psql", AUTH_DB, "-v", "ON_ERROR_STOP=1", "-c", sql], stdout=subprocess.DEVNULL)


def create_and_isolate(
    name: str,
    create_path: str,
    list_path: str,
    get_path_template: str,
    body: dict[str, Any],
    token_a: str,
    token_b: str,
    create_ok: set[int] | None = None,
) -> str | None:
    create_ok = create_ok or {200, 201}
    status, created = request("POST", create_path, token=token_a, body=body)
    if not expect_status(f"{name} create as A", status, create_ok, f"status={status} body={created}"):
        return None
    resource_id = first_id(created)
    if not resource_id:
        status, listed = request("GET", list_path, token=token_a)
        resource_id = first_id(listed)
    if not resource_id:
        record(f"{name} created id", False, f"no id in {created}")
        return None
    record(f"{name} created id", True, resource_id)

    status, listed_b = request("GET", list_path, token=token_b)
    expect_status(f"{name} list as B", status, {200}, f"status={status}")
    ids_b = extract_ids(listed_b)
    record(f"{name} B cannot list A's row", resource_id not in ids_b, f"b_ids={ids_b}")

    if get_path_template:
        get_path = get_path_template.format(id=resource_id)
        status, fetched_b = request("GET", get_path, token=token_b)
        leaked = status == 200 and first_id(fetched_b) == resource_id
        record(f"{name} B get A is empty/404", not leaked, f"status={status}")
        status, fetched_a = request("GET", get_path, token=token_a)
        if status == 405:
            record(f"{name} A get own row", True, "no GET route; list isolation used")
        else:
            expect_status(f"{name} A can get own row", status, {200}, f"status={status}")
    return resource_id


def main() -> int:
    health_direct()

    status, _ = request("GET", "/api/v1/datasets")
    expect_status("unauthenticated datasets is 401", status, {401})

    stamp = int(time.time())
    email_a = f"e2e-a-{stamp}@example.test"
    email_b = f"e2e-b-{stamp}@example.test"
    password = "E2ePassw0rd!"

    status, payload = request(
        "POST",
        "/api/v1/auth/register",
        body={"email": email_a, "password": password, "name": "Tenant A"},
    )
    expect_status("register A", status, {200, 201}, f"status={status} body={payload}")
    status, payload = request(
        "POST",
        "/api/v1/auth/register",
        body={"email": email_b, "password": password, "name": "Tenant B"},
    )
    expect_status("register B", status, {200, 201}, f"status={status} body={payload}")

    status, payload = request("POST", "/api/v1/auth/login", body={"email": email_a, "password": "wrong"})
    expect_status("bad password is 401", status, {401})

    token_a, refresh_a = login(email_a, password)
    token_b, _ = login(email_b, password)
    record("login A and B", True, "tokens issued")

    status, me = request("GET", "/api/v1/users/me", token=token_a)
    expect_status("users/me as A", status, {200}, f"status={status}")
    if isinstance(me, dict):
        record("users/me email matches A", me.get("email") == email_a, str(me.get("email")))
        roles = me.get("roles")
        record("register A is tenant admin", isinstance(roles, list) and "admin" in roles, str(roles))
    status, me_b = request("GET", "/api/v1/users/me", token=token_b)
    roles_b = me_b.get("roles") if isinstance(me_b, dict) else None
    record("register B is tenant admin", isinstance(roles_b, list) and "admin" in roles_b, str(roles_b))

    status, refreshed = request("POST", "/api/v1/auth/refresh", body={"refresh_token": refresh_a})
    if status != 200:
        status, refreshed = request("POST", "/api/v1/auth/refresh", body={"token": refresh_a})
    expect_status("refresh token", status, {200}, f"status={status} body={refreshed}")
    if isinstance(refreshed, dict) and refreshed.get("access_token"):
        token_a = refreshed["access_token"]

    status, sso = request("GET", "/api/v1/auth/sso/providers/public")
    expect_status("public SSO catalog", status, {200}, f"status={status}")

    try:
        promote_admin(email_a)
        token_a, _ = login(email_a, password)
        record("promote A to admin and re-login", True)
    except Exception as exc:  # noqa: BLE001
        record("promote A to admin and re-login", False, str(exc))

    status, me = request("GET", "/api/v1/users/me", token=token_a)
    roles = me.get("roles") if isinstance(me, dict) else None
    record("A JWT includes admin after promote", isinstance(roles, list) and "admin" in roles, str(roles))

    status, payload = request("POST", "/api/v1/groups", token=token_a, body={"name": f"ops-{stamp}", "description": "e2e"})
    expect_status("auth create group as admin", status, {200, 201}, f"status={status} body={payload}")
    group_id = first_id(payload)
    status, groups_b = request("GET", "/api/v1/groups", token=token_b)
    expect_status("list groups as B", status, {200, 403}, f"status={status}")
    if status == 200 and group_id:
        record("B cannot see A's group", group_id not in extract_ids(groups_b), str(extract_ids(groups_b)))
    elif status == 403:
        record("B cannot see A's group", True, "forbidden")

    status, key = request("POST", "/api/v1/api-keys", token=token_a, body={"name": "e2e-deploy"})
    expect_status("create API key as A", status, {200, 201}, f"status={status} body={key}")
    status, keys_b = request("GET", "/api/v1/api-keys", token=token_b)
    expect_status("list API keys as B", status, {200, 403}, f"status={status}")
    if status == 200 and first_id(key):
        record("B cannot see A's API key", first_id(key) not in extract_ids(keys_b), str(extract_ids(keys_b)))

    create_and_isolate(
        "dataset",
        "/api/v1/datasets",
        "/api/v1/datasets",
        "/api/v1/datasets/{id}",
        {"name": f"flights-{stamp}", "description": "e2e dataset", "format": "parquet", "tags": ["e2e"]},
        token_a,
        token_b,
    )
    create_and_isolate(
        "ontology type",
        "/api/v1/ontology/types",
        "/api/v1/ontology/types",
        "/api/v1/ontology/types/{id}",
        {"name": f"Aircraft{stamp}", "display_name": "Aircraft", "description": "e2e type"},
        token_a,
        token_b,
    )
    create_and_isolate(
        "saved query",
        "/api/v1/queries/saved",
        "/api/v1/queries/saved",
        "",
        {"name": f"q-{stamp}", "sql": "SELECT 1", "description": "e2e"},
        token_a,
        token_b,
    )
    create_and_isolate(
        "connection",
        "/api/v1/connections",
        "/api/v1/connections",
        "/api/v1/connections/{id}",
        {
            "name": f"pg-{stamp}",
            "connector_type": "postgresql",
            "config": {
                "host": "127.0.0.1",
                "port": 5432,
                "database": "openfoundry",
                "user": "openfoundry",
                "password": "openfoundry",
            },
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "pipeline",
        "/api/v1/pipelines",
        "/api/v1/pipelines",
        "/api/v1/pipelines/{id}",
        {
            "name": f"pipe-{stamp}",
            "description": "e2e",
            "nodes": [{"id": "n1", "label": "passthrough", "transform_type": "passthrough"}],
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "workflow",
        "/api/v1/workflows",
        "/api/v1/workflows",
        "/api/v1/workflows/{id}",
        {
            "name": f"wf-{stamp}",
            "trigger_type": "manual",
            "steps": [{"id": "s1", "name": "start", "step_type": "noop"}],
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "notebook",
        "/api/v1/notebooks",
        "/api/v1/notebooks",
        "/api/v1/notebooks/{id}",
        {"name": f"nb-{stamp}", "description": "e2e"},
        token_a,
        token_b,
    )
    create_and_isolate(
        "app",
        "/api/v1/apps",
        "/api/v1/apps",
        "/api/v1/apps/{id}",
        {"name": f"Ops Center {stamp}", "slug": f"ops-{stamp}"},
        token_a,
        token_b,
    )
    create_and_isolate(
        "ml experiment",
        "/api/v1/ml/experiments",
        "/api/v1/ml/experiments",
        "",
        {"name": f"exp-{stamp}", "description": "e2e", "objective": "accuracy"},
        token_a,
        token_b,
    )
    create_and_isolate(
        "ai provider",
        "/api/v1/ai/providers",
        "/api/v1/ai/providers",
        "",
        {"name": f"local-{stamp}", "provider_type": "openai_compatible", "model_name": "test", "enabled": True},
        token_a,
        token_b,
    )
    create_and_isolate(
        "ai tool",
        "/api/v1/ai/tools",
        "/api/v1/ai/tools",
        "",
        {"name": f"lookup-{stamp}", "description": "e2e tool"},
        token_a,
        token_b,
    )
    create_and_isolate(
        "fusion rule",
        "/api/v1/fusion/rules",
        "/api/v1/fusion/rules",
        "",
        {
            "name": f"match-{stamp}",
            "conditions": [{"field": "email", "comparator": "exact", "weight": 1.0, "threshold": 1.0, "required": True}],
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "stream",
        "/api/v1/streaming/streams",
        "/api/v1/streaming/streams",
        "",
        {"name": f"events-{stamp}", "description": "e2e"},
        token_a,
        token_b,
    )
    create_and_isolate(
        "report",
        "/api/v1/reports/definitions",
        "/api/v1/reports/definitions",
        "",
        {
            "name": f"weekly-{stamp}",
            "owner": "e2e",
            "generator_kind": "csv",
            "dataset_name": "flights",
            "template": {
                "title": "Weekly",
                "subtitle": "",
                "theme": "default",
                "layout": "single",
                "sections": [{"id": "kpi", "title": "KPI", "kind": "kpi", "query": "SELECT 1", "description": "e2e"}],
            },
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "geo layer",
        "/api/v1/geospatial/layers",
        "/api/v1/geospatial/layers",
        "",
        {
            "name": f"airports-{stamp}",
            "source_kind": "dataset",
            "source_dataset": "airports",
            "geometry_type": "point",
            "features": [
                {
                    "id": "lhr",
                    "label": "Heathrow",
                    "geometry": {"type": "point", "coordinates": {"lat": 51.47, "lon": -0.45}},
                }
            ],
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "code repo",
        "/api/v1/code-repos/repositories",
        "/api/v1/code-repos/repositories",
        "",
        {
            "name": f"transforms-{stamp}",
            "slug": f"transforms-{stamp}",
            "owner": "e2e",
            "visibility": "private",
            "object_store_backend": "local",
            "package_kind": "transform",
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "marketplace listing",
        "/api/v1/marketplace/listings",
        "/api/v1/marketplace/listings",
        "/api/v1/marketplace/listings/{id}",
        {
            "name": f"Connector {stamp}",
            "slug": f"connector-{stamp}",
            "summary": "e2e listing",
            "publisher": "e2e",
            "category_slug": "connectors",
            "package_kind": "connector",
            "repository_slug": f"transforms-{stamp}",
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "nexus peer",
        "/api/v1/nexus/peers",
        "/api/v1/nexus/peers",
        "",
        {
            "slug": f"peer-{stamp}",
            "display_name": "Peer A",
            "region": "eu-west",
            "endpoint_url": "https://peer.example.test",
            "auth_mode": "mtls",
            "trust_level": "high",
            "public_key_fingerprint": "aa:bb:cc",
        },
        token_a,
        token_b,
    )
    create_and_isolate(
        "audit policy",
        "/api/v1/audit/policies",
        "/api/v1/audit/policies",
        "/api/v1/audit/policies/{id}",
        {
            "name": f"retain-{stamp}",
            "scope": "ops",
            "classification": "public",
            "retention_days": 30,
            "purge_mode": "retain",
            "updated_by": "e2e",
        },
        token_a,
        token_b,
    )

    status, event = request(
        "POST",
        "/api/v1/audit/events",
        token=token_a,
        body={
            "source_service": "e2e",
            "channel": "http",
            "actor": email_a,
            "action": "login",
            "resource_type": "session",
            "resource_id": "s1",
            "status": "success",
            "severity": "low",
            "classification": "public",
        },
    )
    expect_status("audit append event as A", status, {200, 201}, f"status={status} body={event}")
    event_id = first_id(event)
    if event_id:
        status, _ = request("GET", f"/api/v1/audit/events/{event_id}", token=token_b)
        record("audit B cannot read A's event", status in {404, 403}, f"status={status}")

    status, note = request(
        "POST",
        "/api/v1/notifications/send",
        token=token_a,
        body={"title": "E2E ready", "body": "platform check", "severity": "info"},
    )
    expect_status("notification send as A", status, {200, 201}, f"status={status} body={note}")
    status, notes_b = request("GET", "/api/v1/notifications", token=token_b)
    expect_status("notification list as B", status, {200}, f"status={status}")
    if first_id(note):
        record("B cannot see A's notification", first_id(note) not in extract_ids(notes_b), str(extract_ids(notes_b)))

    status, providers = request("GET", "/api/v1/ai/providers", token=token_a)
    expect_status("ai providers list as A", status, {200}, f"status={status}")
    provider_names: list[str] = []
    if isinstance(providers, dict):
        provider_names = [
            str(item.get("name"))
            for item in providers.get("data", [])
            if isinstance(item, dict) and item.get("name")
        ]
    record("A received cloned system AI providers", "OpenRouter" in provider_names, str(provider_names))

    status, templates = request("GET", "/api/v1/apps/templates", token=token_a)
    expect_status("app templates list as A", status, {200}, f"status={status}")
    template_keys: list[str] = []
    if isinstance(templates, dict):
        template_keys = [
            str(item.get("key"))
            for item in templates.get("data", [])
            if isinstance(item, dict) and item.get("key")
        ]
    record("A received cloned system app templates", "ops-center" in template_keys, str(template_keys))

    for path, label in (
        ("/api/v1/ai/overview", "ai overview"),
        ("/api/v1/ml/overview", "ml overview"),
        ("/api/v1/fusion/overview", "fusion overview"),
        ("/api/v1/audit/overview", "audit overview"),
        ("/api/v1/marketplace/overview", "marketplace overview"),
        ("/api/v1/nexus/overview", "nexus overview"),
        ("/api/v1/code-repos/overview", "code-repo overview"),
        ("/api/v1/reports/overview", "report overview"),
        ("/api/v1/apps/templates", "app templates"),
        ("/api/v1/widgets/catalog", "widget catalog"),
        ("/api/v1/audit/classifications", "audit classifications"),
        ("/api/v1/roles", "auth roles"),
        ("/api/v1/permissions", "auth permissions"),
    ):
        status, payload = request("GET", path, token=token_a)
        expect_status(label, status, {200}, f"status={status}")

    failed = [item for item in results if not item[1]]
    print()
    print(f"{len(results) - len(failed)}/{len(results)} checks passed")
    if failed:
        print("Failures:")
        for name, _, detail in failed:
            print(f"  - {name}: {detail}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
