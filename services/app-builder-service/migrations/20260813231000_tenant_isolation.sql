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

ALTER TABLE apps ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE apps SET tenant_id = created_by WHERE tenant_id IS NULL;
UPDATE apps SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE apps ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_apps_tenant ON apps (tenant_id);
ALTER TABLE apps DROP CONSTRAINT IF EXISTS apps_slug_key;
DROP INDEX IF EXISTS apps_slug_key;
ALTER TABLE apps DROP CONSTRAINT IF EXISTS apps_tenant_slug_key;
ALTER TABLE apps ADD CONSTRAINT apps_tenant_slug_key UNIQUE (tenant_id, slug);
DROP TRIGGER IF EXISTS trg_apps_fill_tenant ON apps;
CREATE TRIGGER trg_apps_fill_tenant BEFORE INSERT ON apps
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE app_versions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE app_versions v
SET tenant_id = a.tenant_id
FROM apps a
WHERE v.app_id = a.id AND v.tenant_id IS NULL;
UPDATE app_versions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE app_versions ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_app_versions_tenant ON app_versions (tenant_id);
DROP TRIGGER IF EXISTS trg_app_versions_fill_tenant ON app_versions;
CREATE TRIGGER trg_app_versions_fill_tenant BEFORE INSERT ON app_versions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'apps',
        'app_versions'
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
