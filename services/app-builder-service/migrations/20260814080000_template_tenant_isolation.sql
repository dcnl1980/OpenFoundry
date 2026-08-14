-- Isolate app template catalog.

ALTER TABLE app_templates ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE app_templates SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE app_templates ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_app_templates_tenant ON app_templates (tenant_id);
ALTER TABLE app_templates DROP CONSTRAINT IF EXISTS app_templates_key_key;
DROP INDEX IF EXISTS app_templates_key_key;
ALTER TABLE app_templates DROP CONSTRAINT IF EXISTS app_templates_tenant_id_key_key;
ALTER TABLE app_templates ADD CONSTRAINT app_templates_tenant_id_key_key UNIQUE (tenant_id, key);
DROP TRIGGER IF EXISTS trg_app_templates_fill_tenant ON app_templates;
CREATE TRIGGER trg_app_templates_fill_tenant BEFORE INSERT ON app_templates
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE app_templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_templates FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON app_templates;
CREATE POLICY tenant_isolation ON app_templates
    USING (tenant_id = openfoundry_current_tenant())
    WITH CHECK (tenant_id = openfoundry_current_tenant());
