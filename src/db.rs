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

use crate::entities::prelude::*;
use color_eyre::eyre::{eyre, Context, Result};
use sea_orm::{
  ConnectionTrait, DatabaseConnection, DbBackend, DbConn, EntityTrait, ExecResult, Schema,
  SqlxMySqlConnector, SqlxPostgresConnector, SqlxSqliteConnector,
};
use sqlx::{MySql, Pool, Postgres, Sqlite};

async fn create_table<E: EntityTrait>(db: &DbConn, entity: E)
  -> Result<ExecResult, sea_orm::DbErr>
{
  let builder = db.get_database_backend();
  let schema = Schema::new(builder);
  let stmt = builder.build(schema.create_table_from_entity(entity).if_not_exists());

  db.execute(stmt).await
}

async fn create_tables(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr>
{
  create_table(db, User).await?;
  create_table(db, BanTracker).await?;
  Ok(())
}

pub async fn connect(url: &str) -> Result<(DatabaseBackend, DatabaseConnection)>
{
  let (backend, connection) = if DbBackend::MySql.is_prefix_of(url)
  {
    Pool::<MySql>::connect(url)
      .await
      .map(|pool| {
        (
          DatabaseBackend::MySql(pool.clone()),
          SqlxMySqlConnector::from_sqlx_mysql_pool(pool),
        )
      })
      .wrap_err("error connecting to mysql database")
  }
  else if DbBackend::Postgres.is_prefix_of(url)
  {
    Pool::<Postgres>::connect(url)
      .await
      .map(|pool| {
        (
          DatabaseBackend::Postgres(pool.clone()),
          SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
        )
      })
      .wrap_err("error connecting to postgres database")
  }
  else if DbBackend::Sqlite.is_prefix_of(url)
  {
    Pool::<Sqlite>::connect(url)
      .await
      .map(|pool| {
        (
          DatabaseBackend::Sqlite(pool.clone()),
          SqlxSqliteConnector::from_sqlx_sqlite_pool(pool),
        )
      })
      .wrap_err("error connecting to sqlite database")
  }
  else
  {
    Err(eyre!("connection string does not match any driver"))
  }?;
  create_tables(&connection).await?;
  Ok((backend, connection))
}

pub enum DatabaseBackend
{
  MySql(Pool<MySql>),
  Postgres(Pool<Postgres>),
  Sqlite(Pool<Sqlite>),
}
