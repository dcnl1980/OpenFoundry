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

ALTER TABLE notifications ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE notifications SET tenant_id = COALESCE(user_id, '00000000-0000-0000-0000-000000000001') WHERE tenant_id IS NULL;
UPDATE notifications SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE notifications ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_notifications_tenant ON notifications (tenant_id);
DROP TRIGGER IF EXISTS trg_notifications_fill_tenant ON notifications;
CREATE TRIGGER trg_notifications_fill_tenant BEFORE INSERT ON notifications
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE notification_deliveries ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE notification_deliveries d SET tenant_id = n.tenant_id FROM notifications n WHERE d.notification_id = n.id AND d.tenant_id IS NULL;
UPDATE notification_deliveries SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE notification_deliveries ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_notification_deliveries_tenant ON notification_deliveries (tenant_id);
DROP TRIGGER IF EXISTS trg_notification_deliveries_fill_tenant ON notification_deliveries;
CREATE TRIGGER trg_notification_deliveries_fill_tenant BEFORE INSERT ON notification_deliveries
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

ALTER TABLE notification_preferences ADD COLUMN IF NOT EXISTS tenant_id UUID;
UPDATE notification_preferences SET tenant_id = user_id WHERE tenant_id IS NULL;
UPDATE notification_preferences SET tenant_id = '00000000-0000-0000-0000-000000000001' WHERE tenant_id IS NULL;
ALTER TABLE notification_preferences ALTER COLUMN tenant_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_notification_preferences_tenant ON notification_preferences (tenant_id);
DROP TRIGGER IF EXISTS trg_notification_preferences_fill_tenant ON notification_preferences;
CREATE TRIGGER trg_notification_preferences_fill_tenant BEFORE INSERT ON notification_preferences
FOR EACH ROW EXECUTE FUNCTION openfoundry_fill_tenant();

DO $$
DECLARE
    tbl text;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'notifications',
        'notification_deliveries',
        'notification_preferences'
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
