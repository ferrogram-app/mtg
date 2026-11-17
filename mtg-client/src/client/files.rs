// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::mem;
use std::sync::Arc;
#[cfg(feature = "fs")]
use std::{io::SeekFrom, path::Path};

use futures_util::stream::{FuturesUnordered, StreamExt as _};
use mtg_mtsender::InvocationError;
use mtg_tl_types as tl;
use tokio::io::{self, AsyncRead, AsyncReadExt};
use tokio::sync::Mutex as AsyncMutex;
#[cfg(feature = "fs")]
use tokio::{
    fs,
    io::{AsyncSeekExt, AsyncWriteExt},
    sync::mpsc::unbounded_channel,
};

use super::Client;
use crate::media::{Downloadable, Uploaded};
use crate::utils::generate_random_id;
use mtg_session::types::PeerRef;

pub const MIN_CHUNK_SIZE: i32 = 4 * 1024;
pub const MAX_CHUNK_SIZE: i32 = 512 * 1024;
const FILE_MIGRATE_ERROR: i32 = 303;
const BIG_FILE_SIZE: usize = 10 * 1024 * 1024;
const WORKER_COUNT: usize = 4;

/// Iterator returned by [`Client::iter_download`].
pub struct DownloadIter {
    client: Client,
    done: bool,
    size: Option<usize>,
    variant: DownloadIterVariant,
    dc_id: Option<i32>,
    /// Message the media came from, to refresh an expired file reference.
    source: Option<(PeerRef, i32)>,
}

enum DownloadIterVariant {
    Request(tl::functions::upload::GetFile),
    PreDownloaded(Vec<u8>),
    PreFailed(io::Error),
    Empty,
}

impl DownloadIter {
    /// Changes the chunk size, in bytes, used to make requests. Useful if you only need to get a
    /// small part of a file. By default, `MAX_CHUNK_SIZE` is used.
    ///
    /// # Panics
    ///
    /// Panics if `size` is not divisible by `MIN_CHUNK_SIZE`, or if `size` is not in contained in
    /// the range `MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE`.
    pub fn chunk_size(mut self, size: i32) -> Self {
        assert!((MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE).contains(&size) && size % MIN_CHUNK_SIZE == 0);
        if let DownloadIterVariant::Request(request) = &mut self.variant {
            request.limit = size;
        }
        self
    }

    /// Skips `n` chunks to start downloading a different offset from the file. If you want to
    /// skip less data, modify the `chunk_size` before calling this method, and then reset it to
    /// any value you want.
    pub fn skip_chunks(mut self, n: i32) -> Self {
        if let DownloadIterVariant::Request(request) = &mut self.variant {
            request.offset += request.limit as i64 * (n as i64);
        }
        self
    }

    /// Tells where the media came from: when Telegram answers `FILE_REFERENCE_*`, the message is
    /// fetched again and the download continues with its fresh file reference (once per chunk).
    pub fn refresh_from<C: Into<PeerRef>>(mut self, peer: C, message_id: i32) -> Self {
        self.source = Some((peer.into(), message_id));
        self
    }

