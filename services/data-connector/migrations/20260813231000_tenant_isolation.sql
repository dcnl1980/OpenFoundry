-- Tenant partition + PostgreSQL RLS.

CREATE OR REPLACE FUNCTION openfoundry_current_tenant() RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT NULLIF(current_setting('openfoundry.tenant_id', true), '')::uuid
$$;

CREATE OR REPLACE FUNCTION openfoundry_fill_tenant() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.tenant_id IS NULL THEN
        NEW.tenant_id := openfoundry_current_tenant();
    END IF;
    RETURN NEW;
END
$$;

ALTER TABLE connections ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE connections SET tenant_id = owner_id WHERE tenant_id IS NULL;
UPDATE connections SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE connections ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_connections_tenant ON connections (tenant_id);
DROP TRIGGER IF EXISTS trg_connections_fill_tenant ON connections;
CREATE TRIGGER trg_connections_fill_tenant BEFORE INSERT ON connections
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE sync_jobs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE sync_jobs j SET tenant_id = c.tenant_id FROM connections c WHERE j.connection_id = c.id AND j.tenant_id IS NULL;
UPDATE sync_jobs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE sync_jobs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sync_jobs_tenant ON sync_jobs (tenant_id);
DROP TRIGGER IF EXISTS trg_sync_jobs_fill_tenant ON sync_jobs;
CREATE TRIGGER trg_sync_jobs_fill_tenant BEFORE INSERT ON sync_jobs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'connections',
        'sync_jobs'
    ]
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', tbl);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', tbl);
        EXECUTE format('DROP POLICY IF EXISTS tenant_isolation ON %I', tbl);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I
             USING (tenant_id = openfoundry_current_tenant())
             WITH CHECK (tenant_id = openfoundry_current_tenant())',
            tbl
        );
    END LOOP;
END
$$;
