-- Tenant partition + RLS for remaining dataset child tables.

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

ALTER TABLE dataset_branches ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE dataset_branches b SET tenant_id = d.tenant_id FROM datasets d WHERE b.dataset_id = d.id AND b.tenant_id IS NULL;
UPDATE dataset_branches SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE dataset_branches ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dataset_branches_tenant ON dataset_branches (tenant_id);
DROP TRIGGER IF EXISTS trg_dataset_branches_fill_tenant ON dataset_branches;
CREATE TRIGGER trg_dataset_branches_fill_tenant BEFORE INSERT ON dataset_branches
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE dataset_profiles ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE dataset_profiles p SET tenant_id = d.tenant_id FROM datasets d WHERE p.dataset_id = d.id AND p.tenant_id IS NULL;
UPDATE dataset_profiles SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE dataset_profiles ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dataset_profiles_tenant ON dataset_profiles (tenant_id);
DROP TRIGGER IF EXISTS trg_dataset_profiles_fill_tenant ON dataset_profiles;
CREATE TRIGGER trg_dataset_profiles_fill_tenant BEFORE INSERT ON dataset_profiles
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE dataset_quality_rules ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE dataset_quality_rules r SET tenant_id = d.tenant_id FROM datasets d WHERE r.dataset_id = d.id AND r.tenant_id IS NULL;
UPDATE dataset_quality_rules SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE dataset_quality_rules ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dataset_quality_rules_tenant ON dataset_quality_rules (tenant_id);
DROP TRIGGER IF EXISTS trg_dataset_quality_rules_fill_tenant ON dataset_quality_rules;
CREATE TRIGGER trg_dataset_quality_rules_fill_tenant BEFORE INSERT ON dataset_quality_rules
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE dataset_quality_history ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE dataset_quality_history h SET tenant_id = d.tenant_id FROM datasets d WHERE h.dataset_id = d.id AND h.tenant_id IS NULL;
UPDATE dataset_quality_history SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE dataset_quality_history ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dataset_quality_history_tenant ON dataset_quality_history (tenant_id);
DROP TRIGGER IF EXISTS trg_dataset_quality_history_fill_tenant ON dataset_quality_history;
CREATE TRIGGER trg_dataset_quality_history_fill_tenant BEFORE INSERT ON dataset_quality_history
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE dataset_quality_alerts ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE dataset_quality_alerts a SET tenant_id = d.tenant_id FROM datasets d WHERE a.dataset_id = d.id AND a.tenant_id IS NULL;
UPDATE dataset_quality_alerts SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE dataset_quality_alerts ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dataset_quality_alerts_tenant ON dataset_quality_alerts (tenant_id);
DROP TRIGGER IF EXISTS trg_dataset_quality_alerts_fill_tenant ON dataset_quality_alerts;
CREATE TRIGGER trg_dataset_quality_alerts_fill_tenant BEFORE INSERT ON dataset_quality_alerts
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'dataset_branches',
        'dataset_profiles',
        'dataset_quality_rules',
        'dataset_quality_history',
        'dataset_quality_alerts'
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
