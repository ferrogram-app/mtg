// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Mutex;

use libsql::{named_params, params};
use tokio::sync::Mutex as AsyncMutex;

use crate::types::{
    ChannelKind, ChannelState, DcOption, PeerAuth, PeerId, PeerInfo, PeerKind, UpdateState,
    UpdatesState,
};
use crate::{BoxFuture, DEFAULT_DC, KNOWN_DC_OPTIONS, Session};

mod db;
mod error;

use db::Database;
pub use error::SqliteSessionError;

struct Cache {
    pub home_dc: i32,
    pub dc_options: HashMap<i32, DcOption>,
    pub dc_addresses: HashMap<i32, Vec<SocketAddr>>,
}

/// SQLite-based storage. This is the recommended option.
pub struct SqliteSession {
    database: AsyncMutex<Database>,
    cache: Mutex<Cache>,
}

#[repr(u8)]
enum PeerSubtype {
    UserSelf = 1,
    UserBot = 2,
    UserSelfBot = 3,
    Megagroup = 4,
    Broadcast = 8,
    Gigagroup = 12,
    Community = 16,
}

impl SqliteSession {
    /// Open a connection to the SQLite database at `path`,
    /// creating one if it doesn't exist.
    pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self, SqliteSessionError> {
        let conn = libsql::Builder::new_local(path).build().await?.connect()?;
        let db = Database(conn);
        db.init().await?;

        let home_dc = db
            .fetch_one("SELECT * FROM dc_home LIMIT 1", named_params![], |row| {
                row.get::<i32>(0)
            })
            .await?
            .unwrap_or(DEFAULT_DC);

        let dc_options = db
            .fetch_all("SELECT * FROM dc_option", named_params![], |row| {
                Ok(DcOption {
                    id: row.get::<i32>(0)?,
                    ipv4: row.get::<String>(1)?.parse()?,
                    ipv6: row.get::<String>(2)?.parse()?,
                    auth_key: match row.get::<Option<Vec<u8>>>(3)? {
                        None => None,
                        Some(auth_key) => Some(auth_key.try_into().map_err(|v: Vec<u8>| {
                            SqliteSessionError::InvalidAuthKeyLength(v.len())
                        })?),
                    },
                })
            })
            .await?
            .into_iter()
            .map(|dc_option| (dc_option.id, dc_option))
            .collect();

        let mut dc_addresses: HashMap<i32, Vec<SocketAddr>> = HashMap::new();
        for (dc_id, address) in db
            .fetch_all(
                "SELECT dc_id, address FROM dc_address ORDER BY dc_id, position",
                named_params![],
                |row| Ok((row.get::<i32>(0)?, row.get::<String>(1)?.parse()?)),
            )
            .await?
        {
            dc_addresses.entry(dc_id).or_default().push(address);
        }

        Ok(SqliteSession {
            database: AsyncMutex::new(db),
            cache: Mutex::new(Cache {
                home_dc,
                dc_options,
                dc_addresses,
            }),
        })
    }
}

impl Session for SqliteSession {
    type Error = SqliteSessionError;

