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

ALTER TABLE notebooks ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE notebooks SET tenant_id = owner_id WHERE tenant_id IS NULL;
UPDATE notebooks SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE notebooks ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_notebooks_tenant ON notebooks (tenant_id);
DROP TRIGGER IF EXISTS trg_notebooks_fill_tenant ON notebooks;
CREATE TRIGGER trg_notebooks_fill_tenant BEFORE INSERT ON notebooks
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE cells ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE cells c SET tenant_id = n.tenant_id FROM notebooks n WHERE c.notebook_id = n.id AND c.tenant_id IS NULL;
UPDATE cells SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE cells ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_cells_tenant ON cells (tenant_id);
DROP TRIGGER IF EXISTS trg_cells_fill_tenant ON cells;
CREATE TRIGGER trg_cells_fill_tenant BEFORE INSERT ON cells
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE sessions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE sessions s SET tenant_id = n.tenant_id FROM notebooks n WHERE s.notebook_id = n.id AND s.tenant_id IS NULL;
UPDATE sessions SET tenant_id = started_by WHERE tenant_id IS NULL;
UPDATE sessions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE sessions ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sessions_tenant ON sessions (tenant_id);
DROP TRIGGER IF EXISTS trg_sessions_fill_tenant ON sessions;
CREATE TRIGGER trg_sessions_fill_tenant BEFORE INSERT ON sessions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'notebooks',
        'cells',
        'sessions'
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
