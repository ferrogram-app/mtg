// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use libsql::params;

use super::SqliteSessionError;

/// Schema version stored in `PRAGMA user_version`.
pub(super) const VERSION: i64 = 1;

pub(super) struct Database(pub(super) libsql::Connection);

impl Database {
    pub(super) async fn init(&self) -> libsql::Result<()> {
        let mut user_version: i64 = self
            .fetch_one("PRAGMA user_version", params![], |row| row.get(0))
            .await?
            .unwrap_or(0);
        if user_version == VERSION {
            return Ok(());
        }

        if user_version == 0 {
            self.migrate_v0_to_v1().await?;
            user_version += 1;
        }
        if user_version == VERSION {
            // Can't bind PRAGMA parameters, but `VERSION` is not user-controlled input.
            self.0
                .execute(&format!("PRAGMA user_version = {VERSION}"), params![])
                .await?;
        }
        Ok(())
    }

    pub(super) async fn migrate_v0_to_v1(&self) -> libsql::Result<()> {
        let transaction = self.begin_transaction().await?;
        transaction
            .execute(
                "CREATE TABLE dc_home (
                dc_id INTEGER NOT NULL,
                PRIMARY KEY(dc_id))",
                params![],
            )
            .await?;
        transaction
            .execute(
                "CREATE TABLE dc_option (
                dc_id INTEGER NOT NULL,
                ipv4 TEXT NOT NULL,
                ipv6 TEXT NOT NULL,
                auth_key BLOB,
                PRIMARY KEY (dc_id))",
                params![],
            )
            .await?;
        transaction
            .execute(
                "CREATE TABLE peer_info (
                peer_id INTEGER NOT NULL,
                hash INTEGER,
                subtype INTEGER,
                PRIMARY KEY (peer_id))",
                params![],
            )
            .await?;
        transaction
            .execute(
                "CREATE TABLE update_state (
                pts INTEGER NOT NULL,
                qts INTEGER NOT NULL,
                date INTEGER NOT NULL,
                seq INTEGER NOT NULL)",
                params![],
            )
            .await?;
        transaction
            .execute(
                "CREATE TABLE channel_state (
                peer_id INTEGER NOT NULL,
                pts INTEGER NOT NULL,
                PRIMARY KEY (peer_id))",
                params![],
            )
            .await?;

        transaction.commit().await?;
        Ok(())
    }

    pub(super) async fn begin_transaction(&self) -> libsql::Result<libsql::Transaction> {
        self.0.transaction().await
    }

    pub(super) async fn fetch_one<
        T,
        P: libsql::params::IntoParams,
        F: FnOnce(libsql::Row) -> libsql::Result<T>,
    >(
        &self,
        statement: &str,
        params: P,
        select: F,
    ) -> libsql::Result<Option<T>> {
        let mut statement = self.0.prepare(statement).await?;
        let result = statement.query_row(params).await;
        match result {
            Ok(value) => Ok(Some(select(value)?)),
            Err(libsql::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(super) async fn fetch_all<
        T,
        P: libsql::params::IntoParams,
        F: FnMut(libsql::Row) -> Result<T, SqliteSessionError>,
    >(
        &self,
        statement: &str,
        params: P,
        mut select: F,
    ) -> Result<Vec<T>, SqliteSessionError> {
        let statement = self.0.prepare(statement).await?;
        let mut rows = statement.query(params).await?;
        let mut result = Vec::new();
        while let Some(row) = rows.next().await? {
            result.push(select(row)?);
        }
        Ok(result)
    }
}
