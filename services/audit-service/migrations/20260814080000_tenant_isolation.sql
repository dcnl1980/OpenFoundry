-- Tenant partition + PostgreSQL RLS for audit.

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

ALTER TABLE audit_events ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE audit_events SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE audit_events ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_events_tenant ON audit_events (tenant_id);
ALTER TABLE audit_events DROP CONSTRAINT IF EXISTS audit_events_sequence_key;
DROP INDEX IF EXISTS audit_events_sequence_key;
ALTER TABLE audit_events DROP CONSTRAINT IF EXISTS audit_events_tenant_id_sequence_key;
ALTER TABLE audit_events ADD CONSTRAINT audit_events_tenant_id_sequence_key UNIQUE (tenant_id, sequence);
DROP TRIGGER IF EXISTS trg_audit_events_fill_tenant ON audit_events;
CREATE TRIGGER trg_audit_events_fill_tenant BEFORE INSERT ON audit_events
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE audit_policies ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE audit_policies SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE audit_policies ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_policies_tenant ON audit_policies (tenant_id);
DROP TRIGGER IF EXISTS trg_audit_policies_fill_tenant ON audit_policies;
CREATE TRIGGER trg_audit_policies_fill_tenant BEFORE INSERT ON audit_policies
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE compliance_reports ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE compliance_reports SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE compliance_reports ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_compliance_reports_tenant ON compliance_reports (tenant_id);
DROP TRIGGER IF EXISTS trg_compliance_reports_fill_tenant ON compliance_reports;
CREATE TRIGGER trg_compliance_reports_fill_tenant BEFORE INSERT ON compliance_reports
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'audit_events',
        'audit_policies',
        'compliance_reports'
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
