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

ALTER TABLE fusion_match_rules ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE fusion_match_rules SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE fusion_match_rules ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fusion_match_rules_tenant ON fusion_match_rules (tenant_id);
DROP TRIGGER IF EXISTS trg_fusion_match_rules_fill_tenant ON fusion_match_rules;
CREATE TRIGGER trg_fusion_match_rules_fill_tenant BEFORE INSERT ON fusion_match_rules
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE fusion_merge_strategies ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE fusion_merge_strategies SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE fusion_merge_strategies ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fusion_merge_strategies_tenant ON fusion_merge_strategies (tenant_id);
DROP TRIGGER IF EXISTS trg_fusion_merge_strategies_fill_tenant ON fusion_merge_strategies;
CREATE TRIGGER trg_fusion_merge_strategies_fill_tenant BEFORE INSERT ON fusion_merge_strategies
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE fusion_jobs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE fusion_jobs j SET tenant_id = r.tenant_id FROM fusion_match_rules r WHERE j.match_rule_id = r.id AND j.tenant_id IS NULL;
UPDATE fusion_jobs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE fusion_jobs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fusion_jobs_tenant ON fusion_jobs (tenant_id);
DROP TRIGGER IF EXISTS trg_fusion_jobs_fill_tenant ON fusion_jobs;
CREATE TRIGGER trg_fusion_jobs_fill_tenant BEFORE INSERT ON fusion_jobs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE fusion_clusters ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE fusion_clusters c SET tenant_id = j.tenant_id FROM fusion_jobs j WHERE c.job_id = j.id AND c.tenant_id IS NULL;
UPDATE fusion_clusters SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE fusion_clusters ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fusion_clusters_tenant ON fusion_clusters (tenant_id);
DROP TRIGGER IF EXISTS trg_fusion_clusters_fill_tenant ON fusion_clusters;
CREATE TRIGGER trg_fusion_clusters_fill_tenant BEFORE INSERT ON fusion_clusters
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE fusion_review_queue ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE fusion_review_queue q SET tenant_id = c.tenant_id FROM fusion_clusters c WHERE q.cluster_id = c.id AND q.tenant_id IS NULL;
UPDATE fusion_review_queue SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE fusion_review_queue ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fusion_review_queue_tenant ON fusion_review_queue (tenant_id);
DROP TRIGGER IF EXISTS trg_fusion_review_queue_fill_tenant ON fusion_review_queue;
CREATE TRIGGER trg_fusion_review_queue_fill_tenant BEFORE INSERT ON fusion_review_queue
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE fusion_golden_records ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE fusion_golden_records g SET tenant_id = c.tenant_id FROM fusion_clusters c WHERE g.cluster_id = c.id AND g.tenant_id IS NULL;
UPDATE fusion_golden_records SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE fusion_golden_records ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fusion_golden_records_tenant ON fusion_golden_records (tenant_id);
DROP TRIGGER IF EXISTS trg_fusion_golden_records_fill_tenant ON fusion_golden_records;
CREATE TRIGGER trg_fusion_golden_records_fill_tenant BEFORE INSERT ON fusion_golden_records
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'fusion_match_rules',
        'fusion_merge_strategies',
        'fusion_jobs',
        'fusion_clusters',
        'fusion_review_queue',
        'fusion_golden_records'
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
