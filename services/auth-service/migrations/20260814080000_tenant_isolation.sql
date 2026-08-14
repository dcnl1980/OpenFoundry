-- Tenant partition + PostgreSQL RLS for control-plane auth.

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

-- users: personal workspace is the user id; org members share organization_id.
ALTER TABLE users ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE users SET tenant_id = COALESCE(organization_id, id) WHERE tenant_id IS NULL;
ALTER TABLE users ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_tenant ON users (tenant_id);
DROP TRIGGER IF EXISTS trg_users_fill_tenant ON users;
CREATE TRIGGER trg_users_fill_tenant BEFORE INSERT ON users
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE refresh_tokens t
SET tenant_id = u.tenant_id
FROM users u
WHERE t.user_id = u.id AND t.tenant_id IS NULL;
UPDATE refresh_tokens SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE refresh_tokens ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_tenant ON refresh_tokens (tenant_id);
DROP TRIGGER IF EXISTS trg_refresh_tokens_fill_tenant ON refresh_tokens;
CREATE TRIGGER trg_refresh_tokens_fill_tenant BEFORE INSERT ON refresh_tokens
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE groups ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE groups SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE groups ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_groups_tenant ON groups (tenant_id);
ALTER TABLE groups DROP CONSTRAINT IF EXISTS groups_name_key;
DROP INDEX IF EXISTS groups_name_key;
ALTER TABLE groups DROP CONSTRAINT IF EXISTS groups_tenant_id_name_key;
ALTER TABLE groups ADD CONSTRAINT groups_tenant_id_name_key UNIQUE (tenant_id, name);
DROP TRIGGER IF EXISTS trg_groups_fill_tenant ON groups;
CREATE TRIGGER trg_groups_fill_tenant BEFORE INSERT ON groups
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE group_members ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE group_members m
SET tenant_id = g.tenant_id
FROM groups g
WHERE m.group_id = g.id AND m.tenant_id IS NULL;
UPDATE group_members SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE group_members ALTER COLUMN tenant_id SET NOT NULL;
DROP TRIGGER IF EXISTS trg_group_members_fill_tenant ON group_members;
CREATE TRIGGER trg_group_members_fill_tenant BEFORE INSERT ON group_members
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE group_roles ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE group_roles gr
SET tenant_id = g.tenant_id
FROM groups g
WHERE gr.group_id = g.id AND gr.tenant_id IS NULL;
UPDATE group_roles SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE group_roles ALTER COLUMN tenant_id SET NOT NULL;
DROP TRIGGER IF EXISTS trg_group_roles_fill_tenant ON group_roles;
CREATE TRIGGER trg_group_roles_fill_tenant BEFORE INSERT ON group_roles
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE user_roles ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE user_roles ur
SET tenant_id = u.tenant_id
FROM users u
WHERE ur.user_id = u.id AND ur.tenant_id IS NULL;
UPDATE user_roles SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE user_roles ALTER COLUMN tenant_id SET NOT NULL;
DROP TRIGGER IF EXISTS trg_user_roles_fill_tenant ON user_roles;
CREATE TRIGGER trg_user_roles_fill_tenant BEFORE INSERT ON user_roles
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE roles ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE roles SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE roles ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_roles_tenant ON roles (tenant_id);
DROP TRIGGER IF EXISTS trg_roles_fill_tenant ON roles;
CREATE TRIGGER trg_roles_fill_tenant BEFORE INSERT ON roles
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE permissions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE permissions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE permissions ALTER COLUMN tenant_id SET NOT NULL;
DROP TRIGGER IF EXISTS trg_permissions_fill_tenant ON permissions;
CREATE TRIGGER trg_permissions_fill_tenant BEFORE INSERT ON permissions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE role_permissions ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE role_permissions rp
SET tenant_id = r.tenant_id
FROM roles r
WHERE rp.role_id = r.id AND rp.tenant_id IS NULL;
UPDATE role_permissions SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE role_permissions ALTER COLUMN tenant_id SET NOT NULL;
DROP TRIGGER IF EXISTS trg_role_permissions_fill_tenant ON role_permissions;
CREATE TRIGGER trg_role_permissions_fill_tenant BEFORE INSERT ON role_permissions
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE abac_policies ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE abac_policies SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE abac_policies ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_abac_policies_tenant ON abac_policies (tenant_id);
ALTER TABLE abac_policies DROP CONSTRAINT IF EXISTS abac_policies_name_key;
DROP INDEX IF EXISTS abac_policies_name_key;
ALTER TABLE abac_policies DROP CONSTRAINT IF EXISTS abac_policies_tenant_id_name_key;
ALTER TABLE abac_policies ADD CONSTRAINT abac_policies_tenant_id_name_key UNIQUE (tenant_id, name);
DROP TRIGGER IF EXISTS trg_abac_policies_fill_tenant ON abac_policies;
CREATE TRIGGER trg_abac_policies_fill_tenant BEFORE INSERT ON abac_policies
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE api_keys k
SET tenant_id = u.tenant_id
FROM users u
WHERE k.user_id = u.id AND k.tenant_id IS NULL;
UPDATE api_keys SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE api_keys ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant ON api_keys (tenant_id);
DROP TRIGGER IF EXISTS trg_api_keys_fill_tenant ON api_keys;
CREATE TRIGGER trg_api_keys_fill_tenant BEFORE INSERT ON api_keys
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE sso_providers ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE sso_providers SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE sso_providers ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sso_providers_tenant ON sso_providers (tenant_id);
DROP TRIGGER IF EXISTS trg_sso_providers_fill_tenant ON sso_providers;
CREATE TRIGGER trg_sso_providers_fill_tenant BEFORE INSERT ON sso_providers
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE external_identities ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE external_identities ei
SET tenant_id = u.tenant_id
FROM users u
WHERE ei.user_id = u.id AND ei.tenant_id IS NULL;
UPDATE external_identities SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE external_identities ALTER COLUMN tenant_id SET NOT NULL;
DROP TRIGGER IF EXISTS trg_external_identities_fill_tenant ON external_identities;
CREATE TRIGGER trg_external_identities_fill_tenant BEFORE INSERT ON external_identities
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE user_mfa_totp ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE user_mfa_totp m
SET tenant_id = u.tenant_id
FROM users u
WHERE m.user_id = u.id AND m.tenant_id IS NULL;
UPDATE user_mfa_totp SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE user_mfa_totp ALTER COLUMN tenant_id SET NOT NULL;
DROP TRIGGER IF EXISTS trg_user_mfa_totp_fill_tenant ON user_mfa_totp;
CREATE TRIGGER trg_user_mfa_totp_fill_tenant BEFORE INSERT ON user_mfa_totp
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'users',
        'refresh_tokens',
        'groups',
        'group_members',
        'group_roles',
        'user_roles',
        'abac_policies',
        'api_keys',
        'sso_providers',
        'external_identities',
        'user_mfa_totp'
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

