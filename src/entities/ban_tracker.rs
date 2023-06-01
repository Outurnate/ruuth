/*
ruuth: simple auth_request backend
Copyright (C) 2022 Joe Dillon

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use sea_orm::{
  ActiveModelBehavior, DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait, RelationDef,
  RelationTrait,
};
use sea_orm_migration::sea_orm::DeriveEntityModel;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "ban_tracker")]
pub struct Model
{
  #[sea_orm(primary_key, auto_increment = true)]
  pub id: i64,
  pub host: String,
  pub failure_timestamp: i64,
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
