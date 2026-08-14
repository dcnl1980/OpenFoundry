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

ALTER TABLE report_definitions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE report_definitions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE report_definitions ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_report_definitions_tenant ON report_definitions (tenant_id);
DROP TRIGGER IF EXISTS trg_report_definitions_fill_tenant ON report_definitions;
CREATE TRIGGER trg_report_definitions_fill_tenant BEFORE INSERT ON report_definitions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE report_executions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE report_executions e SET tenant_id = d.tenant_id FROM report_definitions d WHERE e.report_id = d.id AND e.tenant_id IS NULL;
UPDATE report_executions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE report_executions ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_report_executions_tenant ON report_executions (tenant_id);
DROP TRIGGER IF EXISTS trg_report_executions_fill_tenant ON report_executions;
CREATE TRIGGER trg_report_executions_fill_tenant BEFORE INSERT ON report_executions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'report_definitions',
        'report_executions'
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
