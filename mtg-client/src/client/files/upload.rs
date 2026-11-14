use super::*;

impl Client {
    /// Uploads an async stream to Telegram servers.
    ///
    /// The file is not sent to any peer, but can be used as media when sending messages for a
    /// certain period of time (less than a day). You can use this uploaded file multiple times.
    ///
    /// Refer to [`InputMessage`] to learn more uses for `uploaded_file`.
    ///
    /// The stream size must be known beforehand. If this is not possible, you might need to
    /// process the entire async stream to determine its size, and then use the size and the
    /// downloaded buffer.
    ///
    /// The stream size may be less or equal to the actual length of the stream, but not more.
    /// If it's less, you may continue to read from the stream after the method returns.
    /// If it's more, the method will fail because it does not have enough data to read.
    ///
    /// Note that Telegram uses the file name in certain methods, for example, to make sure the
    /// file is an image when trying to use send the file as photo media, so it is important that
    /// the file name at least uses the right extension, even if the name is a dummy value.
    /// If the input file name is empty, the non-empty dummy value "a" will be used instead.
    /// Because it has no extension, you may not be able to use the file in certain methods.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client, some_vec: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::InputMessage;
    ///
    /// // In-memory `Vec<u8>` buffers can be used as async streams
    /// let size = some_vec.len();
    /// let mut stream = std::io::Cursor::new(some_vec);
    /// let uploaded_file = client.upload_stream(&mut stream, size, "sleep.jpg".to_string()).await?;
    ///
    /// client.send_message(peer, InputMessage::new().text("Zzz...").photo(uploaded_file)).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`InputMessage`]: crate::message::InputMessage
    pub async fn upload_stream<S: AsyncRead + Unpin>(
        &self,
        stream: &mut S,
        size: usize,
        name: String,
    ) -> Result<Uploaded, io::Error> {
        let file_id = generate_random_id();
        let name = if name.is_empty() {
            "a".to_string()
        } else {
            name
        };

        let big_file = size > BIG_FILE_SIZE;
        let parts = PartStream::new(stream, size);
        let total_parts = parts.total_parts();

        if big_file {
            let parts = Arc::new(parts);
            let mut tasks = FuturesUnordered::new();
            for _ in 0..WORKER_COUNT {
                let handle = self.clone();
                let parts = Arc::clone(&parts);
                let task = async move {
                    while let Some((part, bytes)) = parts.next_part().await? {
                        let ok = handle
                            .invoke(&tl::functions::upload::SaveBigFilePart {
                                file_id,
                                file_part: part,
                                file_total_parts: total_parts,
                                bytes,
                            })
                            .await
                            .map_err(io::Error::other)?;

                        if !ok {
                            return Err(io::Error::other("server failed to store uploaded data"));
                        }
                    }
                    Ok(())
                };
                tasks.push(task);
            }

            while let Some(res) = tasks.next().await {
                res?;
            }

            Ok(Uploaded::from_raw(
                tl::types::InputFileBig {
                    id: file_id,
                    parts: total_parts,
                    name,
                }
                .into(),
            ))
        } else {
            let mut md5 = md5::Context::new();
            while let Some((part, bytes)) = parts.next_part().await? {
                md5.consume(&bytes);
                let ok = self
                    .invoke(&tl::functions::upload::SaveFilePart {
                        file_id,
                        file_part: part,
                        bytes,
                    })
                    .await
                    .map_err(io::Error::other)?;

                if !ok {
                    return Err(io::Error::other("server failed to store uploaded data"));
                }
            }
            Ok(Uploaded::from_raw(
                tl::types::InputFile {
                    id: file_id,
                    parts: total_parts,
                    name,
                    md5_checksum: format!("{:x}", md5.finalize()),
                }
                .into(),
            ))
        }
    }

    /// Uploads a local file to Telegram servers.
    ///
    /// The file is not sent to any peer, but can be used as media when sending messages for a
    /// certain period of time (less than a day). You can use this uploaded file multiple times.
    ///
    /// Refer to [`InputMessage`] to learn more uses for `uploaded_file`.
    ///
    /// If you need more control over the uploaded data, such as performing only a partial upload
    /// or with a different name, use [`Client::upload_stream`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::InputMessage;
    ///
    /// let uploaded_file = client.upload_file("/home/username/photos/holidays.jpg").await?;
    ///
    /// client.send_message(peer, InputMessage::new().text("Check this out!").photo(uploaded_file)).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`InputMessage`]: crate::message::InputMessage
    #[cfg(feature = "fs")]
    pub async fn upload_file<P: AsRef<Path>>(&self, path: P) -> Result<Uploaded, io::Error> {
        let path = path.as_ref();

        let mut file = fs::File::open(path).await?;
        let size = file.seek(SeekFrom::End(0)).await? as usize;
        file.seek(SeekFrom::Start(0)).await?;

        // File name will only be `None` for `..` path, and directories cannot be uploaded as
        // files, so it's fine to unwrap.
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        self.upload_stream(&mut file, size, name).await
    }
}

struct PartStreamInner<'a, S: AsyncRead + Unpin> {
    stream: &'a mut S,
    current_part: i32,
}

struct PartStream<'a, S: AsyncRead + Unpin> {
    inner: AsyncMutex<PartStreamInner<'a, S>>,
    total_parts: i32,
}

impl<'a, S: AsyncRead + Unpin> PartStream<'a, S> {
    fn new(stream: &'a mut S, size: usize) -> Self {
        let total_parts = size.div_ceil(MAX_CHUNK_SIZE as usize) as i32;
        Self {
            inner: AsyncMutex::new(PartStreamInner {
                stream,
                current_part: 0,
            }),
            total_parts,
        }
    }

    fn total_parts(&self) -> i32 {
        self.total_parts
    }

    async fn next_part(&self) -> Result<Option<(i32, Vec<u8>)>, io::Error> {
        let mut lock = self.inner.lock().await;
        if lock.current_part >= self.total_parts {
            return Ok(None);
        }
        let mut read = 0;
        let mut buffer = vec![0; MAX_CHUNK_SIZE as usize];

        while read != buffer.len() {
            let n = lock.stream.read(&mut buffer[read..]).await?;
            if n == 0 {
                if lock.current_part == self.total_parts - 1 {
                    break;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "reached EOF before reaching the last file part",
                    ));
                }
            }
            read += n;
        }

        let bytes = if read == buffer.len() {
            buffer
        } else {
            buffer[..read].to_vec()
        };

        let res = Ok(Some((lock.current_part, bytes)));
        lock.current_part += 1;
        res
    }
}
