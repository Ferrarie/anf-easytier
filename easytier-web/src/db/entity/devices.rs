//! `SeaORM` Entity, hand-written to match the generated entity style.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "devices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub machine_id: String,
    pub display_name: String,
    pub status: String,
    pub approved_by: Option<i32>,
    pub approved_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ApprovedBy",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "SetNull"
    )]
    Users,
    #[sea_orm(has_many = "super::device_tags::Entity")]
    DeviceTags,
    #[sea_orm(has_many = "super::device_networks::Entity")]
    DeviceNetworks,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl Related<super::device_tags::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DeviceTags.def()
    }
}

impl Related<super::device_networks::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DeviceNetworks.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
