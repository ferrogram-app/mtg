//! Parts of a file served by a CDN datacenter (<https://core.telegram.org/cdn>).
//!
//! `upload.getFile` with `cdn_supported` may answer with a redirect. The part is then read from
//! the CDN DC with `upload.getCdnFile`, decrypted (AES-256-CTR, see
//! [`mtg_crypto::aes::ctr_at_offset`]) and checked against SHA-256 hashes the master DC vouches
//! for, before it is handed on. Any trouble but a failed hash check ends in the ordinary
//! `upload.getFile` of the same part.
//!
//! shortcut: stateless, one master round trip per part (the redirect and the hashes are asked
//! again for each part); keep them per file if that cost shows up in transfers.

use mtg_mtsender::InvocationError;
use mtg_tl_types as tl;
use tracing::warn;

use super::Client;

/// Hash fetches (`upload.getCdnFileHashes`) allowed for one part.
const MAX_HASH_FETCHES: usize = 16;

#[derive(Debug, PartialEq)]
pub(crate) enum Check {
    /// No hash starts at this file offset.
    Missing(i64),
    /// A part does not match its hash (or the hash makes no sense).
    Mismatch,
}

/// Checks `data`, the decrypted bytes of the file from `offset`, against `hashes`: each hash
/// covers `limit` bytes from its own offset; the last one may cover fewer (the end of the file).
pub(crate) fn check(data: &[u8], offset: i64, hashes: &[tl::types::FileHash]) -> Result<(), Check> {
    let mut pos = 0;
    while pos < data.len() {
        let at = offset + pos as i64;
        let Some(hash) = hashes.iter().find(|h| h.offset == at) else {
            return Err(Check::Missing(at));
        };
        if hash.limit <= 0 {
            return Err(Check::Mismatch);
        }
        let end = data.len().min(pos + hash.limit as usize);
        if mtg_crypto::sha256_parts(&[&data[pos..end]])[..] != hash.hash[..] {
            return Err(Check::Mismatch);
        }
        pos = end;
    }
    Ok(())
}

enum Failure {
    /// The CDN sent data that does not match its hash: never retried elsewhere.
    Tampered,
    /// Anything else: the part is asked from the master DC instead.
    Other(InvocationError),
}

impl From<InvocationError> for Failure {
    fn from(e: InvocationError) -> Self {
        Self::Other(e)
    }
}

fn hashes(list: &[tl::enums::FileHash]) -> impl Iterator<Item = tl::types::FileHash> {
    list.iter().map(|tl::enums::FileHash::Hash(h)| h.clone())
}

impl Client {
    /// One part of a file: `upload.getFile`, following a CDN redirect when there is one.
    pub(crate) async fn get_file_part(
        &self,
        dc: i32,
        request: &tl::functions::upload::GetFile,
    ) -> Result<Vec<u8>, InvocationError> {
        use tl::enums::upload::File;
        let mut ask = request.clone();
        ask.cdn_supported = true;
        let redirect = match self.invoke_media_in_dc(dc, &ask).await? {
            File::File(file) => return Ok(file.bytes),
            File::CdnRedirect(r) => r,
        };
        match self
            .cdn_part(dc, &redirect, request.offset, request.limit)
            .await
        {
            Ok(bytes) => Ok(bytes),
            Err(Failure::Tampered) => {
                Err(InvocationError::Dropped("CDN data does not match its hash"))
            }
            Err(Failure::Other(error)) => {
                warn!(%error, "CDN download failed; asking the datacenter itself");
                ask.cdn_supported = false;
                match self.invoke_media_in_dc(dc, &ask).await? {
                    File::File(file) => Ok(file.bytes),
                    File::CdnRedirect(_) => Err(InvocationError::Dropped("unexpected CdnRedirect")),
                }
            }
        }
    }

