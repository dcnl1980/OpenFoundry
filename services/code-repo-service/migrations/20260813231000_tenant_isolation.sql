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

ALTER TABLE code_repositories ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_repositories SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_repositories ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE code_repositories DROP CONSTRAINT IF EXISTS code_repositories_slug_key;
ALTER TABLE code_repositories DROP CONSTRAINT IF EXISTS code_repositories_tenant_slug_key;
ALTER TABLE code_repositories ADD CONSTRAINT code_repositories_tenant_slug_key UNIQUE (tenant_id, slug);
CREATE INDEX IF NOT EXISTS idx_code_repositories_tenant ON code_repositories (tenant_id);
DROP TRIGGER IF EXISTS trg_code_repositories_fill_tenant ON code_repositories;
CREATE TRIGGER trg_code_repositories_fill_tenant BEFORE INSERT ON code_repositories
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE code_repository_branches ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_repository_branches b SET tenant_id = r.tenant_id FROM code_repositories r WHERE b.repository_id = r.id AND b.tenant_id IS NULL;
UPDATE code_repository_branches SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_repository_branches ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_repository_branches_tenant ON code_repository_branches (tenant_id);
DROP TRIGGER IF EXISTS trg_code_repository_branches_fill_tenant ON code_repository_branches;
CREATE TRIGGER trg_code_repository_branches_fill_tenant BEFORE INSERT ON code_repository_branches
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE code_repository_commits ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_repository_commits c SET tenant_id = r.tenant_id FROM code_repositories r WHERE c.repository_id = r.id AND c.tenant_id IS NULL;
UPDATE code_repository_commits SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_repository_commits ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_repository_commits_tenant ON code_repository_commits (tenant_id);
DROP TRIGGER IF EXISTS trg_code_repository_commits_fill_tenant ON code_repository_commits;
CREATE TRIGGER trg_code_repository_commits_fill_tenant BEFORE INSERT ON code_repository_commits
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE code_repository_files ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_repository_files f SET tenant_id = r.tenant_id FROM code_repositories r WHERE f.repository_id = r.id AND f.tenant_id IS NULL;
UPDATE code_repository_files SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_repository_files ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_repository_files_tenant ON code_repository_files (tenant_id);
DROP TRIGGER IF EXISTS trg_code_repository_files_fill_tenant ON code_repository_files;
CREATE TRIGGER trg_code_repository_files_fill_tenant BEFORE INSERT ON code_repository_files
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE code_merge_requests ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_merge_requests m SET tenant_id = r.tenant_id FROM code_repositories r WHERE m.repository_id = r.id AND m.tenant_id IS NULL;
UPDATE code_merge_requests SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_merge_requests ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_merge_requests_tenant ON code_merge_requests (tenant_id);
DROP TRIGGER IF EXISTS trg_code_merge_requests_fill_tenant ON code_merge_requests;
CREATE TRIGGER trg_code_merge_requests_fill_tenant BEFORE INSERT ON code_merge_requests
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE code_review_comments ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_review_comments c SET tenant_id = m.tenant_id FROM code_merge_requests m WHERE c.merge_request_id = m.id AND c.tenant_id IS NULL;
UPDATE code_review_comments SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_review_comments ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_review_comments_tenant ON code_review_comments (tenant_id);
DROP TRIGGER IF EXISTS trg_code_review_comments_fill_tenant ON code_review_comments;
CREATE TRIGGER trg_code_review_comments_fill_tenant BEFORE INSERT ON code_review_comments
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE code_ci_runs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_ci_runs c SET tenant_id = r.tenant_id FROM code_repositories r WHERE c.repository_id = r.id AND c.tenant_id IS NULL;
UPDATE code_ci_runs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_ci_runs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_ci_runs_tenant ON code_ci_runs (tenant_id);
DROP TRIGGER IF EXISTS trg_code_ci_runs_fill_tenant ON code_ci_runs;
CREATE TRIGGER trg_code_ci_runs_fill_tenant BEFORE INSERT ON code_ci_runs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE code_repository_integrations ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_repository_integrations i SET tenant_id = r.tenant_id FROM code_repositories r WHERE i.repository_id = r.id AND i.tenant_id IS NULL;
UPDATE code_repository_integrations SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_repository_integrations ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_repository_integrations_tenant ON code_repository_integrations (tenant_id);
DROP TRIGGER IF EXISTS trg_code_repository_integrations_fill_tenant ON code_repository_integrations;
CREATE TRIGGER trg_code_repository_integrations_fill_tenant BEFORE INSERT ON code_repository_integrations
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE code_repository_sync_runs ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE code_repository_sync_runs s SET tenant_id = r.tenant_id FROM code_repositories r WHERE s.repository_id = r.id AND s.tenant_id IS NULL;
UPDATE code_repository_sync_runs SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE code_repository_sync_runs ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_repository_sync_runs_tenant ON code_repository_sync_runs (tenant_id);
DROP TRIGGER IF EXISTS trg_code_repository_sync_runs_fill_tenant ON code_repository_sync_runs;
CREATE TRIGGER trg_code_repository_sync_runs_fill_tenant BEFORE INSERT ON code_repository_sync_runs
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'code_repositories',
        'code_repository_branches',
        'code_repository_commits',
        'code_repository_files',
        'code_merge_requests',
        'code_review_comments',
        'code_ci_runs',
        'code_repository_integrations',
        'code_repository_sync_runs'
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
