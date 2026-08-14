-- Isolate platform AI catalogs: providers and tools.

ALTER TABLE ai_providers ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ai_providers SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ai_providers ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_providers_tenant ON ai_providers (tenant_id);
ALTER TABLE ai_providers DROP CONSTRAINT IF EXISTS ai_providers_tenant_id_name_key;
ALTER TABLE ai_providers ADD CONSTRAINT ai_providers_tenant_id_name_key UNIQUE (tenant_id, name);
DROP TRIGGER IF EXISTS trg_ai_providers_fill_tenant ON ai_providers;
CREATE TRIGGER trg_ai_providers_fill_tenant BEFORE INSERT ON ai_providers
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ai_tools ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ai_tools SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ai_tools ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_tools_tenant ON ai_tools (tenant_id);
ALTER TABLE ai_tools DROP CONSTRAINT IF EXISTS ai_tools_tenant_id_name_key;
ALTER TABLE ai_tools ADD CONSTRAINT ai_tools_tenant_id_name_key UNIQUE (tenant_id, name);
DROP TRIGGER IF EXISTS trg_ai_tools_fill_tenant ON ai_tools;
CREATE TRIGGER trg_ai_tools_fill_tenant BEFORE INSERT ON ai_tools
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'ai_providers',
        'ai_tools'
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
