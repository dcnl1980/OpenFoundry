#!/bin/bash
set -euo pipefail

# Runtime role used by services. Superuser/BYPASSRLS silently skips FORCE RLS.
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
  DO \$\$
  BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'openfoundry_app') THEN
      CREATE ROLE openfoundry_app LOGIN PASSWORD 'openfoundry' NOSUPERUSER NOBYPASSRLS;
    END IF;
  END
  \$\$;
EOSQL

grant_app_role() {
  local db="$1"
  psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    GRANT CONNECT ON DATABASE "$db" TO openfoundry_app;
EOSQL
  psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$db" <<-EOSQL
    GRANT USAGE, CREATE ON SCHEMA public TO openfoundry_app;
    GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO openfoundry_app;
    GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO openfoundry_app;
    GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO openfoundry_app;
    ALTER DEFAULT PRIVILEGES FOR ROLE "$POSTGRES_USER" IN SCHEMA public GRANT ALL ON TABLES TO openfoundry_app;
    ALTER DEFAULT PRIVILEGES FOR ROLE "$POSTGRES_USER" IN SCHEMA public GRANT ALL ON SEQUENCES TO openfoundry_app;
    ALTER DEFAULT PRIVILEGES FOR ROLE "$POSTGRES_USER" IN SCHEMA public GRANT EXECUTE ON FUNCTIONS TO openfoundry_app;
EOSQL
}

grant_app_role "$POSTGRES_DB"

if [ -n "${POSTGRES_MULTIPLE_DATABASES:-}" ]; then
  IFS=',' read -ra DBS <<< "$POSTGRES_MULTIPLE_DATABASES"
  for db in "${DBS[@]}"; do
    db=$(echo "$db" | xargs)
    [ -n "$db" ] || continue
    grant_app_role "$db"
  done
fi
