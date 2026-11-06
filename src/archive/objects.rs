//! Private, immutable encrypted objects; no caller supplies a filesystem path.
use super::*;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct Objects {
    root: PathBuf,
    key: aead::LessSafeKey,
}

/// Erase reconstructed plaintext even when decoding fails.
pub struct Reconstruction {
    pub path: PathBuf,
    _file: std::fs::File,
    _cleanup: Option<Temporary>,
}
pub struct Temporary(pub PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.read(true).write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)
}

impl Objects {
    pub fn open(root: &Path, key: &[u8]) -> Res<Self> {
        if !root.is_absolute() || root.parent().is_none() {
            return Err("unavailable".into());
        }
        let mut b = std::fs::DirBuilder::new();
        b.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
        b.create(root)?;
        let m = std::fs::symlink_metadata(root)?;
        if !m.is_dir() {
            return Err("unavailable".into());
        }
        #[cfg(unix)]
        if std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o077 != 0 {
            return Err("unavailable".into());
        }
        let derived = ring::digest::digest(
            &ring::digest::SHA256,
            &[b"dashq archive objects v1\n".as_slice(), key].concat(),
        );
        let key = aead::UnboundKey::new(&aead::AES_256_GCM, derived.as_ref())
            .map_err(|_| Failure("unavailable"))?;
        Ok(Self {
            root: root.into(),
            key: aead::LessSafeKey::new(key),
        })
    }

    /// Separate persistence, existing connection/transaction authority. No archive data lives in dashq.db.
    pub fn index(&self, c: &Connection) -> Res<()> {
        let path = self.root.join("archive.db");
        let attached: Option<String> = c
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name='archive'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(file) = attached {
            if std::fs::canonicalize(file)? != std::fs::canonicalize(&path)? {
                return Err("unavailable".into());
            }
            return Ok(());
        }
        if !path.try_exists()? {
            match private_file(&path) {
                Ok(file) => file.sync_all()?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_file() {
            return Err("unavailable".into());
        }
        #[cfg(unix)]
        if std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()) & 0o077 != 0 {
            return Err("unavailable".into());
        }
        c.execute("ATTACH DATABASE ?1 AS archive", [path.to_string_lossy()])?;
        let setup = || -> Res<()> {
            if c.query_row("PRAGMA archive.journal_mode=WAL", [], |r| {
                r.get::<_, String>(0)
            })? != "wal"
            {
                return Err("unavailable".into());
            }
            c.execute_batch("PRAGMA archive.synchronous=FULL;")?;
            let version = c.query_row("PRAGMA archive.user_version", [], |r| r.get::<_, i64>(0))?;
            if version > 2 {
                return Err("unavailable".into());
            }
            c.execute_batch(INDEX)?;
            if version < 2 {
                migrate_index(c)?;
            }
            c.execute_batch("PRAGMA archive.user_version=2;")?;
            if c.query_row("PRAGMA archive.quick_check", [], |r| r.get::<_, String>(0))? != "ok" {
                return Err("unavailable".into());
            }
            Ok(())
        };
        if let Err(e) = setup() {
            let _ = c.execute_batch("DETACH DATABASE archive");
            return Err(e);
        }
        std::fs::File::open(&self.root)?.sync_all()?;
        Ok(())
    }

    fn path(&self, hash: &str) -> Res<PathBuf> {
        if !digest(hash) {
            return Err("unavailable".into());
        }
        Ok(self.root.join(hash))
    }

    fn temporary(&self, base: &Path) -> Res<(Temporary, std::fs::File)> {
        let mut random = [0; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| Failure("unavailable"))?;
        let tmp = Temporary(base.join(format!(".archive-{}", hex(&random))));
        let file = private_file(&tmp.0)?;
        Ok((tmp, file))
    }

    pub fn put(&self, hash: &str, bytes: &[u8]) -> Res<()> {
        let path = self.path(hash)?;
        if path.try_exists()? {
            self.read(hash)?;
            return Ok(());
        }
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(bytes)?;
        let mut encrypted = gzip.finish()?;
        let mut nonce = [0; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| Failure("unavailable"))?;
        self.key
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(hash.as_bytes()),
                &mut encrypted,
            )
            .map_err(|_| Failure("unavailable"))?;
        let (tmp, mut file) = self.temporary(&self.root)?;
        file.write_all(&nonce)?;
        file.write_all(&encrypted)?;
        file.sync_all()?;
        // The SQLite writer serializes ingest and GC; hard-link publication never replaces an existing object.
        std::fs::hard_link(&tmp.0, &path)?;
        std::fs::File::open(&self.root)?.sync_all()?;
        Ok(())
    }

    pub fn read(&self, hash: &str) -> Res<Vec<u8>> {
        let path = self.path(hash)?;
        let m = std::fs::symlink_metadata(&path)?;
        if !m.is_file() {
            return Err("unavailable".into());
        }
        let mut bytes = std::fs::read(path)?;
        if bytes.len() < 28 {
            return Err("unavailable".into());
        }
        let nonce: [u8; 12] = bytes[..12].try_into().map_err(|_| Failure("unavailable"))?;
        let clear = self
            .key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(hash.as_bytes()),
                &mut bytes[12..],
            )
            .map_err(|_| Failure("unavailable"))?;
        let mut out = Vec::new();
        GzDecoder::new(&*clear).read_to_end(&mut out)?;
        if hash_bytes(&out) != hash {
            return Err("unavailable".into());
        }
        Ok(out)
    }

    pub fn reconstruct(&self, bytes: &[u8]) -> Res<Reconstruction> {
        // Plaintext is outside the durable object root, so backups never include a reconstruction.
        let (tmp, mut file) = self.temporary(&std::env::temp_dir())?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            std::fs::remove_file(&tmp.0)?;
            file.write_all(bytes)?;
            let path = PathBuf::from(format!("/dev/fd/{}", file.as_raw_fd()));
            Ok(Reconstruction {
                path,
                _file: file,
                _cleanup: None,
            })
        }
        #[cfg(not(unix))]
        {
            file.write_all(bytes)?;
            Ok(Reconstruction {
                path: tmp.0.clone(),
                _file: file,
                _cleanup: Some(tmp),
            })
        }
    }

    pub fn collect(&self, c: &Connection) -> Res<()> {
        for file in std::fs::read_dir(&self.root)? {
            let file = file?;
            let name = file.file_name();
            let Some(name) = name.to_str() else { continue };
            // Only our immutable object namespace; unrelated files are never removed.
            if (name
                .strip_prefix(".archive-")
                .is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())))
                || (digest(name)
                    && !c.query_row(
                        "SELECT EXISTS(SELECT 1 FROM archive.archive_ranges WHERE hash=?1 AND retained=1)",
                        [name],
                        |r| r.get::<_, bool>(0),
                    )?)
            {
                std::fs::remove_file(file.path())?;
            }
        }
        std::fs::File::open(&self.root)?.sync_all()?;
        Ok(())
    }
}
