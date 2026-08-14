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

ALTER TABLE ai_prompt_templates ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ai_prompt_templates SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ai_prompt_templates ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_prompt_templates_tenant ON ai_prompt_templates (tenant_id);
DROP TRIGGER IF EXISTS trg_ai_prompt_templates_fill_tenant ON ai_prompt_templates;
CREATE TRIGGER trg_ai_prompt_templates_fill_tenant BEFORE INSERT ON ai_prompt_templates
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ai_knowledge_bases ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ai_knowledge_bases SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ai_knowledge_bases ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_knowledge_bases_tenant ON ai_knowledge_bases (tenant_id);
DROP TRIGGER IF EXISTS trg_ai_knowledge_bases_fill_tenant ON ai_knowledge_bases;
CREATE TRIGGER trg_ai_knowledge_bases_fill_tenant BEFORE INSERT ON ai_knowledge_bases
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ai_knowledge_documents ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ai_knowledge_documents d SET tenant_id = b.tenant_id FROM ai_knowledge_bases b WHERE d.knowledge_base_id = b.id AND d.tenant_id IS NULL;
UPDATE ai_knowledge_documents SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ai_knowledge_documents ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_knowledge_documents_tenant ON ai_knowledge_documents (tenant_id);
DROP TRIGGER IF EXISTS trg_ai_knowledge_documents_fill_tenant ON ai_knowledge_documents;
CREATE TRIGGER trg_ai_knowledge_documents_fill_tenant BEFORE INSERT ON ai_knowledge_documents
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ai_agents ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ai_agents SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ai_agents ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_agents_tenant ON ai_agents (tenant_id);
DROP TRIGGER IF EXISTS trg_ai_agents_fill_tenant ON ai_agents;
CREATE TRIGGER trg_ai_agents_fill_tenant BEFORE INSERT ON ai_agents
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ai_conversations ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ai_conversations SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ai_conversations ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_conversations_tenant ON ai_conversations (tenant_id);
DROP TRIGGER IF EXISTS trg_ai_conversations_fill_tenant ON ai_conversations;
CREATE TRIGGER trg_ai_conversations_fill_tenant BEFORE INSERT ON ai_conversations
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ai_semantic_cache ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ai_semantic_cache SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ai_semantic_cache ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ai_semantic_cache_tenant ON ai_semantic_cache (tenant_id);
ALTER TABLE ai_semantic_cache DROP CONSTRAINT IF EXISTS ai_semantic_cache_kind_cache_key_key;
ALTER TABLE ai_semantic_cache DROP CONSTRAINT IF EXISTS ai_semantic_cache_tenant_id_kind_cache_key_key;
ALTER TABLE ai_semantic_cache ADD CONSTRAINT ai_semantic_cache_tenant_id_kind_cache_key_key UNIQUE (tenant_id, kind, cache_key);
DROP TRIGGER IF EXISTS trg_ai_semantic_cache_fill_tenant ON ai_semantic_cache;
CREATE TRIGGER trg_ai_semantic_cache_fill_tenant BEFORE INSERT ON ai_semantic_cache
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'ai_prompt_templates',
        'ai_knowledge_bases',
        'ai_knowledge_documents',
        'ai_agents',
        'ai_conversations',
        'ai_semantic_cache'
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
