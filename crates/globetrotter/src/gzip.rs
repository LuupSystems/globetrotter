use flate2::{Compression, write::GzEncoder};
use std::io::Write;

pub fn gzipped_size(value: impl AsRef<[u8]>) -> std::io::Result<usize> {
    #[derive(Debug, Default)]
    struct ByteCounter(pub usize);

    impl std::io::Write for ByteCounter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let writer = ByteCounter::default();
    let mut compressor = GzEncoder::new(writer, Compression::best());
    compressor.write_all(value.as_ref())?;
    let compressed_bytes = compressor.finish()?;
    Ok(compressed_bytes.0)
}
