use sea_orm_migration::prelude::*;

mod m20241029_000001_init;
mod m20260403_000002_scope_network_config_unique;
mod m20260421_000003_add_network_config_source;
mod m20260514_000004_rename_web_config_source;
mod m20260619_000005_managed_config_revisions;
mod m20260820_000006_anf_invites_and_devices;
mod m20260820_000007_anf_networks_tags;
mod m20260821_000008_anf_device_virtual_ip;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20241029_000001_init::Migration),
            Box::new(m20260403_000002_scope_network_config_unique::Migration),
            Box::new(m20260421_000003_add_network_config_source::Migration),
            Box::new(m20260514_000004_rename_web_config_source::Migration),
            Box::new(m20260619_000005_managed_config_revisions::Migration),
            Box::new(m20260820_000006_anf_invites_and_devices::Migration),
            Box::new(m20260820_000007_anf_networks_tags::Migration),
            Box::new(m20260821_000008_anf_device_virtual_ip::Migration),
        ]
    }
}
