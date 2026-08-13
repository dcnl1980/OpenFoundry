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

ALTER TABLE ml_experiments ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ml_experiments SET tenant_id = owner_id WHERE tenant_id IS NULL;
UPDATE ml_experiments SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ml_experiments ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ml_experiments_tenant ON ml_experiments (tenant_id);
DROP TRIGGER IF EXISTS trg_ml_experiments_fill_tenant ON ml_experiments;
CREATE TRIGGER trg_ml_experiments_fill_tenant BEFORE INSERT ON ml_experiments
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ml_runs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ml_runs r SET tenant_id = e.tenant_id FROM ml_experiments e WHERE r.experiment_id = e.id AND r.tenant_id IS NULL;
UPDATE ml_runs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ml_runs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ml_runs_tenant ON ml_runs (tenant_id);
DROP TRIGGER IF EXISTS trg_ml_runs_fill_tenant ON ml_runs;
CREATE TRIGGER trg_ml_runs_fill_tenant BEFORE INSERT ON ml_runs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ml_models ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ml_models SET tenant_id = owner_id WHERE tenant_id IS NULL;
UPDATE ml_models SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ml_models ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ml_models_tenant ON ml_models (tenant_id);
DROP TRIGGER IF EXISTS trg_ml_models_fill_tenant ON ml_models;
CREATE TRIGGER trg_ml_models_fill_tenant BEFORE INSERT ON ml_models
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ml_model_versions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ml_model_versions v SET tenant_id = m.tenant_id FROM ml_models m WHERE v.model_id = m.id AND v.tenant_id IS NULL;
UPDATE ml_model_versions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ml_model_versions ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ml_model_versions_tenant ON ml_model_versions (tenant_id);
DROP TRIGGER IF EXISTS trg_ml_model_versions_fill_tenant ON ml_model_versions;
CREATE TRIGGER trg_ml_model_versions_fill_tenant BEFORE INSERT ON ml_model_versions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ml_features ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ml_features SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ml_features ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ml_features_tenant ON ml_features (tenant_id);
DROP TRIGGER IF EXISTS trg_ml_features_fill_tenant ON ml_features;
CREATE TRIGGER trg_ml_features_fill_tenant BEFORE INSERT ON ml_features
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ml_training_jobs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ml_training_jobs j SET tenant_id = e.tenant_id FROM ml_experiments e WHERE j.experiment_id = e.id AND j.tenant_id IS NULL;
UPDATE ml_training_jobs j SET tenant_id = m.tenant_id FROM ml_models m WHERE j.model_id = m.id AND j.tenant_id IS NULL;
UPDATE ml_training_jobs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ml_training_jobs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ml_training_jobs_tenant ON ml_training_jobs (tenant_id);
DROP TRIGGER IF EXISTS trg_ml_training_jobs_fill_tenant ON ml_training_jobs;
CREATE TRIGGER trg_ml_training_jobs_fill_tenant BEFORE INSERT ON ml_training_jobs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE ml_deployments ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ml_deployments d SET tenant_id = m.tenant_id FROM ml_models m WHERE d.model_id = m.id AND d.tenant_id IS NULL;
UPDATE ml_deployments SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ml_deployments ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ml_deployments_tenant ON ml_deployments (tenant_id);
DROP TRIGGER IF EXISTS trg_ml_deployments_fill_tenant ON ml_deployments;
CREATE TRIGGER trg_ml_deployments_fill_tenant BEFORE INSERT ON ml_deployments
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();
ALTER TABLE ml_deployments DROP CONSTRAINT IF EXISTS ml_deployments_endpoint_path_key;
ALTER TABLE ml_deployments DROP CONSTRAINT IF EXISTS ml_deployments_tenant_endpoint_path_key;
ALTER TABLE ml_deployments ADD CONSTRAINT ml_deployments_tenant_endpoint_path_key UNIQUE (tenant_id, endpoint_path);

ALTER TABLE ml_batch_predictions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE ml_batch_predictions p SET tenant_id = d.tenant_id FROM ml_deployments d WHERE p.deployment_id = d.id AND p.tenant_id IS NULL;
UPDATE ml_batch_predictions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE ml_batch_predictions ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ml_batch_predictions_tenant ON ml_batch_predictions (tenant_id);
DROP TRIGGER IF EXISTS trg_ml_batch_predictions_fill_tenant ON ml_batch_predictions;
CREATE TRIGGER trg_ml_batch_predictions_fill_tenant BEFORE INSERT ON ml_batch_predictions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'ml_experiments',
        'ml_runs',
        'ml_models',
        'ml_model_versions',
        'ml_features',
        'ml_training_jobs',
        'ml_deployments',
        'ml_batch_predictions'
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