    async fn cdn_part(
        &self,
        master: i32,
        redirect: &tl::types::upload::FileCdnRedirect,
        offset: i64,
        limit: i32,
    ) -> Result<Vec<u8>, Failure> {
        use tl::enums::upload::CdnFile;
        self.load_cdn_keys().await?;
        let bad = |what| Failure::Other(InvocationError::Dropped(what));
        let key: [u8; 32] = (redirect.encryption_key.as_slice())
            .try_into()
            .map_err(|_| bad("CDN key is not 32 bytes"))?;
        let iv: [u8; 16] = (redirect.encryption_iv.as_slice())
            .try_into()
            .map_err(|_| bad("CDN iv is not 16 bytes"))?;
        let mut known: Vec<_> = hashes(&redirect.file_hashes).collect();
        let get = tl::functions::upload::GetCdnFile {
            file_token: redirect.file_token.clone(),
            offset,
            limit,
        };

        let mut file = self.0.handle.invoke_in_dc(redirect.dc_id, &get).await?;
        if let CdnFile::ReuploadNeeded(tl::types::upload::CdnFileReuploadNeeded { request_token }) =
            file
        {
            // the CDN does not have the file yet: the master DC puts it there and vouches for it
            let more = self
                .invoke_in_dc(
                    master,
                    &tl::functions::upload::ReuploadCdnFile {
                        file_token: redirect.file_token.clone(),
                        request_token,
                    },
                )
                .await?;
            known.extend(hashes(&more));
            file = self.0.handle.invoke_in_dc(redirect.dc_id, &get).await?;
        }
        let CdnFile::File(tl::types::upload::CdnFile { mut bytes }) = file else {
            return Err(bad("the CDN does not have the file after a reupload"));
        };
        if !mtg_crypto::aes::ctr_at_offset(&mut bytes, &key, &iv, offset as u64) {
            return Err(bad("CDN offset cannot be decrypted"));
        }

        for _ in 0..MAX_HASH_FETCHES {
            match check(&bytes, offset, &known) {
                Ok(()) => return Ok(bytes),
                Err(Check::Mismatch) => return Err(Failure::Tampered),
                Err(Check::Missing(at)) => {
                    let more = self
                        .invoke_in_dc(
                            master,
                            &tl::functions::upload::GetCdnFileHashes {
                                file_token: redirect.file_token.clone(),
                                offset: at,
                            },
                        )
                        .await?;
                    if more.is_empty() {
                        return Err(bad("the master datacenter has no hashes for the part"));
                    }
                    known.extend(hashes(&more));
                }
            }
        }
        Err(bad("too many hash requests for one part"))
    }