ALTER TABLE roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE roles FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON roles;
CREATE POLICY tenant_isolation ON roles
    USING (tenant_id = openfoundry_current_tenant() OR name IN ('admin', 'editor', 'viewer'))
    WITH CHECK (tenant_id = openfoundry_current_tenant() OR name IN ('admin', 'editor', 'viewer'));

ALTER TABLE permissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE permissions FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON permissions;
CREATE POLICY tenant_isolation ON permissions
    USING (
        tenant_id = openfoundry_current_tenant()
        OR tenant_id = '00000000-0000-0000-0000-000000000001'
    )
    WITH CHECK (
        tenant_id = openfoundry_current_tenant()
        OR tenant_id = '00000000-0000-0000-0000-000000000001'
    );

ALTER TABLE role_permissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE role_permissions FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON role_permissions;
CREATE POLICY tenant_isolation ON role_permissions
    USING (
        tenant_id = openfoundry_current_tenant()
        OR tenant_id = '00000000-0000-0000-0000-000000000001'
    )
    WITH CHECK (
        tenant_id = openfoundry_current_tenant()
        OR tenant_id = '00000000-0000-0000-0000-000000000001'
    );

CREATE OR REPLACE FUNCTION openfoundry_lookup_user_by_email(p_email text)
RETURNS TABLE (id uuid, tenant_id uuid)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
    SELECT u.id, u.tenant_id
    FROM users u
    WHERE u.email = p_email
$$;

CREATE OR REPLACE FUNCTION openfoundry_lookup_sso_provider_by_slug(p_slug text)
RETURNS TABLE (id uuid, tenant_id uuid)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
    SELECT p.id, p.tenant_id
    FROM sso_providers p
    WHERE p.slug = p_slug AND p.enabled = true
$$;

CREATE OR REPLACE FUNCTION openfoundry_lookup_sso_provider_by_id(p_id uuid)
RETURNS TABLE (id uuid, tenant_id uuid)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
    SELECT p.id, p.tenant_id
    FROM sso_providers p
    WHERE p.id = p_id
$$;

CREATE OR REPLACE FUNCTION openfoundry_list_public_sso_providers()
RETURNS TABLE (id uuid, slug text, name text, provider_type text)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
    SELECT p.id, p.slug, p.name, p.provider_type
    FROM sso_providers p
    WHERE p.enabled = true
    ORDER BY p.name
$$;

GRANT EXECUTE ON FUNCTION openfoundry_lookup_user_by_email(text) TO PUBLIC;
GRANT EXECUTE ON FUNCTION openfoundry_lookup_sso_provider_by_slug(text) TO PUBLIC;
GRANT EXECUTE ON FUNCTION openfoundry_lookup_sso_provider_by_id(uuid) TO PUBLIC;
GRANT EXECUTE ON FUNCTION openfoundry_list_public_sso_providers() TO PUBLIC;