    /// Fetch and return the next chunk.
    pub async fn next(&mut self) -> Result<Option<Vec<u8>>, InvocationError> {
        if self.done {
            return Ok(None);
        }

        let variant = mem::replace(&mut self.variant, DownloadIterVariant::Empty);

        let mut request = match variant {
            DownloadIterVariant::Request(r) => r,
            DownloadIterVariant::PreDownloaded(data) => {
                self.done = true;
                return Ok(Some(data.clone()));
            }
            DownloadIterVariant::PreFailed(error) => {
                return Err(InvocationError::Io(error));
            }
            DownloadIterVariant::Empty => return Ok(None),
        };

        let mut refreshed = false;
        let mut dc = self
            .dc_id
            .map_or_else(|| self.client.0.session.home_dc_id(), Ok)?;

        loop {
            break match self.client.get_file_part(dc, &request).await {
                Ok(bytes) => {
                    let reached_known_size = self
                        .size
                        .is_some_and(|size| request.offset as usize + bytes.len() >= size);

                    if reached_known_size || bytes.len() < request.limit as usize {
                        self.done = true;
                        if bytes.is_empty() {
                            break Ok(None);
                        }
                    }

                    request.offset += request.limit as i64;
                    self.variant = DownloadIterVariant::Request(request);

                    Ok(Some(bytes))
                }
                Err(InvocationError::Rpc(err)) if &err.name == "AUTH_KEY_UNREGISTERED" => {
                    match self.client.copy_auth_to_dc(dc).await {
                        Ok(_) => continue,
                        Err(e) => Err(e),
                    }
                }
                Err(InvocationError::Rpc(err))
                    if err.code == FILE_MIGRATE_ERROR && err.value.is_some() =>
                {
                    dc = err.value.unwrap_or_default() as _;
                    continue;
                }
                Err(InvocationError::Rpc(err))
                    if err.name.starts_with("FILE_REFERENCE_") && !refreshed =>
                {
                    refreshed = true;
                    match self.refresh_reference(&mut request).await {
                        Ok(true) => continue,
                        Ok(false) => {
                            self.variant = DownloadIterVariant::Request(request);
                            Err(InvocationError::Rpc(err))
                        }
                        Err(e) => {
                            self.variant = DownloadIterVariant::Request(request);
                            Err(e)
                        }
                    }
                }
                // the offset is at (or past) the end: the file ended on a chunk boundary
                Err(InvocationError::Rpc(err))
                    if err.name == "OFFSET_INVALID" && request.offset > 0 =>
                {
                    self.done = true;
                    Ok(None)
                }
                Err(e) => {
                    // keep the request so that asking again retries this chunk
                    self.variant = DownloadIterVariant::Request(request);
                    Err(e)
                }
            };
        }
    }
}

/// Method implementations related to uploading or downloading files.
fn file_reference(location: &mut tl::enums::InputFileLocation) -> Option<&mut Vec<u8>> {
    match location {
        tl::enums::InputFileLocation::InputDocumentFileLocation(l) => Some(&mut l.file_reference),
        tl::enums::InputFileLocation::InputPhotoFileLocation(l) => Some(&mut l.file_reference),
        _ => None,
    }
}

