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

ALTER TABLE streaming_streams ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE streaming_streams SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE streaming_streams ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_streaming_streams_tenant ON streaming_streams (tenant_id);
DROP TRIGGER IF EXISTS trg_streaming_streams_fill_tenant ON streaming_streams;
CREATE TRIGGER trg_streaming_streams_fill_tenant BEFORE INSERT ON streaming_streams
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE streaming_windows ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE streaming_windows SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE streaming_windows ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_streaming_windows_tenant ON streaming_windows (tenant_id);
DROP TRIGGER IF EXISTS trg_streaming_windows_fill_tenant ON streaming_windows;
CREATE TRIGGER trg_streaming_windows_fill_tenant BEFORE INSERT ON streaming_windows
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE streaming_topologies ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE streaming_topologies SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE streaming_topologies ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_streaming_topologies_tenant ON streaming_topologies (tenant_id);
DROP TRIGGER IF EXISTS trg_streaming_topologies_fill_tenant ON streaming_topologies;
CREATE TRIGGER trg_streaming_topologies_fill_tenant BEFORE INSERT ON streaming_topologies
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE streaming_topology_runs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE streaming_topology_runs r
SET tenant_id = t.tenant_id
FROM streaming_topologies t
WHERE r.topology_id = t.id AND r.tenant_id IS NULL;
UPDATE streaming_topology_runs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE streaming_topology_runs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_streaming_topology_runs_tenant ON streaming_topology_runs (tenant_id);
DROP TRIGGER IF EXISTS trg_streaming_topology_runs_fill_tenant ON streaming_topology_runs;
CREATE TRIGGER trg_streaming_topology_runs_fill_tenant BEFORE INSERT ON streaming_topology_runs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'streaming_streams',
        'streaming_windows',
        'streaming_topologies',
        'streaming_topology_runs'
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
