CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE OR REPLACE FUNCTION openfoundry_clone_system_ai_catalog() RETURNS void
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

    IF NOT EXISTS (SELECT 1 FROM ai_providers WHERE tenant_id = dest) THEN
        INSERT INTO ai_providers (
            id,
            name,
            provider_type,
            model_name,
            endpoint_url,
            api_mode,
            credential_reference,
            enabled,
            load_balance_weight,
            max_output_tokens,
            cost_tier,
            tags,
            route_rules,
            health_state,
            tenant_id
        )
        SELECT
            gen_random_uuid(),
            name,
            provider_type,
            model_name,
            endpoint_url,
            api_mode,
            credential_reference,
            enabled,
            load_balance_weight,
            max_output_tokens,
            cost_tier,
            tags,
            route_rules,
            health_state,
            dest
        FROM ai_providers
        WHERE tenant_id = src;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM ai_tools WHERE tenant_id = dest) THEN
        INSERT INTO ai_tools (
            id,
            name,
            description,
            category,
            execution_mode,
            status,
            input_schema,
            output_schema,
            tags,
            tenant_id
        )
        SELECT
            gen_random_uuid(),
            name,
            description,
            category,
            execution_mode,
            status,
            input_schema,
            output_schema,
            tags,
            dest
        FROM ai_tools
        WHERE tenant_id = src;
    END IF;
END
$$;

GRANT EXECUTE ON FUNCTION openfoundry_clone_system_ai_catalog() TO PUBLIC;
