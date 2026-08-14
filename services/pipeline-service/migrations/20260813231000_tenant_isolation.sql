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

ALTER TABLE pipelines ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE pipelines SET tenant_id = owner_id WHERE tenant_id IS NULL;
UPDATE pipelines SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE pipelines ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_pipelines_tenant ON pipelines (tenant_id);
DROP TRIGGER IF EXISTS trg_pipelines_fill_tenant ON pipelines;
CREATE TRIGGER trg_pipelines_fill_tenant BEFORE INSERT ON pipelines
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE pipeline_runs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE pipeline_runs r SET tenant_id = p.tenant_id FROM pipelines p WHERE r.pipeline_id = p.id AND r.tenant_id IS NULL;
UPDATE pipeline_runs SET tenant_id = started_by WHERE tenant_id IS NULL AND started_by IS NOT NULL;
UPDATE pipeline_runs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE pipeline_runs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_pipeline_runs_tenant ON pipeline_runs (tenant_id);
DROP TRIGGER IF EXISTS trg_pipeline_runs_fill_tenant ON pipeline_runs;
CREATE TRIGGER trg_pipeline_runs_fill_tenant BEFORE INSERT ON pipeline_runs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE lineage_edges ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE lineage_edges e SET tenant_id = p.tenant_id FROM pipelines p WHERE e.pipeline_id = p.id AND e.tenant_id IS NULL;
UPDATE lineage_edges SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE lineage_edges ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_lineage_edges_tenant ON lineage_edges (tenant_id);
DROP TRIGGER IF EXISTS trg_lineage_edges_fill_tenant ON lineage_edges;
CREATE TRIGGER trg_lineage_edges_fill_tenant BEFORE INSERT ON lineage_edges
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE column_lineage_edges ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE column_lineage_edges e SET tenant_id = p.tenant_id FROM pipelines p WHERE e.pipeline_id = p.id AND e.tenant_id IS NULL;
UPDATE column_lineage_edges SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE column_lineage_edges ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_column_lineage_edges_tenant ON column_lineage_edges (tenant_id);
DROP TRIGGER IF EXISTS trg_column_lineage_edges_fill_tenant ON column_lineage_edges;
CREATE TRIGGER trg_column_lineage_edges_fill_tenant BEFORE INSERT ON column_lineage_edges
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'pipelines',
        'pipeline_runs',
        'lineage_edges',
        'column_lineage_edges'
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

CREATE OR REPLACE FUNCTION openfoundry_due_pipelines()
RETURNS TABLE (id uuid, tenant_id uuid)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
  SELECT id, tenant_id FROM pipelines
  WHERE status = 'active' AND next_run_at IS NOT NULL AND next_run_at <= NOW()
$$;
GRANT EXECUTE ON FUNCTION openfoundry_due_pipelines() TO PUBLIC;
