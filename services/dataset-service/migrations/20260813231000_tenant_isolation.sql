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

ALTER TABLE datasets ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE datasets SET tenant_id = owner_id WHERE tenant_id IS NULL;
UPDATE datasets SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE datasets ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_datasets_tenant ON datasets (tenant_id);
DROP TRIGGER IF EXISTS trg_datasets_fill_tenant ON datasets;
CREATE TRIGGER trg_datasets_fill_tenant BEFORE INSERT ON datasets
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE dataset_schemas ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE dataset_schemas s SET tenant_id = d.tenant_id FROM datasets d WHERE s.dataset_id = d.id AND s.tenant_id IS NULL;
UPDATE dataset_schemas SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE dataset_schemas ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dataset_schemas_tenant ON dataset_schemas (tenant_id);
DROP TRIGGER IF EXISTS trg_dataset_schemas_fill_tenant ON dataset_schemas;
CREATE TRIGGER trg_dataset_schemas_fill_tenant BEFORE INSERT ON dataset_schemas
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE dataset_versions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE dataset_versions v SET tenant_id = d.tenant_id FROM datasets d WHERE v.dataset_id = d.id AND v.tenant_id IS NULL;
UPDATE dataset_versions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE dataset_versions ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dataset_versions_tenant ON dataset_versions (tenant_id);
DROP TRIGGER IF EXISTS trg_dataset_versions_fill_tenant ON dataset_versions;
CREATE TRIGGER trg_dataset_versions_fill_tenant BEFORE INSERT ON dataset_versions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'datasets',
        'dataset_schemas',
        'dataset_versions'
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
