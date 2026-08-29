use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260829_000009_anf_user_two_factor"
    }
}

/// users 表增加 TOTP 两步验证字段：
/// - totp_secret_encrypted：AES-256-GCM 加密后的 base32 secret（base64(nonce||ciphertext)）
/// - totp_enabled：完成绑定验证后才置 1
/// - totp_fail_count / totp_lock_until：账号级验证失败退避（累计失败次数 / 锁定截止 unix 秒）
/// - totp_last_step：最近一次成功验证的 TOTP 窗口，防同窗口重放
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE users ADD COLUMN totp_secret_encrypted TEXT;
                ALTER TABLE users ADD COLUMN totp_enabled BOOLEAN NOT NULL DEFAULT 0;
                ALTER TABLE users ADD COLUMN totp_fail_count INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE users ADD COLUMN totp_lock_until INTEGER;
                ALTER TABLE users ADD COLUMN totp_last_step INTEGER;
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
                ALTER TABLE users DROP COLUMN totp_last_step;
                ALTER TABLE users DROP COLUMN totp_lock_until;
                ALTER TABLE users DROP COLUMN totp_fail_count;
                ALTER TABLE users DROP COLUMN totp_enabled;
                ALTER TABLE users DROP COLUMN totp_secret_encrypted;
                "#,
            )
            .await?;
        Ok(())
    }
}
