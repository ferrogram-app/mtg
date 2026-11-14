//! The fake-TLS layer of an MTProxy connection: handshake, then records both ways.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use mtg_mtproto::transport::fake_tls::{
    Error, MAX_RECORD_PAYLOAD, RecordDecoder, RecordEncoder, check_server_hello, client_hello,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::net::tcp::{ReadHalf, WriteHalf};

/// More than any real answer: a peer sending this much without an answer is not a proxy.
const MAX_HELLO_ANSWER: usize = 64 * 1024;

fn invalid(e: Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

pub struct TlsStream {
    tcp: TcpStream,
    decoder: RecordDecoder,
    encoder: RecordEncoder,
    /// The record being written, how much of it is out, and how many caller bytes it holds.
    out: Vec<u8>,
    out_pos: usize,
    owed: usize,
}

impl TlsStream {
    pub async fn connect(
        address: SocketAddr,
        secret: &[u8; 16],
        domain: &str,
        unix_time: i32,
    ) -> io::Result<Self> {
        let mut tcp = TcpStream::connect(address).await?;
        let (hello, digest) = client_hello(domain, secret, unix_time);
        tcp.write_all(&hello).await?;

        let mut answer = Vec::new();
        let mut chunk = [0u8; 4096];
        let total = loop {
            match check_server_hello(secret, &digest, &answer) {
                Ok(total) => break total,
                Err(Error::MissingBytes) if answer.len() < MAX_HELLO_ANSWER => {}
                Err(e) => return Err(invalid(e)),
            }
            let n = tcp.read(&mut chunk).await?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            answer.extend_from_slice(&chunk[..n]);
        };
        if answer.len() > total {
            // we have not spoken yet, so nothing more may follow the answer
            return Err(invalid(Error::BadRecord));
        }
        Ok(Self {
            tcp,
            decoder: RecordDecoder::default(),
            encoder: RecordEncoder::default(),
            out: Vec::new(),
            out_pos: 0,
            owed: 0,
        })
    }

    pub fn split(&mut self) -> (TlsRead<'_>, TlsWrite<'_>) {
        let (read, write) = self.tcp.split();
        (
            TlsRead {
                half: read,
                decoder: &mut self.decoder,
            },
            TlsWrite {
                half: write,
                encoder: &mut self.encoder,
                out: &mut self.out,
                out_pos: &mut self.out_pos,
                owed: &mut self.owed,
            },
        )
    }

    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.tcp.shutdown().await
    }
}

pub struct TlsRead<'a> {
    half: ReadHalf<'a>,
    decoder: &'a mut RecordDecoder,
}

impl AsyncRead for TlsRead<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let before = buf.filled().len();
            std::task::ready!(Pin::new(&mut self.half).poll_read(cx, buf))?;
            let got = buf.filled().len() - before;
            if got == 0 {
                return Poll::Ready(Ok(())); // end of stream
            }
            let this = &mut *self;
            let payload = this
                .decoder
                .decode(&mut buf.filled_mut()[before..])
                .map_err(invalid)?;
            buf.set_filled(before + payload);
            if payload > 0 {
                return Poll::Ready(Ok(()));
            }
            // only record headers arrived: read on
        }
    }
}

pub struct TlsWrite<'a> {
    half: WriteHalf<'a>,
    encoder: &'a mut RecordEncoder,
    out: &'a mut Vec<u8>,
    out_pos: &'a mut usize,
    owed: &'a mut usize,
}

impl AsyncWrite for TlsWrite<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = &mut *self;
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if *this.owed == 0 {
            // a new record; a record left half-written by a cancelled call is finished first
            let n = buf.len().min(MAX_RECORD_PAYLOAD);
            this.out.clear();
            *this.out_pos = 0;
            this.encoder.encode(&buf[..n], this.out);
            *this.owed = n;
        }
        while *this.out_pos < this.out.len() {
            let k = std::task::ready!(
                Pin::new(&mut this.half).poll_write(cx, &this.out[*this.out_pos..])
            )?;
            if k == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            *this.out_pos += k;
        }
        Poll::Ready(Ok(std::mem::take(this.owed)))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.half).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.half).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_mtproto::transport::fake_tls::server_hello;
    use tokio::net::TcpListener;

    const SECRET: [u8; 16] = [9; 16];

    /// A peer that answers the hello, then returns every payload byte it receives.
    async fn peer(listener: TcpListener, expect: usize) -> Vec<u8> {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut head = [0u8; 5];
        sock.read_exact(&mut head).await.unwrap();
        let mut hello = head.to_vec();
        hello.resize(5 + u16::from_be_bytes([head[3], head[4]]) as usize, 0);
        sock.read_exact(&mut hello[5..]).await.unwrap();
        let mut seed = (0u8..=255)
            .cycle()
            .map(|i| i.wrapping_mul(37).wrapping_add(11));
        sock.write_all(&server_hello(&SECRET, &hello, &mut |b| {
            b.iter_mut()
                .for_each(|x| *x = x.wrapping_add(seed.next().unwrap()))
        }))
        .await
        .unwrap();
        let mut ccs = [0u8; 6];
        sock.read_exact(&mut ccs).await.unwrap();
        assert_eq!(ccs, [0x14, 3, 3, 0, 1, 1]);
        let mut decoder = RecordDecoder::default();
        let mut got = Vec::new();
        let mut buf = [0u8; 1000];
        while got.len() < expect {
            let n = sock.read(&mut buf).await.unwrap();
            let m = decoder.decode(&mut buf[..n]).unwrap();
            got.extend_from_slice(&buf[..m]);
        }
        // and the way back
        let mut wire = Vec::new();
        RecordEncoder::established().encode(&[1, 2, 3], &mut wire);
        sock.write_all(&wire[..2]).await.unwrap(); // a split header
        sock.write_all(&wire[2..]).await.unwrap();
        got
    }

    #[tokio::test]
    async fn big_writes_are_split_into_records_and_reads_see_only_payload() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let data: Vec<u8> = (0..10_000u32).map(|i| i as u8).collect();
        let server = tokio::spawn(peer(listener, data.len()));

        let mut stream = TlsStream::connect(addr, &SECRET, "example.com", 7)
            .await
            .unwrap();
        let (mut reader, mut writer) = stream.split();
        writer.write_all(&data).await.unwrap();
        assert_eq!(writer.write(&[]).await.unwrap(), 0);
        let mut back = [0u8; 8];
        let n = reader.read(&mut back).await.unwrap();
        assert_eq!(&back[..n], &[1, 2, 3][..n]);
        assert_eq!(server.await.unwrap(), data);
        stream.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn a_peer_that_is_not_a_proxy_is_rejected() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            sock.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                .await
                .unwrap();
        });
        let err = TlsStream::connect(addr, &SECRET, "example.com", 7)
            .await
            .err()
            .unwrap();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn a_peer_that_hangs_up_during_the_handshake_is_an_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
        });
        let err = TlsStream::connect(addr, &SECRET, "example.com", 7)
            .await
            .err()
            .unwrap();
        // a reset instead of a clean close happens when the peer drops unread bytes
        assert!(matches!(
            err.kind(),
            io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset
        ));
    }
}
