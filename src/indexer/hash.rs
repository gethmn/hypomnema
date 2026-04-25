use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

const HASH_BUFFER_BYTES: usize = 64 * 1024;

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("opening file for hashing: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; HASH_BUFFER_BYTES];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("reading from {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn hash_file_matches_known_sha256() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"hello world").unwrap();
        tmp.flush().unwrap();
        let hash = hash_file(tmp.path()).unwrap();
        assert_eq!(
            hash,
            "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn hash_file_handles_empty_file() {
        let tmp = NamedTempFile::new().unwrap();
        let hash = hash_file(tmp.path()).unwrap();
        assert_eq!(
            hash,
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hash_file_handles_multi_buffer_input() {
        // 200 KiB of zeros — exercises the read loop across multiple 64 KiB buffers.
        let mut tmp = NamedTempFile::new().unwrap();
        let payload = vec![0u8; 200 * 1024];
        tmp.write_all(&payload).unwrap();
        tmp.flush().unwrap();
        let hash = hash_file(tmp.path()).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let expected = format!("sha256:{:x}", hasher.finalize());
        assert_eq!(hash, expected);
    }

    #[test]
    fn hash_file_missing_path_errors() {
        let err = hash_file(Path::new("/no/such/path/should/exist/x.md")).unwrap_err();
        assert!(format!("{err:#}").contains("opening file for hashing"));
    }
}