impl DownloadIter {
    /// Re-fetches the source message and copies the fresh file reference into `request`.
    /// `Ok(false)`: nothing to refresh from (no source, message or media gone).
    async fn refresh_reference(
        &self,
        request: &mut tl::functions::upload::GetFile,
    ) -> Result<bool, InvocationError> {
        let Some((peer, id)) = self.source else {
            return Ok(false);
        };
        let message = self
            .client
            .get_messages_by_id(peer, &[id])
            .await?
            .pop()
            .flatten();
        let fresh = message
            .and_then(|m| m.media())
            .and_then(|m| m.to_raw_input_location())
            .and_then(|mut l| file_reference(&mut l).map(mem::take));
        match (fresh, file_reference(&mut request.location)) {
            (Some(new), Some(old)) => {
                *old = new;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl Client {
    /// Returns a new iterator over the contents of a media document that will be downloaded.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(downloadable: mtg_client::media::Media, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut file_bytes = Vec::new();
    /// let mut download = client.iter_download(&downloadable);
    ///
    /// while let Some(chunk) = download.next().await? {
    ///     file_bytes.extend(chunk);
    /// }
    ///
    /// // The file is now downloaded in-memory, inside `file_bytes`!
    /// # Ok(())
    /// # }
    /// ```
    pub fn iter_download<D: Downloadable>(&self, downloadable: &D) -> DownloadIter {
        if let Some(data) = downloadable.to_data() {
            DownloadIter {
                client: self.clone(),
                done: false,
                size: Some(data.len()),
                variant: DownloadIterVariant::PreDownloaded(data),
                dc_id: downloadable.dc_id(),
                source: None,
            }
        } else if let Some(location) = downloadable.to_raw_input_location() {
            DownloadIter {
                client: self.clone(),
                done: false,
                size: downloadable.size(),
                variant: DownloadIterVariant::Request(tl::functions::upload::GetFile {
                    precise: false,
                    cdn_supported: false,
                    location,
                    offset: 0,
                    limit: MAX_CHUNK_SIZE,
                }),
                dc_id: downloadable.dc_id(),
                source: None,
            }
        } else {
            DownloadIter {
                client: self.clone(),
                done: false,
                size: None,
                variant: DownloadIterVariant::PreFailed(io::Error::other("media not downloadable")),
                dc_id: downloadable.dc_id(),
                source: None,
            }
        }
    }

    /// Downloads a media file into the specified path.
    ///
    /// If the file already exists, it will be overwritten.
    ///
    /// This is a small wrapper around [`Client::iter_download`] for the common case of
    /// wanting to save the file locally.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(downloadable: mtg_client::media::Media, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// client.download_media(&downloadable, "/home/username/photos/holidays.jpg").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "fs")]
    pub async fn download_media<D: Downloadable, P: AsRef<Path>>(
        &self,
        downloadable: &D,
        path: P,
    ) -> Result<(), InvocationError> {
        // Concurrent downloader
        if let Some((location, size)) = downloadable
            .to_raw_input_location()
            .zip(downloadable.size())
        {
            let dc_id = downloadable
                .dc_id()
                .map_or_else(|| self.0.session.home_dc_id(), Ok)?;
            if size > BIG_FILE_SIZE {
                return self
                    .download_media_concurrent(location, size, path, WORKER_COUNT, dc_id)
                    .await;
            }
        }

        let mut download = self.iter_download(downloadable);
        Ok(Client::load(path, &mut download).await?)
    }

    #[cfg(feature = "fs")]
    async fn load<P: AsRef<Path>>(path: P, download: &mut DownloadIter) -> Result<(), io::Error> {
        let mut file = fs::File::create(path).await?;
        while let Some(chunk) = download.next().await.map_err(io::Error::other)? {
            file.write_all(&chunk).await?;
        }
        // tokio writes in the background: without a flush the last chunk can be lost on drop
        file.flush().await?;

        Ok(())
    }

    /// Downloads a `Document` to specified path using multiple connections
    #[cfg(feature = "fs")]
    async fn download_media_concurrent<P: AsRef<Path>>(
        &self,
        location: tl::enums::InputFileLocation,
        size: usize,
        path: P,
        workers: usize,
        dc_id: i32,
    ) -> Result<(), InvocationError> {
        // Allocate
        let mut file = fs::File::create(path).await.map_err(InvocationError::Io)?;
        file.set_len(size as u64)
            .await
            .map_err(InvocationError::Io)?;
        file.seek(SeekFrom::Start(0))
            .await
            .map_err(InvocationError::Io)?;

        // Start workers
        let (tx, mut rx) = unbounded_channel();
        let part_index = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let mut tasks = vec![];
        for _ in 0..workers {
            let location = location.clone();
            let tx = tx.clone();
            let part_index = part_index.clone();
            let client = self.clone();
            let task = tokio::task::spawn(async move {
                let mut retry_offset = None;
                let mut task_dc = dc_id;
                loop {
                    // Calculate file offset
                    let offset: i64 = {
                        if let Some(offset) = retry_offset {
                            retry_offset = None;
                            offset
                        } else {
                            let i = part_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            MAX_CHUNK_SIZE as i64 * i
                        }
                    };
                    if offset >= size as i64 {
                        break;
                    }
                    // Fetch from telegram
                    let request = &tl::functions::upload::GetFile {
                        precise: true,
                        cdn_supported: false,
                        location: location.clone(),
                        offset,
                        limit: MAX_CHUNK_SIZE,
                    };
                    match client.get_file_part(task_dc, request).await {
                        Ok(bytes) => {
                            if tx.send((offset as u64, bytes)).is_err() {
                                return Err(InvocationError::Dropped("download receiver closed"));
                            }
                        }
                        Err(InvocationError::Rpc(err)) if &err.name == "AUTH_KEY_UNREGISTERED" => {
                            match client.copy_auth_to_dc(task_dc).await {
                                Ok(_) => {
                                    retry_offset = Some(offset);
                                    continue;
                                }
                                Err(e) => return Err(e),
                            }
                        }
                        Err(InvocationError::Rpc(err)) => {
                            if let (FILE_MIGRATE_ERROR, Some(dc)) = (err.code, err.value) {
                                task_dc = dc as _;
                                retry_offset = Some(offset);
                                continue;
                            }
                            return Err(InvocationError::Rpc(err));
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok::<(), InvocationError>(())
            });
            tasks.push(task);
        }
        drop(tx);

        // File write loop
        let mut pos = 0;
        while let Some((offset, data)) = rx.recv().await {
            if offset != pos {
                file.seek(SeekFrom::Start(offset)).await?;
            }
            file.write_all(&data).await?;
            pos = offset + data.len() as u64;
        }
        file.flush().await?;

        // Check if all tasks finished succesfully
        for task in tasks {
            let res = task.await.map_err(io::Error::other)?;
            res?;
        }
        Ok(())
    }
}

mod upload;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    use mtg_mtsender::SenderPool;
    use mtg_testkit::FakeServer;

    use super::*;

    struct Thing {
        size: Option<usize>,
        data: Option<Vec<u8>>,
        located: bool,
    }

    impl Downloadable for Thing {
        fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
            self.located.then(|| {
                tl::types::InputPeerPhotoFileLocation {
                    big: false,
                    peer: tl::types::InputPeerSelf {}.into(),
                    photo_id: 1,
                }
                .into()
            })
        }
        fn to_data(&self) -> Option<Vec<u8>> {
            self.data.clone()
        }
        fn size(&self) -> Option<usize> {
            self.size
        }
    }

    fn thing(size: Option<usize>) -> Thing {
        Thing {
            size,
            data: None,
            located: true,
        }
    }

    fn file(bytes: Vec<u8>) -> tl::enums::upload::File {
        tl::enums::upload::File::File(tl::types::upload::File {
            r#type: tl::enums::storage::FileType::FileUnknown,
            mtime: 0,
            bytes,
        })
    }

    fn client(server: &FakeServer) -> Client {
        let pool = SenderPool::new(server.session(), 1);
        tokio::spawn(pool.runner.run());
        Client::new(pool.handle)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn chunks_come_in_order_until_a_short_one() {
        let server = FakeServer::start().await;
        server.on(|r: tl::functions::upload::GetFile| {
            assert!(r.cdn_supported);
            Ok(file(vec![
                r.offset as u8 / 4;
                (3 * 4096 - r.offset as usize).min(4096)
            ]))
        });
        let client = client(&server);
        let mut it = client.iter_download(&thing(None)).chunk_size(4096);
        let mut sizes = Vec::new();
        while let Some(chunk) = it.next().await.unwrap() {
            sizes.push(chunk.len());
        }
        // the third chunk is full, the fourth asks past the end and comes back empty
        assert_eq!(sizes, [4096, 4096, 4096]);
        assert!(it.next().await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_known_size_ends_the_download_without_an_extra_request() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::upload::GetFile| Ok(file(vec![1; 4096])));
        let client = client(&server);
        let mut it = client.iter_download(&thing(Some(4096))).chunk_size(4096);
        assert_eq!(it.next().await.unwrap().unwrap().len(), 4096);
        assert!(it.next().await.unwrap().is_none());
        assert_eq!(
            server
                .requests()
                .iter()
                .filter(|r| **r == "GetFile")
                .count(),
            1
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn offset_invalid_after_the_first_chunk_is_the_end_of_the_file() {
        let server = FakeServer::start().await;
        server.on(|r: tl::functions::upload::GetFile| {
            if r.offset == 0 {
                Ok(file(vec![1; 4096]))
            } else {
                Err((400, "OFFSET_INVALID".into()))
            }
        });
        let client = client(&server);
        let mut it = client.iter_download(&thing(None)).chunk_size(4096);
        assert_eq!(it.next().await.unwrap().unwrap().len(), 4096);
        assert!(it.next().await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn offset_invalid_at_the_start_and_other_errors_are_errors_that_can_be_retried() {
        let server = FakeServer::start().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        server.on(
            move |_: tl::functions::upload::GetFile| match seen.fetch_add(1, SeqCst) {
                0 => Err((400, "OFFSET_INVALID".into())),
                1 => Err((400, "LOCATION_INVALID".into())),
                _ => Ok(file(vec![9; 10])),
            },
        );
        let client = client(&server);
        let mut it = client.iter_download(&thing(None));
        for _ in 0..2 {
            let InvocationError::Rpc(_) = it.next().await.unwrap_err() else {
                panic!("not an rpc error")
            };
        }
        // the failed chunk was not lost: asking again gets it
        assert_eq!(it.next().await.unwrap().unwrap(), vec![9; 10]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn predownloaded_and_not_downloadable_media() {
        let server = FakeServer::start().await;
        let client = client(&server);
        let data = Thing {
            size: None,
            data: Some(vec![1, 2]),
            located: false,
        };
        let mut it = client.iter_download(&data);
        assert_eq!(it.next().await.unwrap().unwrap(), vec![1, 2]);
        assert!(it.next().await.unwrap().is_none());
        let none = Thing {
            size: None,
            data: None,
            located: false,
        };
        let mut it = client.iter_download(&none);
        assert!(matches!(it.next().await, Err(InvocationError::Io(_))));
        assert!(server.requests().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_big_file_is_fetched_by_several_workers_and_written_in_place() {
        let server = FakeServer::start().await;
        let size = 3 * MAX_CHUNK_SIZE as usize + 100;
        server.on(move |r: tl::functions::upload::GetFile| {
            let from = r.offset as usize;
            let to = (from + r.limit as usize).min(size);
            Ok(file((from..to).map(|i| (i / 1000) as u8).collect()))
        });
        let client = client(&server);
        let dir = std::env::temp_dir().join(format!("mtg-files-test-{}", std::process::id()));
        let path = dir.with_extension("bin");
        client
            .download_media_concurrent(
                thing(Some(size)).to_raw_input_location().unwrap(),
                size,
                &path,
                2,
                2,
            )
            .await
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(bytes.len(), size);
        let bad = bytes
            .iter()
            .enumerate()
            .find(|(i, b)| **b != (i / 1000) as u8);
        assert_eq!(bad, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn download_media_writes_a_small_file_completely() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::upload::GetFile| Ok(file(vec![7; 4101])));
        let client = client(&server);
        let path = std::env::temp_dir().join(format!("mtg-small-{}.bin", std::process::id()));
        client
            .download_media(&thing(Some(4101)), &path)
            .await
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(bytes, vec![7; 4101]);
    }

    fn flaky(server: &FakeServer, first: (i32, &'static str)) {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        server.on(move |_: tl::functions::upload::GetFile| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Err((first.0, first.1.to_string()))
            } else {
                Ok(file(vec![9; 10]))
            }
        });
    }

    /// A message in `peer` whose document carries `reference`.
    fn with_document(peer: PeerRef, reference: Option<Vec<u8>>) -> tl::enums::messages::Messages {
        let mut m = crate::test_util::message(7, peer, "", 0);
        if let (tl::enums::Message::Message(m), Some(file_reference)) = (&mut m, reference) {
            m.media = Some(
                tl::types::MessageMediaDocument {
                    nopremium: false,
                    spoiler: false,
                    video: false,
                    round: false,
                    voice: false,
                    document: Some(
                        tl::types::Document {
                            id: 1,
                            access_hash: 2,
                            file_reference,
                            date: 0,
                            mime_type: "a/b".into(),
                            size: 10,
                            thumbs: None,
                            video_thumbs: None,
                            dc_id: 2,
                            attributes: Vec::new(),
                        }
                        .into(),
                    ),
                    alt_documents: None,
                    video_cover: None,
                    video_timestamp: None,
                    ttl_seconds: None,
                }
                .into(),
            );
        }
        tl::types::messages::Messages {
            messages: vec![m],
            topics: Vec::new(),
            chats: Vec::new(),
            users: Vec::new(),
        }
        .into()
    }

    struct Doc(Vec<u8>);

    impl Downloadable for Doc {
        fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
            Some(
                tl::types::InputDocumentFileLocation {
                    id: 1,
                    access_hash: 2,
                    file_reference: self.0.clone(),
                    thumb_size: String::new(),
                }
                .into(),
            )
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_expired_file_reference_is_refreshed_from_the_message_once() {
        let server = FakeServer::start().await;
        let peer = crate::test_util::user_ref(5);
        server.on(move |req: tl::functions::upload::GetFile| {
            let tl::enums::InputFileLocation::InputDocumentFileLocation(l) = req.location else {
                panic!("not a document")
            };
            if l.file_reference == [2] {
                Ok(file(vec![9; 10]))
            } else {
                Err((400, "FILE_REFERENCE_EXPIRED".into()))
            }
        });
        let fetches = Arc::new(AtomicUsize::new(0));
        let seen = fetches.clone();
        server.on(move |_: tl::functions::messages::GetMessages| {
            seen.fetch_add(1, SeqCst);
            Ok(with_document(peer, Some(vec![2])))
        });
        let client = client(&server);
        let mut it = client.iter_download(&Doc(vec![1])).refresh_from(peer, 7);
        assert_eq!(it.next().await.unwrap().unwrap(), vec![9; 10]);
        assert_eq!(fetches.load(SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_file_reference_that_stays_bad_fails_after_one_refresh() {
        let server = FakeServer::start().await;
        let peer = crate::test_util::user_ref(5);
        server.on(|_: tl::functions::upload::GetFile| Err((400, "FILE_REFERENCE_EXPIRED".into())));
        let fetches = Arc::new(AtomicUsize::new(0));
        let seen = fetches.clone();
        server.on(move |_: tl::functions::messages::GetMessages| {
            seen.fetch_add(1, SeqCst);
            Ok(with_document(peer, Some(vec![2])))
        });
        let client = client(&server);
        let mut it = client.iter_download(&Doc(vec![1])).refresh_from(peer, 7);
        let InvocationError::Rpc(e) = it.next().await.unwrap_err() else {
            panic!("not an rpc error")
        };
        assert_eq!(e.name, "FILE_REFERENCE_EXPIRED");
        assert_eq!(fetches.load(SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nothing_to_refresh_from_leaves_the_error() {
        let server = FakeServer::start().await;
        let peer = crate::test_util::user_ref(5);
        server.on(|_: tl::functions::upload::GetFile| Err((400, "FILE_REFERENCE_EXPIRED".into())));
        let client = client(&server);
        // no source at all
        let mut it = client.iter_download(&Doc(vec![1]));
        assert!(matches!(it.next().await, Err(InvocationError::Rpc(_))));
        // the message lost its media
        server.on(move |_: tl::functions::messages::GetMessages| Ok(with_document(peer, None)));
        let mut it = client.iter_download(&Doc(vec![1])).refresh_from(peer, 7);
        assert!(matches!(it.next().await, Err(InvocationError::Rpc(_))));
        // the location kind has no file reference
        let mut it = client.iter_download(&thing(None)).refresh_from(peer, 7);
        assert!(matches!(it.next().await, Err(InvocationError::Rpc(_))));
        // the lookup itself fails
        server.on(|_: tl::functions::messages::GetMessages| Err((500, "INTERNAL".into())));
        let mut it = client.iter_download(&Doc(vec![1])).refresh_from(peer, 7);
        let InvocationError::Rpc(e) = it.next().await.unwrap_err() else {
            panic!("not an rpc error")
        };
        assert_eq!(e.name, "INTERNAL");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn skipping_chunks_moves_the_offset() {
        let server = FakeServer::start().await;
        let offsets = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = offsets.clone();
        server.on(move |r: tl::functions::upload::GetFile| {
            seen.lock().unwrap().push(r.offset);
            Ok(file(vec![1; 10]))
        });
        let client = client(&server);
        let mut it = client
            .iter_download(&thing(None))
            .chunk_size(4096)
            .skip_chunks(3);
        it.next().await.unwrap();
        assert_eq!(*offsets.lock().unwrap(), [3 * 4096]);
        // a pre-downloaded iterator ignores both
        let data = Thing {
            size: None,
            data: Some(vec![1]),
            located: true,
        };
        let mut it = client.iter_download(&data).chunk_size(4096).skip_chunks(1);
        assert_eq!(it.next().await.unwrap(), Some(vec![1]));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_file_migrate_error_retries_and_an_unregistered_key_is_exported() {
        let server = FakeServer::start().await;
        flaky(&server, (303, "FILE_MIGRATE_2"));
        let client = client(&server);
        let mut it = client.iter_download(&thing(None));
        assert_eq!(it.next().await.unwrap().unwrap().len(), 10);

        let server = FakeServer::start().await;
        flaky(&server, (401, "AUTH_KEY_UNREGISTERED"));
        let client = crate::test_util::client(&server);
        let mut it = client.iter_download(&thing(None));
        assert_eq!(it.next().await.unwrap().unwrap().len(), 10);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn concurrent_workers_retry_migrations_and_report_errors() {
        let size = 2 * MAX_CHUNK_SIZE as usize;
        let path = std::env::temp_dir().join(format!("mtg-conc-{}.bin", std::process::id()));
        for first in [(303, "FILE_MIGRATE_2"), (401, "AUTH_KEY_UNREGISTERED")] {
            let server = FakeServer::start().await;
            let calls = std::sync::atomic::AtomicUsize::new(0);
            server.on(move |r: tl::functions::upload::GetFile| {
                if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    return Err((first.0, first.1.to_string()));
                }
                let n = (size - r.offset as usize).min(r.limit as usize);
                Ok(file(vec![3; n]))
            });
            let client = client(&server);
            client
                .download_media_concurrent(
                    thing(Some(size)).to_raw_input_location().unwrap(),
                    size,
                    &path,
                    1,
                    2,
                )
                .await
                .unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), vec![3; size]);
        }

        let server = FakeServer::start().await;
        server.on(|_: tl::functions::upload::GetFile| Err((400, "LOCATION_INVALID".to_string())));
        let r = client(&server)
            .download_media_concurrent(
                thing(Some(size)).to_raw_input_location().unwrap(),
                size,
                &path,
                2,
                2,
            )
            .await;
        assert!(matches!(r, Err(e) if e.to_string().contains("LOCATION_INVALID")));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_file_over_the_big_threshold_goes_through_the_workers() {
        let server = FakeServer::start().await;
        let size = BIG_FILE_SIZE + 1;
        server.on(move |r: tl::functions::upload::GetFile| {
            let n = (size - r.offset as usize).min(r.limit as usize);
            Ok(file(vec![5; n]))
        });
        let path = std::env::temp_dir().join(format!("mtg-big-{}.bin", std::process::id()));
        client(&server)
            .download_media(&thing(Some(size)), &path)
            .await
            .unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(len as usize, size);
    }
}