    /// Gives the pool the keys CDN datacenters sign their handshake with (once).
    async fn load_cdn_keys(&self) -> Result<(), InvocationError> {
        self.0
            .cdn_keys
            .get_or_try_init(|| async {
                let tl::enums::CdnConfig::Config(config) =
                    self.invoke(&tl::functions::help::GetCdnConfig {}).await?;
                self.0.handle.set_cdn_keys(&config);
                Ok(())
            })
            .await
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_mtsender::SenderPool;
    use mtg_session::Session;
    use mtg_session::types::DcOption;
    use mtg_testkit::FakeServer;

    fn hash(offset: i64, data: &[u8]) -> tl::types::FileHash {
        tl::types::FileHash {
            offset,
            limit: data.len() as i32,
            hash: mtg_crypto::sha256_parts(&[data]).to_vec(),
        }
    }

    #[test]
    fn check_accepts_matching_hashes_and_reports_the_rest() {
        let data = [1u8; 300];
        let hashes = [hash(1000, &data[..200]), hash(1200, &data[200..])];
        assert_eq!(check(&data, 1000, &hashes), Ok(()));
        assert_eq!(check(&[], 0, &[]), Ok(()));
        assert_eq!(check(&data, 1000, &hashes[..1]), Err(Check::Missing(1200)));
        assert_eq!(check(&data, 0, &hashes), Err(Check::Missing(0)));
        let mut bad = hashes.clone();
        bad[1].hash[0] ^= 1;
        assert_eq!(check(&data, 1000, &bad), Err(Check::Mismatch));
        bad[0].limit = 0;
        assert_eq!(check(&data, 1000, &bad), Err(Check::Mismatch));
    }

    const KEY: [u8; 32] = [3; 32];
    const IV: [u8; 16] = [5; 16];

    fn plain() -> Vec<u8> {
        (0..4096u32).map(|i| i as u8).collect()
    }

    fn encrypted() -> Vec<u8> {
        let mut data = plain();
        assert!(mtg_crypto::aes::ctr_at_offset(&mut data, &KEY, &IV, 0));
        data
    }

    fn redirect(hashes: Vec<tl::types::FileHash>) -> tl::enums::upload::File {
        tl::enums::upload::File::CdnRedirect(tl::types::upload::FileCdnRedirect {
            dc_id: 203,
            file_token: vec![1],
            encryption_key: KEY.to_vec(),
            encryption_iv: IV.to_vec(),
            file_hashes: hashes.into_iter().map(Into::into).collect(),
        })
    }

    fn request() -> tl::functions::upload::GetFile {
        tl::functions::upload::GetFile {
            precise: true,
            cdn_supported: false,
            location: tl::types::InputPeerPhotoFileLocation {
                big: false,
                peer: tl::types::InputPeerSelf {}.into(),
                photo_id: 1,
            }
            .into(),
            offset: 0,
            limit: 4096,
        }
    }

    /// A client whose DC 203 is a CDN one living on the same fake server.
    async fn client(server: &FakeServer) -> Client {
        let port = server.addr.port();
        server.on(move |_: tl::functions::help::GetConfig| {
            let tl::enums::Config::Config(mut config) = mtg_testkit::config();
            config.dc_options = vec![tl::enums::DcOption::Option(tl::types::DcOption {
                ipv6: false,
                media_only: false,
                tcpo_only: false,
                cdn: true,
                r#static: false,
                this_port_only: false,
                id: 203,
                ip_address: "127.0.0.1".into(),
                port: port as i32,
                secret: None,
            })];
            Ok(tl::enums::Config::Config(config))
        });
        server.on(|_: tl::functions::help::GetCdnConfig| {
            Ok(tl::enums::CdnConfig::Config(tl::types::CdnConfig {
                public_keys: Vec::new(),
            }))
        });
        let session = server.session();
        let tl_dc = session.dc_option(2).unwrap().unwrap();
        session
            .set_dc_option(&DcOption {
                id: 203,
                ipv4: tl_dc.ipv4,
                ipv6: tl_dc.ipv6,
                auth_key: Some(mtg_testkit::KEY),
            })
            .await
            .unwrap();
        let pool = SenderPool::new(session, 1);
        tokio::spawn(pool.runner.run());
        let client = Client::new(pool.handle);
        // brings the config that lists DC 203 as a CDN one
        client
            .invoke_in_dc(2, &tl::functions::help::GetConfig {})
            .await
            .unwrap();
        client
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_part_from_a_cdn_is_decrypted_and_verified() {
        let server = FakeServer::start().await;
        let hashes = vec![hash(0, &plain())];
        server.on(move |_: tl::functions::upload::GetFile| Ok(redirect(hashes.clone())));
        server.on(|_: tl::functions::upload::GetCdnFile| {
            Ok(tl::enums::upload::CdnFile::File(
                tl::types::upload::CdnFile { bytes: encrypted() },
            ))
        });
        let client = client(&server).await;
        assert_eq!(client.get_file_part(2, &request()).await.unwrap(), plain());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_cdn_without_the_file_is_reuploaded_and_missing_hashes_are_fetched() {
        use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::upload::GetFile| Ok(redirect(Vec::new())));
        let uploaded = std::sync::Arc::new(AtomicBool::new(false));
        let flag = uploaded.clone();
        server.on(move |_: tl::functions::upload::GetCdnFile| {
            Ok(if flag.load(SeqCst) {
                tl::enums::upload::CdnFile::File(tl::types::upload::CdnFile { bytes: encrypted() })
            } else {
                tl::enums::upload::CdnFile::ReuploadNeeded(
                    tl::types::upload::CdnFileReuploadNeeded {
                        request_token: vec![9],
                    },
                )
            })
        });
        server.on(move |r: tl::functions::upload::ReuploadCdnFile| {
            assert_eq!(r.request_token, vec![9]);
            uploaded.store(true, SeqCst);
            Ok(Vec::new())
        });
        server.on(|_: tl::functions::upload::GetCdnFileHashes| Ok(vec![hash(0, &plain()).into()]));
        let client = client(&server).await;
        assert_eq!(client.get_file_part(2, &request()).await.unwrap(), plain());
        let requests = server.requests();
        assert!(requests.iter().any(|r| r.contains("ReuploadCdnFile")));
        assert!(requests.iter().any(|r| r.contains("GetCdnFileHashes")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tampered_cdn_data_is_a_hard_error() {
        let server = FakeServer::start().await;
        let mut wrong = hash(0, &plain());
        wrong.hash[0] ^= 1;
        server.on(move |_: tl::functions::upload::GetFile| Ok(redirect(vec![wrong.clone()])));
        server.on(|_: tl::functions::upload::GetCdnFile| {
            Ok(tl::enums::upload::CdnFile::File(
                tl::types::upload::CdnFile { bytes: encrypted() },
            ))
        });
        let client = client(&server).await;
        let error = client.get_file_part(2, &request()).await.unwrap_err();
        assert!(matches!(error, InvocationError::Dropped(m) if m.contains("hash")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_cdn_that_fails_falls_back_to_the_datacenter() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        let server = FakeServer::start().await;
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        server.on(move |r: tl::functions::upload::GetFile| {
            Ok(if seen.fetch_add(1, SeqCst) == 0 {
                assert!(r.cdn_supported);
                redirect(Vec::new())
            } else {
                assert!(!r.cdn_supported);
                tl::enums::upload::File::File(tl::types::upload::File {
                    r#type: tl::enums::storage::FileType::FileUnknown,
                    mtime: 0,
                    bytes: vec![1, 2, 3],
                })
            })
        });
        server.on(|_: tl::functions::upload::GetCdnFile| Err((400, "FILE_TOKEN_INVALID".into())));
        let client = client(&server).await;
        assert_eq!(
            client.get_file_part(2, &request()).await.unwrap(),
            vec![1, 2, 3]
        );
        assert_eq!(calls.load(SeqCst), 2);
    }
}