    fn home_dc_id(&self) -> Result<i32, SqliteSessionError> {
        Ok(self
            .cache
            .lock()
            .map_err(|_| SqliteSessionError::Poisoned)?
            .home_dc)
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), SqliteSessionError>> {
        let ok = match self.cache.lock() {
            Err(_) => Err(SqliteSessionError::Poisoned),
            Ok(mut x) => {
                x.home_dc = dc_id;
                Ok(())
            }
        };
        Box::pin(async move {
            ok?;

            let transaction = self.database.lock().await.begin_transaction().await?;
            transaction
                .execute("DELETE FROM dc_home", params![])
                .await?;
            let stmt = transaction
                .prepare("INSERT INTO dc_home VALUES (:dc_id)")
                .await?;
            stmt.execute(named_params! {":dc_id": dc_id}).await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, SqliteSessionError> {
        Ok(self
            .cache
            .lock()
            .map_err(|_| SqliteSessionError::Poisoned)?
            .dc_options
            .get(&dc_id)
            .cloned()
            .or_else(|| {
                KNOWN_DC_OPTIONS
                    .iter()
                    .find(|dc_option| dc_option.id == dc_id)
                    .cloned()
            }))
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), SqliteSessionError>> {
        let ok = match self.cache.lock() {
            Err(_) => Err(SqliteSessionError::Poisoned),
            Ok(mut x) => {
                x.dc_options.insert(dc_option.id, dc_option.clone());
                Ok(())
            }
        };

        let dc_option = dc_option.clone();
        Box::pin(async move {
            ok?;

            let db = self.database.lock().await;
            db.0.execute(
                "INSERT OR REPLACE INTO dc_option VALUES (:dc_id, :ipv4, :ipv6, :auth_key)",
                named_params! {
                    ":dc_id": dc_option.id,
                    ":ipv4": dc_option.ipv4.to_string(),
                    ":ipv6": dc_option.ipv6.to_string(),
                    ":auth_key": dc_option.auth_key.map(|k| k.to_vec()),
                },
            )
            .await?;
            Ok(())
        })
    }

    fn dc_addresses(&self, dc_id: i32) -> Result<Vec<SocketAddr>, SqliteSessionError> {
        Ok(self
            .cache
            .lock()
            .map_err(|_| SqliteSessionError::Poisoned)?
            .dc_addresses
            .get(&dc_id)
            .cloned()
            .unwrap_or_default())
    }

    fn set_dc_addresses(
        &self,
        dc_id: i32,
        addresses: Vec<SocketAddr>,
    ) -> BoxFuture<'_, Result<(), SqliteSessionError>> {
        let ok = match self.cache.lock() {
            Err(_) => Err(SqliteSessionError::Poisoned),
            Ok(mut x) => {
                x.dc_addresses.insert(dc_id, addresses.clone());
                Ok(())
            }
        };

        Box::pin(async move {
            ok?;

            let db = self.database.lock().await;
            let transaction = db.begin_transaction().await?;
            transaction
                .execute(
                    "DELETE FROM dc_address WHERE dc_id = :dc_id",
                    named_params! {":dc_id": dc_id},
                )
                .await?;
            for (position, address) in addresses.iter().enumerate() {
                transaction
                    .execute(
                        "INSERT INTO dc_address VALUES (:dc_id, :position, :address)",
                        named_params! {
                            ":dc_id": dc_id,
                            ":position": position as i64,
                            ":address": address.to_string(),
                        },
                    )
                    .await?;
            }
            transaction.commit().await?;
            Ok(())
        })
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, SqliteSessionError>> {
        Box::pin(async move {
            let db = self.database.lock().await;
            let map_row = |row: libsql::Row| {
                let subtype = row.get::<Option<i64>>(2)?.map(|s| s as u8);
                Ok(match peer.kind() {
                    PeerKind::User => PeerInfo::User {
                        id: PeerId::user_unchecked(row.get::<i64>(0)?).bare_id_unchecked(),
                        auth: row.get::<Option<i64>>(1)?.map(PeerAuth::from_hash),
                        bot: subtype.map(|s| s & PeerSubtype::UserBot as u8 != 0),
                        is_self: subtype.map(|s| s & PeerSubtype::UserSelf as u8 != 0),
                    },
                    PeerKind::Chat => PeerInfo::Chat {
                        id: peer.bare_id_unchecked(),
                    },
                    PeerKind::Channel => PeerInfo::Channel {
                        id: peer.bare_id_unchecked(),
                        auth: row.get::<Option<i64>>(1)?.map(PeerAuth::from_hash),
                        kind: subtype.and_then(|s| {
                            if (s & PeerSubtype::Gigagroup as u8) == PeerSubtype::Gigagroup as u8 {
                                Some(ChannelKind::Gigagroup)
                            } else if s & PeerSubtype::Broadcast as u8 != 0 {
                                Some(ChannelKind::Broadcast)
                            } else if s & PeerSubtype::Megagroup as u8 != 0 {
                                Some(ChannelKind::Megagroup)
                            } else {
                                None
                            }
                        }),
                    },
                })
            };

            Ok(if let Some(peer_id) = peer.bot_api_dialog_id() {
                db.fetch_one(
                    "SELECT * FROM peer_info WHERE peer_id = :peer_id LIMIT 1",
                    named_params! {":peer_id": peer_id},
                    map_row,
                )
                .await?
            } else {
                db.fetch_one(
                    "SELECT * FROM peer_info WHERE subtype & :type LIMIT 1",
                    named_params! {":type": PeerSubtype::UserSelf as i64},
                    map_row,
                )
                .await?
            })
        })
    }

    fn cache_peer(&self, peer: PeerInfo) -> BoxFuture<'_, Result<(), SqliteSessionError>> {
        self.cache_peers(vec![peer])
    }

    fn cache_peers(&self, peers: Vec<PeerInfo>) -> BoxFuture<'_, Result<(), SqliteSessionError>> {
        Box::pin(async move {
            let mut result = Ok(());
            let mut extended_peers = Vec::with_capacity(peers.len());
            for peer in peers {
                extended_peers.push(match self.peer(peer.id()).await {
                    Ok(Some(mut existing_peer)) => {
                        existing_peer.extend_info(&peer);
                        existing_peer
                    }
                    Ok(None) => peer,
                    Err(e) => {
                        result = Err(e);
                        peer
                    }
                });
            }

            let db = self.database.lock().await;
            let tx = db.0.transaction().await?;
            let stmt = tx
                .prepare("INSERT OR REPLACE INTO peer_info VALUES (:peer_id, :hash, :subtype)")
                .await?;
            for peer in extended_peers {
                let subtype = match peer {
                    PeerInfo::User { bot, is_self, .. } => {
                        match (bot.unwrap_or_default(), is_self.unwrap_or_default()) {
                            (true, true) => Some(PeerSubtype::UserSelfBot),
                            (true, false) => Some(PeerSubtype::UserBot),
                            (false, true) => Some(PeerSubtype::UserSelf),
                            (false, false) => None,
                        }
                    }
                    PeerInfo::Chat { .. } => None,
                    PeerInfo::Channel { kind, .. } => kind.map(|kind| match kind {
                        ChannelKind::Megagroup => PeerSubtype::Megagroup,
                        ChannelKind::Broadcast => PeerSubtype::Broadcast,
                        ChannelKind::Gigagroup => PeerSubtype::Gigagroup,
                        ChannelKind::Community => PeerSubtype::Community,
                    }),
                };
                let mut params = vec![];
                let peer_id = peer.id().bot_api_dialog_id_unchecked();
                params.push((":peer_id".to_owned(), Some(peer_id)));
                params.push((":hash".to_owned(), peer.auth().map(|a| a.hash())));
                params.push((":subtype".to_owned(), subtype.map(|s| s as i64)));
                if let Err(err) = stmt.execute(params).await {
                    result = Err(err.into());
                }
                stmt.reset();
            }
            tx.commit().await?;
            result
        })
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, SqliteSessionError>> {
        Box::pin(async move {
            let db = self.database.lock().await;
            let mut state = db
                .fetch_one(
                    "SELECT * FROM update_state LIMIT 1",
                    named_params![],
                    |row| {
                        Ok(UpdatesState {
                            pts: row.get(0)?,
                            qts: row.get(1)?,
                            date: row.get(2)?,
                            seq: row.get(3)?,
                            channels: Vec::new(),
                        })
                    },
                )
                .await?
                .unwrap_or_default();
            state.channels = db
                .fetch_all("SELECT * FROM channel_state", named_params![], |row| {
                    Ok(ChannelState {
                        id: row.get(0)?,
                        pts: row.get(1)?,
                    })
                })
                .await?;
            Ok(state)
        })
    }

    fn set_update_state(
        &self,
        update: UpdateState,
    ) -> BoxFuture<'_, Result<(), SqliteSessionError>> {
        Box::pin(async move {
            let db = self.database.lock().await;
            let transaction = db.begin_transaction().await?;

            match update {
                UpdateState::All(updates_state) => {
                    transaction
                        .execute("DELETE FROM update_state", params![])
                        .await?;
                    transaction
                        .execute(
                            "INSERT INTO update_state VALUES (:pts, :qts, :date, :seq)",
                            named_params! {
                                ":pts": updates_state.pts,
                                ":qts": updates_state.qts,
                                ":date": updates_state.date,
                                ":seq": updates_state.seq,
                            },
                        )
                        .await?;

                    transaction
                        .execute("DELETE FROM channel_state", params![])
                        .await?;
                    for channel in updates_state.channels {
                        transaction
                            .execute(
                                "INSERT INTO channel_state VALUES (:peer_id, :pts)",
                                named_params! {
                                    ":peer_id": channel.id,
                                    ":pts": channel.pts,
                                },
                            )
                            .await?;
                    }
                }
                UpdateState::Primary { pts, date, seq } => {
                    let previous = db
                        .fetch_one(
                            "SELECT * FROM update_state LIMIT 1",
                            named_params![],
                            |_| Ok(()),
                        )
                        .await?;

                    if previous.is_some() {
                        transaction
                            .execute(
                                "UPDATE update_state SET pts = :pts, date = :date, seq = :seq",
                                named_params! {
                                    ":pts": pts,
                                    ":date": date,
                                    ":seq": seq,
                                },
                            )
                            .await?;
                    } else {
                        transaction
                            .execute(
                                "INSERT INTO update_state VALUES (:pts, 0, :date, :seq)",
                                named_params! {
                                    ":pts": pts,
                                    ":date": date,
                                    ":seq": seq,
                                },
                            )
                            .await?;
                    }
                }
                UpdateState::Secondary { qts } => {
                    let previous = db
                        .fetch_one(
                            "SELECT * FROM update_state LIMIT 1",
                            named_params![],
                            |_| Ok(()),
                        )
                        .await?;

                    if previous.is_some() {
                        transaction
                            .execute(
                                "UPDATE update_state SET qts = :qts",
                                named_params! {":qts": qts},
                            )
                            .await?;
                    } else {
                        transaction
                            .execute(
                                "INSERT INTO update_state VALUES (0, :qts, 0, 0)",
                                named_params! {":qts": qts},
                            )
                            .await?;
                    }
                }
                UpdateState::Channel { id, pts } => {
                    transaction
                        .execute(
                            "INSERT OR REPLACE INTO channel_state VALUES (:peer_id, :pts)",
                            named_params! {
                                ":peer_id": id,
                                ":pts": pts,
                            },
                        )
                        .await?;
                }
            }

            transaction.commit().await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
