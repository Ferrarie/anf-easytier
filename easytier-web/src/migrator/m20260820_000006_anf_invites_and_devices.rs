use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260820_000006_anf_invites_and_devices"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE TABLE invite_codes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                    code TEXT NOT NULL UNIQUE,
                    created_by INTEGER NOT NULL,
                    max_uses INTEGER NOT NULL DEFAULT 1,
                    used_count INTEGER NOT NULL DEFAULT 0,
                    expires_at TEXT NULL,
                    enabled BOOLEAN NOT NULL DEFAULT 1,
                    created_at TEXT NOT NULL,
                    CONSTRAINT fk_invite_codes_created_by_to_users_id
                        FOREIGN KEY (created_by) REFERENCES users(id)
                        ON DELETE CASCADE ON UPDATE CASCADE
                );

                CREATE TABLE devices (
                    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                    machine_id TEXT NOT NULL UNIQUE,
                    display_name TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    approved_by INTEGER NULL,
                    approved_at TEXT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    CONSTRAINT fk_devices_approved_by_to_users_id
                        FOREIGN KEY (approved_by) REFERENCES users(id)
                        ON DELETE SET NULL ON UPDATE CASCADE
                );

                CREATE TABLE device_tags (
                    device_id INTEGER NOT NULL,
                    tag TEXT NOT NULL,
                    PRIMARY KEY (device_id, tag),
                    CONSTRAINT fk_device_tags_device_id_to_devices_id
                        FOREIGN KEY (device_id) REFERENCES devices(id)
                        ON DELETE CASCADE ON UPDATE CASCADE
                );

                CREATE TABLE device_networks (
                    device_id INTEGER NOT NULL,
                    network_inst_id TEXT NOT NULL,
                    PRIMARY KEY (device_id, network_inst_id),
                    CONSTRAINT fk_device_networks_device_id_to_devices_id
                        FOREIGN KEY (device_id) REFERENCES devices(id)
                        ON DELETE CASCADE ON UPDATE CASCADE
                );

                CREATE TABLE admin_devices (
                    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                    machine_id TEXT NOT NULL UNIQUE,
                    user_id INTEGER NOT NULL,
                    created_at TEXT NOT NULL,
                    CONSTRAINT fk_admin_devices_user_id_to_users_id
                        FOREIGN KEY (user_id) REFERENCES users(id)
                        ON DELETE CASCADE ON UPDATE CASCADE
                );

                CREATE TABLE acl_rules (
                    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                    network_inst_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    enabled BOOLEAN NOT NULL DEFAULT 1,
                    source_tags TEXT NOT NULL,
                    destination_tags TEXT NOT NULL,
                    protocol TEXT NOT NULL DEFAULT 'any',
                    ports TEXT NOT NULL DEFAULT '[]',
                    action TEXT NOT NULL DEFAULT 'drop',
                    priority INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                INSERT OR IGNORE INTO groups (name) VALUES ('superusers');

                -- 确保内置 admin 账号属于 superusers 组，否则管理后台（AdminSession）
                -- 的 is_superuser 校验会 403。幂等：仅当 admin 尚未加入时插入。
                INSERT INTO users_groups (user_id, group_id)
                SELECT u.id, g.id
                FROM users u, groups g
                WHERE u.username = 'admin'
                  AND g.name = 'superusers'
                  AND NOT EXISTS (
                      SELECT 1 FROM users_groups ug
                      WHERE ug.user_id = u.id AND ug.group_id = g.id
                  );
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                DROP TABLE IF EXISTS acl_rules;
                DROP TABLE IF EXISTS admin_devices;
                DROP TABLE IF EXISTS device_networks;
                DROP TABLE IF EXISTS device_tags;
                DROP TABLE IF EXISTS devices;
                DROP TABLE IF EXISTS invite_codes;
                DELETE FROM groups WHERE name = 'superusers';
                "#,
            )
            .await?;
        Ok(())
    }
}
