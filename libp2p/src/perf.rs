use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const PROTOCOL_NAME: &str = "/litep2p-perf/1.0.0";
const LOG_TARGET: &str = "litep2p-perf";

async fn read_u64<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    substream: &mut S,
) -> Result<u64, std::io::Error> {
    let mut buf = [0u8; 8];
    substream.read_exact(&mut buf).await?;
    Ok(u64::from_be_bytes(buf))
}

async fn write_u64<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    substream: &mut S,
    value: u64,
) -> Result<(), std::io::Error> {
    substream.write_all(&value.to_be_bytes()).await?;
    substream.flush().await?;
    Ok(())
}

async fn recv_bytes<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    substream: &mut S,
    to_recv: u64,
) -> Result<(), std::io::Error> {
    let mut buf = vec![0u8; 1024];
    let mut total = 0;
    while total < to_recv {
        let n = substream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        total += n as u64;
    }
    Ok(())
}

async fn send_bytes<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    substream: &mut S,
    to_send: u64,
) -> Result<(), std::io::Error> {
    let buf = vec![0u8; 1024];
    let mut total = 0;
    while total < to_send {
        substream.write_all(&buf).await?;
        substream.flush().await?;
        total += buf.len() as u64;
    }
    Ok(())
}

pub async fn server_mode<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut substream: S,
) -> Result<(), std::io::Error> {
    // Step 1. Read the download bytes.
    let to_recv = read_u64(&mut substream).await?;
    // Step 2. Receive the download bytes.
    recv_bytes(&mut substream, to_recv).await?;

    // Step 3. Read the upload bytes.
    let to_send = read_u64(&mut substream).await?;
    // Step 4. Send the upload bytes.
    send_bytes(&mut substream, to_send).await?;

    Ok(())
}

pub async fn client_mode<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut substream: S,
    upload_bytes: u64,
    download_bytes: u64,
) -> Result<(), std::io::Error> {
    // Step 1. Send the upload bytes.
    write_u64(&mut substream, upload_bytes).await?;
    // Step 2. Send the upload bytes.
    let now = std::time::Instant::now();
    send_bytes(&mut substream, upload_bytes).await?;
    let elapsed = now.elapsed();
    tracing::info!(
        target: LOG_TARGET,
        "Uploaded {} bytes in {:.4}s bandwidth {}",
        utils::format_bytes(upload_bytes as usize),
        elapsed.as_secs_f64(),
        utils::format_bandwidth(elapsed, upload_bytes as usize)
    );
    // Step 3. Send the download bytes.
    write_u64(&mut substream, download_bytes).await?;
    // Step 4. Receive the download bytes.
    let now = std::time::Instant::now();
    recv_bytes(&mut substream, download_bytes).await?;
    let elapsed = now.elapsed();
    tracing::info!(
        target: LOG_TARGET,
        "Downloaded {} bytes in {:.4}s bandwidth {}",
        utils::format_bytes(download_bytes as usize),
        elapsed.as_secs_f64(),
        utils::format_bandwidth(elapsed, download_bytes as usize)
    );

    Ok(())
}
