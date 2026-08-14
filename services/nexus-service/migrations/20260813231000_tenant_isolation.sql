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

ALTER TABLE nexus_peers ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE nexus_peers SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE nexus_peers ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE nexus_peers DROP CONSTRAINT IF EXISTS nexus_peers_slug_key;
ALTER TABLE nexus_peers DROP CONSTRAINT IF EXISTS nexus_peers_tenant_slug_key;
ALTER TABLE nexus_peers ADD CONSTRAINT nexus_peers_tenant_slug_key UNIQUE (tenant_id, slug);
CREATE INDEX IF NOT EXISTS idx_nexus_peers_tenant ON nexus_peers (tenant_id);
DROP TRIGGER IF EXISTS trg_nexus_peers_fill_tenant ON nexus_peers;
CREATE TRIGGER trg_nexus_peers_fill_tenant BEFORE INSERT ON nexus_peers
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE nexus_contracts ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE nexus_contracts c SET tenant_id = p.tenant_id FROM nexus_peers p WHERE c.peer_id = p.id AND c.tenant_id IS NULL;
UPDATE nexus_contracts SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE nexus_contracts ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_nexus_contracts_tenant ON nexus_contracts (tenant_id);
DROP TRIGGER IF EXISTS trg_nexus_contracts_fill_tenant ON nexus_contracts;
CREATE TRIGGER trg_nexus_contracts_fill_tenant BEFORE INSERT ON nexus_contracts
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE nexus_shares ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE nexus_shares s SET tenant_id = c.tenant_id FROM nexus_contracts c WHERE s.contract_id = c.id AND s.tenant_id IS NULL;
UPDATE nexus_shares SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE nexus_shares ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_nexus_shares_tenant ON nexus_shares (tenant_id);
DROP TRIGGER IF EXISTS trg_nexus_shares_fill_tenant ON nexus_shares;
CREATE TRIGGER trg_nexus_shares_fill_tenant BEFORE INSERT ON nexus_shares
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE nexus_access_grants ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE nexus_access_grants g SET tenant_id = s.tenant_id FROM nexus_shares s WHERE g.share_id = s.id AND g.tenant_id IS NULL;
UPDATE nexus_access_grants SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE nexus_access_grants ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_nexus_access_grants_tenant ON nexus_access_grants (tenant_id);
DROP TRIGGER IF EXISTS trg_nexus_access_grants_fill_tenant ON nexus_access_grants;
CREATE TRIGGER trg_nexus_access_grants_fill_tenant BEFORE INSERT ON nexus_access_grants
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE nexus_sync_statuses ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE nexus_sync_statuses st SET tenant_id = s.tenant_id FROM nexus_shares s WHERE st.share_id = s.id AND st.tenant_id IS NULL;
UPDATE nexus_sync_statuses SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE nexus_sync_statuses ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_nexus_sync_statuses_tenant ON nexus_sync_statuses (tenant_id);
DROP TRIGGER IF EXISTS trg_nexus_sync_statuses_fill_tenant ON nexus_sync_statuses;
CREATE TRIGGER trg_nexus_sync_statuses_fill_tenant BEFORE INSERT ON nexus_sync_statuses
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'nexus_peers',
        'nexus_contracts',
        'nexus_shares',
        'nexus_access_grants',
        'nexus_sync_statuses'
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
