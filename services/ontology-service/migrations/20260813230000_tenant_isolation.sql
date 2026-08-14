-- Tenant partition + PostgreSQL RLS for every ontology resource.

CREATE OR REPLACE FUNCTION openfoundry_current_tenant() RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT NULLIF(current_setting('openfoundry.tenant_id', true), '')::uuid
$$;

ALTER TABLE object_types ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE object_types SET tenant_id = owner_id WHERE tenant_id IS NULL;
ALTER TABLE object_types ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE properties ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE properties p
SET tenant_id = t.tenant_id
FROM object_types t
WHERE p.object_type_id = t.id AND p.tenant_id IS NULL;
ALTER TABLE properties ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE link_types ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE link_types SET tenant_id = owner_id WHERE tenant_id IS NULL;
ALTER TABLE link_types ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE object_instances ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE object_instances SET tenant_id = created_by WHERE tenant_id IS NULL;
ALTER TABLE object_instances ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE link_instances ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE link_instances SET tenant_id = created_by WHERE tenant_id IS NULL;
ALTER TABLE link_instances ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE action_types ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE action_types SET tenant_id = owner_id WHERE tenant_id IS NULL;
ALTER TABLE action_types ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE object_types DROP CONSTRAINT IF EXISTS object_types_name_key;
ALTER TABLE object_types DROP CONSTRAINT IF EXISTS object_types_tenant_name_key;
ALTER TABLE object_types ADD CONSTRAINT object_types_tenant_name_key UNIQUE (tenant_id, name);

ALTER TABLE action_types DROP CONSTRAINT IF EXISTS action_types_name_key;
ALTER TABLE action_types DROP CONSTRAINT IF EXISTS action_types_tenant_name_key;
ALTER TABLE action_types ADD CONSTRAINT action_types_tenant_name_key UNIQUE (tenant_id, name);

ALTER TABLE link_types DROP CONSTRAINT IF EXISTS link_types_name_source_type_id_target_type_id_key;
ALTER TABLE link_types DROP CONSTRAINT IF EXISTS link_types_tenant_name_endpoints_key;
ALTER TABLE link_types ADD CONSTRAINT link_types_tenant_name_endpoints_key
    UNIQUE (tenant_id, name, source_type_id, target_type_id);

CREATE INDEX IF NOT EXISTS idx_object_types_tenant ON object_types (tenant_id);
CREATE INDEX IF NOT EXISTS idx_properties_tenant ON properties (tenant_id);
CREATE INDEX IF NOT EXISTS idx_link_types_tenant ON link_types (tenant_id);
CREATE INDEX IF NOT EXISTS idx_object_instances_tenant ON object_instances (tenant_id);
CREATE INDEX IF NOT EXISTS idx_link_instances_tenant ON link_instances (tenant_id);
CREATE INDEX IF NOT EXISTS idx_action_types_tenant ON action_types (tenant_id);

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'object_types',
        'properties',
        'link_types',
        'object_instances',
        'link_instances',
        'action_types'
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
