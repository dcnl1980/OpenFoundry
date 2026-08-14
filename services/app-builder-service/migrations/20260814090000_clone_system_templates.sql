CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE OR REPLACE FUNCTION openfoundry_clone_system_app_templates() RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
    dest uuid := openfoundry_current_tenant();
    src uuid := '00000000-0000-0000-0000-000000000001';
BEGIN
    IF dest IS NULL OR dest = src THEN
        RETURN;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM app_templates WHERE tenant_id = dest) THEN
        INSERT INTO app_templates (
            id,
            key,
            name,
            description,
            category,
            preview_image_url,
            definition,
            tenant_id
        )
        SELECT
            gen_random_uuid(),
            key,
            name,
            description,
            category,
            preview_image_url,
            definition,
            dest
        FROM app_templates
        WHERE tenant_id = src;
    END IF;
END
$$;

GRANT EXECUTE ON FUNCTION openfoundry_clone_system_app_templates() TO PUBLIC;
