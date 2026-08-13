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

ALTER TABLE saved_queries ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE saved_queries SET tenant_id = owner_id WHERE tenant_id IS NULL;
UPDATE saved_queries SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE saved_queries ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_saved_queries_tenant ON saved_queries (tenant_id);
DROP TRIGGER IF EXISTS trg_saved_queries_fill_tenant ON saved_queries;
CREATE TRIGGER trg_saved_queries_fill_tenant BEFORE INSERT ON saved_queries
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'saved_queries'
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
