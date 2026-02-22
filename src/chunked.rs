use axum::body::Body;
use bytes::{Buf, BytesMut};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Decode an aws-chunked body into a plain data stream.
///
/// AWS chunked format:
/// ```text
/// <hex-size>;chunk-signature=<sig>\r\n
/// <data>\r\n
/// ...
/// 0;chunk-signature=<sig>\r\n
/// \r\n
/// ```
pub fn decode_aws_chunked(body: Body) -> Body {
    Body::from_stream(AwsChunkedDecoder::new(body))
}

enum DecoderState {
    /// Reading the chunk header line (e.g. "a;chunk-signature=...")
    ReadingHeader,
    /// Reading chunk data bytes; `remaining` bytes left to read
    ReadingData { remaining: usize },
    /// Reading the \r\n after chunk data
    ReadingDataTrailer,
    /// Terminal 0-size chunk seen; done
    Done,
}

struct AwsChunkedDecoder {
    body: Body,
    buf: BytesMut,
    state: DecoderState,
}

impl AwsChunkedDecoder {
    fn new(body: Body) -> Self {
        Self {
            body,
            buf: BytesMut::with_capacity(8192),
            state: DecoderState::ReadingHeader,
        }
    }
}

impl futures_core::Stream for AwsChunkedDecoder {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            match this.state {
                DecoderState::Done => return Poll::Ready(None),

                DecoderState::ReadingHeader => {
                    if let Some(pos) = find_crlf(&this.buf) {
                        let line = &this.buf[..pos];
                        let size_str = std::str::from_utf8(line)
                            .unwrap_or("")
                            .split(';')
                            .next()
                            .unwrap_or("0");
                        let chunk_size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
                        this.buf.advance(pos + 2);

                        if chunk_size == 0 {
                            this.state = DecoderState::Done;
                            return Poll::Ready(None);
                        }
                        this.state = DecoderState::ReadingData {
                            remaining: chunk_size,
                        };
                        continue;
                    }
                }

                DecoderState::ReadingData { ref mut remaining } => {
                    if !this.buf.is_empty() {
                        let to_take = (*remaining).min(this.buf.len());
                        let data = this.buf.split_to(to_take).freeze();
                        *remaining -= to_take;
                        if *remaining == 0 {
                            this.state = DecoderState::ReadingDataTrailer;
                        }
                        return Poll::Ready(Some(Ok(data)));
                    }
                }

                DecoderState::ReadingDataTrailer => {
                    if this.buf.len() >= 2 {
                        this.buf.advance(2);
                        this.state = DecoderState::ReadingHeader;
                        continue;
                    }
                }
            }

            // Poll for more data from the underlying body
            let body_pin = unsafe { Pin::new_unchecked(&mut this.body) };
            match http_body::Body::poll_frame(body_pin, cx) {
                Poll::Ready(Some(Ok(frame))) => {
                    if let Ok(data) = frame.into_data() {
                        this.buf.extend_from_slice(&data);
                    }
                    continue;
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{}", e),
                    ))));
                }
                Poll::Ready(None) => {
                    if this.buf.is_empty() {
                        this.state = DecoderState::Done;
                        return Poll::Ready(None);
                    }
                    continue;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    fn make_aws_chunked_body(chunks: &[(usize, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (size, data) in chunks {
            let header = format!("{:x};chunk-signature=test\r\n", size);
            out.extend_from_slice(header.as_bytes());
            out.extend_from_slice(data);
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(b"0;chunk-signature=test\r\n\r\n");
        out
    }

    #[tokio::test]
    async fn decode_single_chunk() {
        let raw = make_aws_chunked_body(&[(5, b"hello")]);
        let body = Body::from(raw);
        let decoded = decode_aws_chunked(body);
        let collected = decoded.collect().await.unwrap().to_bytes();
        assert_eq!(&collected[..], b"hello");
    }

    #[tokio::test]
    async fn decode_multiple_chunks() {
        let raw = make_aws_chunked_body(&[(5, b"hello"), (6, b" world")]);
        let body = Body::from(raw);
        let decoded = decode_aws_chunked(body);
        let collected = decoded.collect().await.unwrap().to_bytes();
        assert_eq!(&collected[..], b"hello world");
    }

    #[tokio::test]
    async fn decode_empty_body() {
        let raw = b"0;chunk-signature=test\r\n\r\n".to_vec();
        let body = Body::from(raw);
        let decoded = decode_aws_chunked(body);
        let collected = decoded.collect().await.unwrap().to_bytes();
        assert!(collected.is_empty());
    }

    #[tokio::test]
    async fn decode_large_chunk() {
        let data = vec![0x42u8; 1024];
        let raw = make_aws_chunked_body(&[(1024, &data)]);
        let body = Body::from(raw);
        let decoded = decode_aws_chunked(body);
        let collected = decoded.collect().await.unwrap().to_bytes();
        assert_eq!(collected.len(), 1024);
        assert!(collected.iter().all(|&b| b == 0x42));
    }

    #[tokio::test]
    async fn decode_streamed_chunks() {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(4);
        let body = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx));

        let raw = make_aws_chunked_body(&[(3, b"abc"), (3, b"def")]);

        tokio::spawn(async move {
            for chunk in raw.chunks(10) {
                tx.send(Ok(bytes::Bytes::copy_from_slice(chunk)))
                    .await
                    .unwrap();
            }
        });

        let decoded = decode_aws_chunked(body);
        let collected = decoded.collect().await.unwrap().to_bytes();
        assert_eq!(&collected[..], b"abcdef");
    }
}
