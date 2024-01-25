//! `SeaORM` Entity. Generated by sea-orm-codegen 0.12.12

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "sim_card")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub created_at: DateTimeWithTimeZone,
    pub phone_number: String,
    pub ssn: String,
    pub apn_address: String,
    pub apn_user: String,
    pub apn_password: String,
    pub pin: Option<String>,
    pub pin2: Option<String>,
    pub puk: Option<String>,
    pub puk2: Option<String>,
    pub organization_id: i32,
    pub tracker_id: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organization::Entity",
        from = "Column::OrganizationId",
        to = "super::organization::Column::Id",
        on_update = "Cascade",
        on_delete = "NoAction"
    )]
    Organization,
    #[sea_orm(
        belongs_to = "super::vehicle_tracker::Entity",
        from = "Column::TrackerId",
        to = "super::vehicle_tracker::Column::Id",
        on_update = "Cascade",
        on_delete = "SetNull"
    )]
    VehicleTracker,
}

impl Related<super::organization::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::vehicle_tracker::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::VehicleTracker.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
