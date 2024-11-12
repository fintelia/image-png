use super::stream::{
    DecodeOptions, Decoded, DecodingError, FormatErrorInner, StreamingDecoder, CHUNK_BUFFER_SIZE,
};
use super::Limits;

use std::io::{BufRead, BufReader, ErrorKind, Read};

use crate::chunk;
use crate::common::Info;

/// Helper for encapsulating reading input from `Read` and feeding it into a `StreamingDecoder`
/// while hiding low-level `Decoded` events and only exposing a few high-level reading operations
/// like:
///
/// * `read_header_info` - reading until `IHDR` chunk
/// * `read_until_image_data` - reading until `IDAT` / `fdAT` sequence
/// * `decode_image_data` - reading from `IDAT` / `fdAT` sequence into `Vec<u8>`
/// * `finish_decoding_image_data()` - discarding remaining data from `IDAT` / `fdAT` sequence
/// * `read_until_end_of_input()` - reading until `IEND` chunk
pub(crate) struct ReadDecoder<R: Read> {
    reader: R,
    decoder: StreamingDecoder,
}

impl<R: BufRead> ReadDecoder<R> {
    pub fn new(r: R) -> Self {
        Self {
            reader: r,
            decoder: StreamingDecoder::new(),
        }
    }

    pub fn with_options(r: R, options: DecodeOptions) -> Self {
        let mut decoder = StreamingDecoder::new_with_options(options);
        decoder.limits = Limits::default();

        Self { reader: r, decoder }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.decoder.limits = limits;
    }

    pub fn reserve_bytes(&mut self, bytes: usize) -> Result<(), DecodingError> {
        self.decoder.limits.reserve_bytes(bytes)
    }

    pub fn set_ignore_text_chunk(&mut self, ignore_text_chunk: bool) {
        self.decoder.set_ignore_text_chunk(ignore_text_chunk);
    }

    pub fn set_ignore_iccp_chunk(&mut self, ignore_iccp_chunk: bool) {
        self.decoder.set_ignore_iccp_chunk(ignore_iccp_chunk);
    }

    pub fn ignore_checksums(&mut self, ignore_checksums: bool) {
        self.decoder.set_ignore_adler32(ignore_checksums);
        self.decoder.set_ignore_crc(ignore_checksums);
    }

    /// Reads until the end of `IHDR` chunk.
    ///
    /// Prerequisite: None (idempotent).
    pub fn read_header_info(&mut self) -> Result<&Info<'static>, DecodingError> {
        if self.decoder.info.is_none() {
            self.decoder.read_ihdr(&mut self.reader)?;
        }
        Ok(self.info().unwrap())
    }

    /// Reads until the start of the next `IDAT` or `fdAT` chunk.
    ///
    /// Prerequisite: **Not** within `IDAT` / `fdAT` chunk sequence.
    pub fn read_until_image_data(&mut self) -> Result<(), DecodingError> {
        self.decoder.read_metadata(&mut self.reader)
    }

    /// Reads `image_data` and reports whether there may be additional data afterwards (i.e. if it
    /// is okay to call `decode_image_data` and/or `finish_decoding_image_data` again)..
    ///
    /// Prerequisite: Input is currently positioned within `IDAT` / `fdAT` chunk sequence.
    pub fn decode_image_data(
        &mut self,
        image_data: &mut Vec<u8>,
    ) -> Result<ImageDataCompletionStatus, DecodingError> {
        self.decoder.read_image_data(&mut self.reader, image_data)
    }

    /// Consumes and discards the rest of an `IDAT` / `fdAT` chunk sequence.
    ///
    /// Prerequisite: Input is currently positioned within `IDAT` / `fdAT` chunk sequence.
    pub fn finish_decoding_image_data(&mut self) -> Result<(), DecodingError> {
        loop {
            let mut to_be_discarded = vec![];
            if let ImageDataCompletionStatus::Done = self.decode_image_data(&mut to_be_discarded)? {
                return Ok(());
            }
        }
    }

    /// Reads until the `IEND` chunk.
    ///
    /// Prerequisite: `IEND` chunk hasn't been reached yet.
    pub fn read_until_end_of_input(&mut self) -> Result<(), DecodingError> {
        self.decoder.read_until_end_of_input(&mut self.reader)
    }

    pub fn info(&self) -> Option<&Info<'static>> {
        self.decoder.info.as_ref()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ImageDataCompletionStatus {
    ExpectingMoreData,
    Done,
}
