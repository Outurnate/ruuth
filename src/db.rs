use sea_orm::{DbConn, EntityTrait, Schema, ConnectionTrait, ExecResult};
use crate::{entities::prelude::*};
use sea_orm::{DbBackend, SqlxMySqlConnector, SqlxPostgresConnector, SqlxSqliteConnector, DatabaseConnection};
use sqlx::{Pool, Sqlite, MySql, Postgres};
use color_eyre::eyre::{Result, Context, eyre};

async fn create_table<E: EntityTrait>(db: &DbConn, entity: E) -> Result<ExecResult, sea_orm::DbErr>
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
    Pool::<MySql>::connect(url).await
      .map(|pool| (DatabaseBackend::MySql(pool.clone()), SqlxMySqlConnector::from_sqlx_mysql_pool(pool)))
      .wrap_err("error connecting to mysql database")
  }
  else if DbBackend::Postgres.is_prefix_of(url)
  {
    Pool::<Postgres>::connect(url).await
      .map(|pool| (DatabaseBackend::Postgres(pool.clone()), SqlxPostgresConnector::from_sqlx_postgres_pool(pool)))
      .wrap_err("error connecting to postgres database")
  }
  else if DbBackend::Sqlite.is_prefix_of(url)
  {
    Pool::<Sqlite>::connect(url).await
      .map(|pool| (DatabaseBackend::Sqlite(pool.clone()), SqlxSqliteConnector::from_sqlx_sqlite_pool(pool)))
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
  Sqlite(Pool<Sqlite>)
}