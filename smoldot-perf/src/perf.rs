use crate::Instant;
use crate::perf::PerfStreamInner::Negotiating;
use smoldot::libp2p::connection::multistream_select;
use smoldot::libp2p::read_write::ReadWrite;
use web_sys::console;

pub const PROTOCOL_NAME: &str = "/litep2p-perf/1.0.0";

pub(crate) struct PerfStream {
    pub upload_duration: Option<core::time::Duration>,
    pub download_duration: Option<core::time::Duration>,

    upload_bytes: u64,
    download_bytes: u64,
    inner: PerfStreamInner,
}

impl PerfStream {
    pub fn new(upload_bytes: u64, download_bytes: u64) -> Self {
        Self {
            upload_duration: None,
            download_duration: None,
            upload_bytes,
            download_bytes,
            inner: PerfStreamInner::new(),
        }
    }

    pub fn read_write(self, read_write: &mut ReadWrite<Instant>) -> Option<Self> {
        let upload_bytes = self.upload_bytes;
        let download_bytes = self.download_bytes;
        let mut upload_duration = self.upload_duration;
        let mut download_duration = self.download_duration;

        self.read_write2(read_write).map(|inner| {
            match inner {
                PerfStreamInner::Done(upload, download) => {
                    upload_duration = upload.into();
                    download_duration = download.into();
                }
                _ => {}
            }

            PerfStream {
                upload_duration,
                download_duration,
                upload_bytes,
                download_bytes,
                inner,
            }
        })
    }

    fn read_write2(self, read_write: &mut ReadWrite<Instant>) -> Option<PerfStreamInner> {
        match self.inner {
            Negotiating(nego) => {
                console::log_1(&"PerfStream::read_write2(): negotiating...".into());

                match nego.read_write(read_write) {
                    Ok(multistream_select::Negotiation::InProgress(nego)) => {
                        Some(Negotiating(nego))
                    }
                    Ok(multistream_select::Negotiation::Success) => {
                        console::log_1(&"PerfStream::read_write2(): done negotiating!".into());
                        read_write.wake_up_asap();
                        Some(PerfStreamInner::NumberOfBytesUpload)
                    }
                    Ok(multistream_select::Negotiation::NotAvailable) => None, // log?
                    Err(err) => {
                        console::log_1(&format!("kaput: {:?}", err).into());
                        None
                    }
                    _ => unreachable!("probably..."),
                }
            }
            PerfStreamInner::NumberOfBytesUpload => {
                console::log_1(
                    &"PerfStream::read_write2(): sending number of bytes, upload".into(),
                );
                read_write.write_out(Vec::from(self.upload_bytes.to_be_bytes()));
                read_write.wake_up_asap();
                Some(PerfStreamInner::BytesUpload(self.upload_bytes, None))
            }
            PerfStreamInner::BytesUpload(expected_bytes, mut started_at) => {
                if expected_bytes == self.upload_bytes {
                    console::log_1(
                        &format!(
                            "PerfStream::read_write2(): starting upload of {} bytes",
                            self.upload_bytes,
                        )
                        .into(),
                    );

                    started_at = Some(read_write.now);
                }

                let chunk_size: usize = match read_write.write_bytes_queueable {
                    Some(wbq) => {
                        if wbq == 0 {
                            console::log_1(
                                &"PerfStream::read_write2(): zero bytes queueable".into(),
                            );
                            return Some(PerfStreamInner::BytesUpload(expected_bytes, started_at));
                        }
                        std::cmp::min(wbq, 1024)
                    }
                    None => {
                        console::log_1(&"PerfStream::read_write2(): no bytes queueable".into());
                        return None;
                    }
                };

                read_write.write_out(vec![0u8; chunk_size]);
                read_write.wake_up_asap();

                let remaining_bytes = expected_bytes.saturating_sub(chunk_size as u64);
                if remaining_bytes > 0 {
                    Some(PerfStreamInner::BytesUpload(remaining_bytes, started_at))
                } else {
                    console::log_1(&"PerfStream::read_write2(): done uploading".into());
                    let upload_duration = read_write.now - started_at.unwrap();
                    Some(PerfStreamInner::NumberOfBytesDownload(upload_duration))
                }
            }
            PerfStreamInner::NumberOfBytesDownload(upload_duration) => {
                console::log_1(
                    &"PerfStream::read_write2(): sending number of bytes, download".into(),
                );
                read_write.write_out(Vec::from(self.download_bytes.to_be_bytes()));

                Some(PerfStreamInner::BytesDownload(
                    self.download_bytes,
                    upload_duration,
                    None,
                ))
            }
            PerfStreamInner::BytesDownload(expected_bytes, upload_duration, mut started_at) => {
                if expected_bytes == self.download_bytes {
                    console::log_1(
                        &format!(
                            "PerfStream::read_write2(): starting download of {} bytes",
                            self.download_bytes,
                        )
                        .into(),
                    );

                    started_at = Some(read_write.now);
                }

                let remaining_bytes =
                    expected_bytes.saturating_sub(read_write.incoming_buffer.len() as u64);
                if remaining_bytes > 0 {
                    read_write.discard_all_incoming();
                    Some(PerfStreamInner::BytesDownload(
                        remaining_bytes,
                        upload_duration,
                        started_at,
                    ))
                } else {
                    let download_duration = read_write.now - started_at.unwrap();

                    console::log_1(&format!(
                        "PerfStream::read_write2(): done downloading. upload duration: {:?} download duration: {:?}",
                        upload_duration,
                        download_duration,
                    ).into());

                    // This should cause WebRtcFraming to include the FIN flag in the outgoing message.
                    read_write.write_bytes_queueable = None;

                    Some(PerfStreamInner::Done(upload_duration, download_duration))
                }
            }
            PerfStreamInner::Done(_, _) => None,
        }
    }
}

enum PerfStreamInner {
    Negotiating(multistream_select::InProgress<String>),
    NumberOfBytesUpload,

    // expected bytes, instant when the upload started
    BytesUpload(u64, Option<Instant>),

    // upload duration
    NumberOfBytesDownload(core::time::Duration),

    // expected bytes, upload duration, instant when the download started
    BytesDownload(u64, core::time::Duration, Option<Instant>),

    // upload duration, download duration
    Done(core::time::Duration, core::time::Duration),
}

impl PerfStreamInner {
    fn new() -> Self {
        Negotiating(multistream_select::InProgress::new(
            multistream_select::Config::Dialer {
                requested_protocol: PROTOCOL_NAME.to_string(),
            },
        ))
    }
}
