//! `SeaORM` Entity, hand-written to match the generated entity style.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "acl_rules")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub network_inst_id: String,
    pub name: String,
    pub enabled: bool,
    pub source_tags: String,
    pub destination_tags: String,
    pub protocol: String,
    pub ports: String,
    pub action: String,
    pub priority: i32,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
