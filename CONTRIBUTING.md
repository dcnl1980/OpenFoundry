# Contributing

## Local stack

```bash
cp .env.example .env
docker compose -f infra/docker-compose.yml up -d postgres redis nats
cargo build
./scripts/start-platform.sh
python3 scripts/e2e-platform.py
```

`start-platform.sh` creates one Postgres database per service, grants `openfoundry_app`, and starts every binary in tmux session `of-platform`. Do not point every service at the same database: sqlx migrations share `_sqlx_migrations`.

The first registered user in a tenant is that tenant’s admin. To promote a specific operator later, set `BOOTSTRAP_ADMIN_EMAIL` on `auth-service` and have them log in once, then unset it.

## Production deploy

Production is the Helm chart in `infra/k8s/helm/open-foundry` with `values-prod.yaml`.

Required secrets before `ENVIRONMENT=production` will boot:

1. `openfoundry-jwt` with key `JWT_SECRET` (unique, ≥ 32 characters, not the value in `.env.example`).
2. `openfoundry-db` with keys `{service}-database-url` and `{service}-migration-url` for each service that has a database (see `values.yaml`). Runtime URLs must use `openfoundry_app`. Migration URLs use the owner role.
3. `openfoundry-mtls` with `tls.crt`, `tls.key`, and `ca.crt`. Generate local material with `./scripts/gen-mtls-certs.sh`. The gateway uses the client cert; services use the server cert. In Kubernetes, mount the identity that matches the process.

```bash
helm template openfoundry infra/k8s/helm/open-foundry \
  -f infra/k8s/helm/open-foundry/values.yaml \
  -f infra/k8s/helm/open-foundry/values-prod.yaml
```

Create per-service databases listed in `infra/docker-compose.yml` (`POSTGRES_MULTIPLE_DATABASES`) and grant `openfoundry_app` as `infra/init-db/02-app-role.sh` does.

## Tests

```bash
cargo test -p auth-middleware -- --test-threads=1
cargo test -p service-runtime
cargo test --test tenant_isolation -- --test-threads=1
```
