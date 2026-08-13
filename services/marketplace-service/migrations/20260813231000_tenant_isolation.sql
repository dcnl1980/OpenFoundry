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

ALTER TABLE marketplace_listings ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE marketplace_listings SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE marketplace_listings ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE marketplace_listings DROP CONSTRAINT IF EXISTS marketplace_listings_slug_key;
ALTER TABLE marketplace_listings DROP CONSTRAINT IF EXISTS marketplace_listings_tenant_slug_key;
ALTER TABLE marketplace_listings ADD CONSTRAINT marketplace_listings_tenant_slug_key UNIQUE (tenant_id, slug);
CREATE INDEX IF NOT EXISTS idx_marketplace_listings_tenant ON marketplace_listings (tenant_id);
DROP TRIGGER IF EXISTS trg_marketplace_listings_fill_tenant ON marketplace_listings;
CREATE TRIGGER trg_marketplace_listings_fill_tenant BEFORE INSERT ON marketplace_listings
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE marketplace_package_versions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE marketplace_package_versions v SET tenant_id = l.tenant_id FROM marketplace_listings l WHERE v.listing_id = l.id AND v.tenant_id IS NULL;
UPDATE marketplace_package_versions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE marketplace_package_versions ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_marketplace_package_versions_tenant ON marketplace_package_versions (tenant_id);
DROP TRIGGER IF EXISTS trg_marketplace_package_versions_fill_tenant ON marketplace_package_versions;
CREATE TRIGGER trg_marketplace_package_versions_fill_tenant BEFORE INSERT ON marketplace_package_versions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE marketplace_reviews ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE marketplace_reviews r SET tenant_id = l.tenant_id FROM marketplace_listings l WHERE r.listing_id = l.id AND r.tenant_id IS NULL;
UPDATE marketplace_reviews SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE marketplace_reviews ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_marketplace_reviews_tenant ON marketplace_reviews (tenant_id);
DROP TRIGGER IF EXISTS trg_marketplace_reviews_fill_tenant ON marketplace_reviews;
CREATE TRIGGER trg_marketplace_reviews_fill_tenant BEFORE INSERT ON marketplace_reviews
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE marketplace_installs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE marketplace_installs i SET tenant_id = l.tenant_id FROM marketplace_listings l WHERE i.listing_id = l.id AND i.tenant_id IS NULL;
UPDATE marketplace_installs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE marketplace_installs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_marketplace_installs_tenant ON marketplace_installs (tenant_id);
DROP TRIGGER IF EXISTS trg_marketplace_installs_fill_tenant ON marketplace_installs;
CREATE TRIGGER trg_marketplace_installs_fill_tenant BEFORE INSERT ON marketplace_installs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'marketplace_listings',
        'marketplace_package_versions',
        'marketplace_reviews',
        'marketplace_installs'
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
