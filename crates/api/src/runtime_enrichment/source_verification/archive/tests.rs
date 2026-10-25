use std::io::Write;

use super::*;

/// A `.tar.gz` holding `entries`, plus one directory entry that must be skipped.
fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(gz);
    let mut dir = tar::Header::new_gnu();
    dir.set_entry_type(tar::EntryType::Directory);
    dir.set_size(0);
    builder
        .append_data(&mut dir, "source/", std::io::empty())
        .unwrap();
    for (path, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        builder.append_data(&mut header, path, *body).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    w.add_directory("contracts/", opts).unwrap();
    for (path, body) in entries {
        w.start_file(*path, opts).unwrap();
        w.write_all(body).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[test]
fn unpacks_tar_gz_sorted_and_skips_directories() {
    let bytes = tar_gz(&[
        ("source/src/lib.rs", b"#![no_std]\n"),
        ("source/Cargo.toml", b"[package]\n"),
    ]);
    let files = unpack(&bytes, "tar.gz").unwrap();
    assert_eq!(
        files,
        vec![
            SourceFile {
                path: "source/Cargo.toml".into(),
                size: 10,
                text: Some("[package]\n".into())
            },
            SourceFile {
                path: "source/src/lib.rs".into(),
                size: 11,
                text: Some("#![no_std]\n".into())
            },
        ]
    );
}

#[test]
fn unpacks_zip_and_skips_directories() {
    let bytes = zip(&[("contracts/workers/dvn/src/lib.rs", b"pub mod dvn;\n")]);
    let files = unpack(&bytes, "zip").unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "contracts/workers/dvn/src/lib.rs");
    assert_eq!(files[0].text.as_deref(), Some("pub mod dvn;\n"));
}

#[test]
fn non_utf8_file_stays_listed_without_text() {
    let bytes = tar_gz(&[("source/logo.png", &[0x89, 0x50, 0xff, 0xfe])]);
    let files = unpack(&bytes, "tar.gz").unwrap();
    assert_eq!(files[0].path, "source/logo.png");
    assert_eq!(files[0].size, 4);
    assert_eq!(files[0].text, None);
}

#[test]
fn too_many_files_is_an_error() {
    let names: Vec<String> = (0..=MAX_FILES).map(|i| format!("f{i}")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
    let err = unpack(&zip(&entries), "zip").unwrap_err();
    assert!(matches!(err, ArchiveError::UnpackCapExceeded), "{err}");
}

#[test]
fn exactly_max_files_unpacks() {
    let names: Vec<String> = (0..MAX_FILES).map(|i| format!("f{i}")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
    assert_eq!(unpack(&zip(&entries), "zip").unwrap().len(), MAX_FILES);
}

#[test]
fn unpacked_bytes_over_the_cap_is_an_error() {
    // Zeros compress to almost nothing: a small archive that unpacks past the cap.
    let big = vec![0u8; MAX_UNPACKED_BYTES as usize + 1];
    let err = unpack(&tar_gz(&[("source/big", &big)]), "tar.gz").unwrap_err();
    assert!(matches!(err, ArchiveError::UnpackCapExceeded), "{err}");
}

#[test]
fn unpacked_bytes_at_the_cap_unpack() {
    let exact = vec![0u8; MAX_UNPACKED_BYTES as usize];
    let files = unpack(&tar_gz(&[("source/big", &exact)]), "tar.gz").unwrap();
    assert_eq!(files[0].size, MAX_UNPACKED_BYTES);
}

#[test]
fn garbage_is_corrupt_not_empty() {
    assert!(matches!(
        unpack(b"not an archive", "zip"),
        Err(ArchiveError::Corrupt(_))
    ));
    assert!(matches!(
        unpack(b"not an archive", "tar.gz"),
        Err(ArchiveError::Corrupt(_))
    ));
}

#[test]
fn sha256_must_match_case_insensitively() {
    // sha256("abc")
    let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    check_sha256(b"abc", abc).unwrap();
    check_sha256(b"abc", &abc.to_uppercase()).unwrap();
    let err = check_sha256(b"abd", abc).unwrap_err();
    assert!(matches!(err, ArchiveError::ShaMismatch { .. }), "{err}");
}

#[test]
fn format_comes_from_the_extension() {
    let base = "https://results.example/sources/ab";
    assert_eq!(format_of(&format!("{base}.tar.gz")).unwrap(), "tar.gz");
    assert_eq!(format_of(&format!("{base}.tgz")).unwrap(), "tar.gz");
    assert_eq!(format_of(&format!("{base}.zip")).unwrap(), "zip");
    assert!(matches!(
        format_of(&format!("{base}.rar")),
        Err(ArchiveError::UnknownFormat(_))
    ));
}

#[test]
fn archive_must_be_https_on_the_verifier_host() {
    let verifier = "https://results.staging.example";
    check_host("https://results.staging.example/sources/a.zip", verifier).unwrap();
    check_host(
        "https://results.staging.example:443/sources/a.zip",
        verifier,
    )
    .unwrap();
    for bad in [
        "https://results.staging.example:8443/sources/a.zip",
        "http://results.staging.example/sources/a.zip",
        "https://evil.example/sources/a.zip",
        "https://results.staging.example.evil.example/a.zip",
        "file:///etc/passwd",
        "not a url",
        "https://10.0.0.1/sources/a.zip",
    ] {
        assert!(
            matches!(check_host(bad, verifier), Err(ArchiveError::ForeignUrl(_))),
            "{bad} must be refused"
        );
    }
}

#[test]
fn a_realistic_project_size_unpacks() {
    // The largest archive published so far unpacks to 1.46 MB; 3 MB in one
    // file must fit comfortably under the caps.
    let body = vec![b'a'; 3 * 1024 * 1024];
    let files = unpack(&tar_gz(&[("source/big.rs", &body)]), "tar.gz").unwrap();
    assert_eq!(files[0].size, 3 * 1024 * 1024);
}

#[test]
fn the_unpack_cap_counts_every_file_together() {
    // Two files of 60% of the cap each: each fits alone, both do not.
    let part = vec![0u8; (MAX_UNPACKED_BYTES as usize) * 6 / 10];
    let err = unpack(
        &tar_gz(&[("source/a", &part), ("source/b", &part)]),
        "tar.gz",
    )
    .unwrap_err();
    assert!(matches!(err, ArchiveError::UnpackCapExceeded), "{err}");
}

mod download {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::super::*;

    async fn serving(status: u16, body: Vec<u8>) -> (MockServer, String) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sources/a.zip"))
            .respond_with(ResponseTemplate::new(status).set_body_bytes(body))
            .mount(&server)
            .await;
        let url = format!("{}/sources/a.zip", server.uri());
        (server, url)
    }

    #[tokio::test]
    async fn returns_the_body() {
        let (_s, url) = serving(200, b"PK archive bytes".to_vec()).await;
        let bytes = download(&reqwest::Client::new(), &url).await.unwrap();
        assert_eq!(bytes, b"PK archive bytes");
    }

    #[tokio::test]
    async fn a_body_up_to_the_cap_is_kept_whole() {
        let body = vec![7u8; MAX_ARCHIVE_BYTES];
        let (_s, url) = serving(200, body.clone()).await;
        assert_eq!(download(&reqwest::Client::new(), &url).await.unwrap(), body);
    }

    #[tokio::test]
    async fn a_body_over_the_cap_is_refused() {
        let (_s, url) = serving(200, vec![7u8; MAX_ARCHIVE_BYTES + 1]).await;
        let err = download(&reqwest::Client::new(), &url).await.unwrap_err();
        assert!(matches!(err, ArchiveError::TooLarge), "{err}");
    }

    #[tokio::test]
    async fn an_http_error_is_an_error() {
        let (_s, url) = serving(404, Vec::new()).await;
        let err = download(&reqwest::Client::new(), &url).await.unwrap_err();
        assert!(
            matches!(err, ArchiveError::Download(VerificationError::Status(404))),
            "{err}"
        );
    }

    #[test]
    fn the_caps_are_the_documented_ones() {
        // docs/architecture/backend/backend-overview.md states them.
        assert_eq!(MAX_ARCHIVE_BYTES, 10 * 1024 * 1024);
        assert_eq!(MAX_UNPACKED_BYTES, 4 * 1024 * 1024);
        assert_eq!(MAX_FILES, 2_000);
    }
}

#[test]
fn an_ip_literal_is_refused_even_as_the_verifier_host() {
    for (archive, verifier) in [
        ("https://10.0.0.1/a.zip", "https://10.0.0.1"),
        ("https://[::1]/a.zip", "https://[::1]"),
    ] {
        assert!(
            matches!(
                check_host(archive, verifier),
                Err(ArchiveError::ForeignUrl(_))
            ),
            "{archive}"
        );
    }
}

#[test]
fn pick_takes_the_first_usable_mirror() {
    let verifier = "https://results.staging.example";
    let links = |urls: &[&str]| -> Vec<Link> {
        urls.iter()
            .map(|u| Link {
                url: u.to_string(),
                kind: None,
            })
            .collect()
    };
    let mirrors = links(&[
        "https://mirror.example/a.zip",
        "https://results.staging.example/a.tar.zst",
        "https://results.staging.example/a.tar.gz",
        "https://results.staging.example/b.zip",
    ]);
    assert_eq!(
        pick(&mirrors, verifier).unwrap(),
        Some((
            "https://results.staging.example/a.tar.gz".to_string(),
            "tar.gz"
        ))
    );
    assert_eq!(pick(&[], verifier).unwrap(), None);
    assert!(pick(&links(&["https://mirror.example/a.zip"]), verifier).is_err());
}

/// A `.tar.gz` built entry by entry, for the kinds the plain helper lacks.
fn tar_gz_with(add: impl FnOnce(&mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>)) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(gz);
    add(&mut builder);
    builder.into_inner().unwrap().finish().unwrap()
}

#[test]
fn metadata_and_links_are_not_files() {
    // As `tar -x` leaves them: a pax global header (the demo contract's
    // archive holds one with the git commit id) and a link make no file.
    let bytes = tar_gz_with(|b| {
        let record = b"52 comment=7bd0765afdca5f99fbbec403d4b45cc3dbb05524\n";
        let mut global = tar::Header::new_ustar();
        global.set_entry_type(tar::EntryType::XGlobalHeader);
        global.set_size(record.len() as u64);
        b.append_data(&mut global, "pax_global_header", &record[..])
            .unwrap();
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        b.append_link(&mut link, "README.md", "../README.md")
            .unwrap();
        let mut file = tar::Header::new_ustar();
        file.set_size(2);
        file.set_mode(0o644);
        b.append_data(&mut file, "source/a.rs", &b"x\n"[..])
            .unwrap();
    });
    let files = unpack(&bytes, "tar.gz").unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "source/a.rs");
}

#[test]
fn a_zip_path_listed_twice_keeps_one_copy() {
    // Three published zips list each file 2-6 times, identical copies.
    // The zip writer refuses a repeated name, so the second name is
    // rewritten in the finished bytes (same length; names are not checksummed).
    let mut bytes = zip(&[("source/a.rs", b"one\n"), ("source/b.rs", b"one\n")]);
    let (from, to) = (b"source/b.rs", b"source/a.rs");
    let mut i = 0;
    while i + from.len() <= bytes.len() {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
        }
        i += 1;
    }
    let files = unpack(&bytes, "zip").unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "source/a.rs");
    assert_eq!(files[0].text.as_deref(), Some("one\n"));
}
