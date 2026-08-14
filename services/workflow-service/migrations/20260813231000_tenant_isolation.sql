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

ALTER TABLE workflows ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE workflows SET tenant_id = owner_id WHERE tenant_id IS NULL;
UPDATE workflows SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE workflows ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_workflows_tenant ON workflows (tenant_id);
DROP TRIGGER IF EXISTS trg_workflows_fill_tenant ON workflows;
CREATE TRIGGER trg_workflows_fill_tenant BEFORE INSERT ON workflows
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE workflow_runs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE workflow_runs r SET tenant_id = w.tenant_id FROM workflows w WHERE r.workflow_id = w.id AND r.tenant_id IS NULL;
UPDATE workflow_runs SET tenant_id = started_by WHERE tenant_id IS NULL AND started_by IS NOT NULL;
UPDATE workflow_runs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE workflow_runs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_workflow_runs_tenant ON workflow_runs (tenant_id);
DROP TRIGGER IF EXISTS trg_workflow_runs_fill_tenant ON workflow_runs;
CREATE TRIGGER trg_workflow_runs_fill_tenant BEFORE INSERT ON workflow_runs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE workflow_approvals ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE workflow_approvals a SET tenant_id = w.tenant_id FROM workflows w WHERE a.workflow_id = w.id AND a.tenant_id IS NULL;
UPDATE workflow_approvals SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE workflow_approvals ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_workflow_approvals_tenant ON workflow_approvals (tenant_id);
DROP TRIGGER IF EXISTS trg_workflow_approvals_fill_tenant ON workflow_approvals;
CREATE TRIGGER trg_workflow_approvals_fill_tenant BEFORE INSERT ON workflow_approvals
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'workflows',
        'workflow_runs',
        'workflow_approvals'
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

CREATE OR REPLACE FUNCTION openfoundry_due_workflows()
RETURNS TABLE (id uuid, tenant_id uuid)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
  SELECT id, tenant_id FROM workflows
  WHERE status = 'active'
    AND trigger_type = 'cron'
    AND next_run_at IS NOT NULL
    AND next_run_at <= NOW()
$$;
GRANT EXECUTE ON FUNCTION openfoundry_due_workflows() TO PUBLIC;

CREATE OR REPLACE FUNCTION openfoundry_webhook_workflow(p_id uuid)
RETURNS TABLE (id uuid, tenant_id uuid)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
  SELECT id, tenant_id FROM workflows WHERE id = p_id
$$;
GRANT EXECUTE ON FUNCTION openfoundry_webhook_workflow(uuid) TO PUBLIC;
