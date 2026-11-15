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

        use tl::enums::upload::File;

        // TODO handle maybe FILEREF_UPGRADE_NEEDED
        let mut dc = self
            .dc_id
            .map_or_else(|| self.client.0.session.home_dc_id(), Ok)?;

        loop {
            break match self.client.invoke_media_in_dc(dc, &request).await {
                Ok(File::File(f)) => {
                    let reached_known_size = self
                        .size
                        .is_some_and(|size| request.offset as usize + f.bytes.len() >= size);

                    if reached_known_size || f.bytes.len() < request.limit as usize {
                        self.done = true;
                        if f.bytes.is_empty() {
                            break Ok(None);
                        }
                    }

                    request.offset += request.limit as i64;
                    self.variant = DownloadIterVariant::Request(request);

                    Ok(Some(f.bytes))
                }
                Ok(File::CdnRedirect(_)) => {
                    // the server ignored cdn_supported = false: fail instead of trusting it
                    break Err(InvocationError::Dropped("unexpected CdnRedirect"));
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
                Err(e) => Err(e),
            };
        }
    }
}

/// Method implementations related to uploading or downloading files.
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
            }
        } else {
            DownloadIter {
                client: self.clone(),
                done: false,
                size: None,
                variant: DownloadIterVariant::PreFailed(io::Error::other("media not downloadable")),
                dc_id: downloadable.dc_id(),
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
                    match client.invoke_media_in_dc(task_dc, request).await {
                        Ok(tl::enums::upload::File::File(file)) => {
                            if tx.send((offset as u64, file.bytes)).is_err() {
                                return Err(InvocationError::Dropped("download receiver closed"));
                            }
                        }
                        Ok(tl::enums::upload::File::CdnRedirect(_)) => {
                            return Err(InvocationError::Dropped("unexpected CdnRedirect"));
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

        // Check if all tasks finished succesfully
        for task in tasks {
            let res = task.await.map_err(io::Error::other)?;
            res?;
        }
        Ok(())
    }
}

mod upload;
