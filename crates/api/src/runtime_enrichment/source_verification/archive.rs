//! Download, check and unpack a verified source archive (task 0639).
//!
//! Verifiers publish the submitted source as one content-addressed archive
//! (`<sha256>.tar.gz` or `<sha256>.zip`) on their own storage. Before any
//! byte is shown: the URL must be https on the verifier's own host, the body
//! must hash to the record's `source_sha256`, and the unpacked project must
//! stay inside the caps below. Any miss is an error, never a partial result.

use std::io::Read;

use sha2::{Digest, Sha256};

use super::{Link, VerificationError};

/// Caps above anything published (measured 2026-10-14: largest archive
/// 376 KB packed, 297 files, 1.46 MB unpacked); they stop a hostile or
/// broken archive. The unpacked cap is what the response can carry: an AWS
/// Lambda response is limited to 6 MB, and the files travel as JSON text.
const MAX_ARCHIVE_BYTES: usize = 10 * 1024 * 1024;
pub(crate) const MAX_UNPACKED_BYTES: u64 = 4 * 1024 * 1024;
const MAX_FILES: usize = 2_000;

/// One file of the project. `text` is `None` for a file that is not UTF-8;
/// it stays listed so nothing silently disappears from the tree.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub size: u64,
    pub text: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive URL is not https on the verifier's host: {0}")]
    ForeignUrl(String),
    #[error("archive format not recognised from its URL: {0}")]
    UnknownFormat(String),
    #[error("archive is larger than {MAX_ARCHIVE_BYTES} bytes")]
    TooLarge,
    #[error("archive sha256 {actual} does not match the published {expected}")]
    ShaMismatch { expected: String, actual: String },
    #[error("archive exceeds the unpack caps ({MAX_FILES} files, {MAX_UNPACKED_BYTES} bytes)")]
    UnpackCapExceeded,
    #[error("archive is corrupt: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Download(#[from] VerificationError),
}

/// The format named by the URL's extension — the only place verifiers state
/// it: `tar.gz` or `zip`, as the API reports it.
pub fn format_of(url: &str) -> Result<&'static str, ArchiveError> {
    if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
        Ok("tar.gz")
    } else if url.ends_with(".zip") {
        Ok("zip")
    } else {
        Err(ArchiveError::UnknownFormat(url.to_owned()))
    }
}

/// The archive must sit on the verifier's own host and port, over https, named by a
/// domain — the URL comes from a third-party document, and the Lambda must
/// not fetch whatever it names. An IP literal is refused, as SEP-1 hosts are.
pub fn check_host(archive_url: &str, verifier_url: &str) -> Result<(), ArchiveError> {
    let (Ok(archive), Ok(verifier)) = (
        reqwest::Url::parse(archive_url),
        reqwest::Url::parse(verifier_url),
    ) else {
        return Err(ArchiveError::ForeignUrl(archive_url.to_owned()));
    };
    // `domain()` is `None` for an IP literal.
    let named = archive.domain().is_some();
    if archive.scheme() != "https"
        || !named
        || archive.host_str() != verifier.host_str()
        || archive.port_or_known_default() != verifier.port_or_known_default()
    {
        return Err(ArchiveError::ForeignUrl(archive_url.to_owned()));
    }
    Ok(())
}

/// The first published archive the Lambda may fetch, as `(url, format)`: a
/// known format on the verifier's own host. A record can list mirrors; one
/// we cannot use does not hide the next. `Ok(None)` when nothing is
/// published at all; the last refusal when nothing usable is.
pub fn pick(
    links: &[Link],
    verifier_url: &str,
) -> Result<Option<(String, &'static str)>, ArchiveError> {
    let mut last_err = None;
    for link in links {
        let format = match format_of(&link.url) {
            Ok(format) => format,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        if let Err(e) = check_host(&link.url, verifier_url) {
            last_err = Some(e);
            continue;
        }
        return Ok(Some((link.url.clone(), format)));
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(None),
    }
}

/// Download the archive, capped at [`MAX_ARCHIVE_BYTES`].
pub async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, ArchiveError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(VerificationError::from)?;
    if !resp.status().is_success() {
        return Err(VerificationError::Status(resp.status().as_u16()).into());
    }
    match super::read_capped(resp, MAX_ARCHIVE_BYTES).await {
        Ok(bytes) => Ok(bytes),
        Err(VerificationError::TooLarge(_)) => Err(ArchiveError::TooLarge),
        Err(other) => Err(ArchiveError::Download(other)),
    }
}

pub fn check_sha256(bytes: &[u8], expected_hex: &str) -> Result<(), ArchiveError> {
    let actual = hex::encode(Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected_hex) {
        return Err(ArchiveError::ShaMismatch {
            expected: expected_hex.to_owned(),
            actual,
        });
    }
    Ok(())
}

/// Unpack every regular file, sorted by path. Directories, links and other
/// entry types carry no source and are skipped.
pub fn unpack(bytes: &[u8], format: &str) -> Result<Vec<SourceFile>, ArchiveError> {
    let mut files = match format {
        "tar.gz" => unpack_tar_gz(bytes)?,
        "zip" => unpack_zip(bytes)?,
        other => return Err(ArchiveError::UnknownFormat(other.to_owned())),
    };
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn corrupt(e: impl std::fmt::Display) -> ArchiveError {
    ArchiveError::Corrupt(e.to_string())
}

fn unpack_tar_gz(bytes: &[u8]) -> Result<Vec<SourceFile>, ArchiveError> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in tar.entries().map_err(corrupt)? {
        let entry = entry.map_err(corrupt)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(corrupt)?
            .to_string_lossy()
            .into_owned();
        files.push(read_file(entry, path, &mut total, files.len())?);
    }
    Ok(files)
}

fn unpack_zip(bytes: &[u8]) -> Result<Vec<SourceFile>, ArchiveError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(corrupt)?;
    let mut files = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(corrupt)?;
        if !entry.is_file() {
            continue;
        }
        let path = entry.name().to_owned();
        files.push(read_file(entry, path, &mut total, files.len())?);
    }
    Ok(files)
}

/// Read one entry, counting against the caps as it goes: the read is bounded
/// by the remaining budget, so a lying size header cannot inflate memory.
fn read_file(
    entry: impl Read,
    path: String,
    total: &mut u64,
    count: usize,
) -> Result<SourceFile, ArchiveError> {
    if count >= MAX_FILES {
        return Err(ArchiveError::UnpackCapExceeded);
    }
    let budget = MAX_UNPACKED_BYTES - *total;
    let mut buf = Vec::new();
    entry
        .take(budget + 1)
        .read_to_end(&mut buf)
        .map_err(corrupt)?;
    let size = buf.len() as u64;
    if size > budget {
        return Err(ArchiveError::UnpackCapExceeded);
    }
    *total += size;
    Ok(SourceFile {
        path,
        size,
        text: String::from_utf8(buf).ok(),
    })
}

#[cfg(test)]
mod tests;
