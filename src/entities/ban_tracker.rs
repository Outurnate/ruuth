use sea_orm::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "ban_tracker")]
pub struct Model
{
  #[sea_orm(primary_key, auto_increment = true)]
  pub id: i64,
  pub host: String,
  pub failure_timestamp: i64
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl RelationTrait for Relation
{
  fn def(&self) -> RelationDef
  {
    panic!("No RelationDef")
  }
}

impl ActiveModelBehavior for ActiveModel {}