use sea_orm::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model
{
  #[sea_orm(primary_key, auto_increment = false)]
  pub username: String,
  pub password_hash: String,
  pub totp_secret: Vec<u8>,
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
